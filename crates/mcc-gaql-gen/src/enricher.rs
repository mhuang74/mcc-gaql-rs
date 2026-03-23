//
// Metadata enricher: uses an LLM to generate contextual descriptions for Google Ads
// fields, merging structural data from the Fields Service with any scraped documentation.
//
// Design:
// - Groups fields by resource and sends batched prompts to the LLM
// - Each prompt covers ~15 fields to stay within token limits and get cross-field context
// - Responses are JSON objects mapping field names to descriptions
// - Falls back gracefully: if the LLM call fails, the field keeps its existing description
// - Enriched descriptions update FieldMetadata.description and FieldMetadata.usage_notes

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use rig::completion::Prompt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache, ResourceMetadata};

use crate::rag::{format_llm_request_debug, format_llm_response_debug};

use crate::model_pool::{ModelLease, ModelPool};
use crate::scraper::ScrapedDocs;

/// Retry an async operation with exponential backoff and jitter.
/// Base delays: 1s, 2s, 4s (doubling each retry) with ±50% random jitter
/// to prevent thundering herd when multiple concurrent tasks fail together.
async fn retry_with_backoff<T, E, Fut, F>(
    operation_name: &str,
    max_retries: u32,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    use rand::Rng;

    let mut attempt = 0;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempt += 1;
                if attempt > max_retries {
                    log::warn!(
                        "{} failed after {} retries: {}",
                        operation_name,
                        max_retries,
                        e
                    );
                    return Err(e);
                }
                // Base delay in milliseconds: 1000, 2000, 4000
                let base_delay_ms = 1000u64 << (attempt - 1);
                // Add jitter: ±50% of base delay
                let jitter_range = base_delay_ms / 2;
                let jitter =
                    rand::thread_rng().gen_range(0..=jitter_range * 2) as i64 - jitter_range as i64;
                let delay_ms = (base_delay_ms as i64 + jitter).max(100) as u64;
                log::info!(
                    "{} failed (attempt {}), retrying in {}ms: {}",
                    operation_name,
                    attempt,
                    delay_ms,
                    e
                );
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// LLM-based enricher for Google Ads field metadata
pub struct MetadataEnricher {
    model_pool: Arc<ModelPool>,
    /// Maximum fields per LLM batch (controls token usage)
    batch_size: usize,
}

impl MetadataEnricher {
    /// Create a new enricher backed by the given model pool.
    pub fn new(model_pool: Arc<ModelPool>) -> Self {
        Self {
            model_pool,
            batch_size: 15,
        }
    }

    /// Override the number of fields sent per LLM batch (default: 15).
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Enrich all fields in the cache with LLM-generated descriptions.
    /// Also enriches resource-level metadata.
    /// Uses concurrent processing across all models in the pool.
    /// Modifies the cache in place.
    pub async fn enrich(
        &self,
        cache: &mut FieldMetadataCache,
        scraped: &ScrapedDocs,
    ) -> Result<()> {
        let resources = cache.get_resources();
        let total_resources = resources.len();
        let concurrency = self.model_pool.total_concurrency();

        log::info!(
            "Starting LLM enrichment for {} resources ({} fields total, concurrency: {})",
            total_resources,
            cache.fields.len(),
            concurrency
        );

        // Wrap scraped docs in Arc for sharing across concurrent tasks
        let scraped = Arc::new(scraped.clone());
        let model_pool = Arc::clone(&self.model_pool);

        // Collect all batches across all resources for parallel processing
        let mut all_batches: Vec<(String, Vec<FieldMetadata>)> = Vec::new();

        for resource in &resources {
            let resource_field_names: Vec<String> = cache
                .get_resource_fields(resource)
                .iter()
                .map(|f| f.name.clone())
                .collect();

            if resource_field_names.is_empty() {
                continue;
            }

            for batch in resource_field_names.chunks(self.batch_size) {
                let batch_fields: Vec<FieldMetadata> = batch
                    .iter()
                    .filter_map(|name| cache.fields.get(name).cloned())
                    .collect();

                if !batch_fields.is_empty() {
                    all_batches.push((resource.clone(), batch_fields));
                }
            }
        }

        let total_batches = all_batches.len();
        log::info!(
            "Processing {} batches with concurrency {}",
            total_batches,
            concurrency
        );

        // Process batches concurrently using buffer_unordered(model_count).
        // Each task acquires a model lease from the pool so that at most one
        // request is in-flight per model at any time.
        let results: Vec<_> = stream::iter(all_batches.into_iter().enumerate())
            .map(|(idx, (resource, batch_fields))| {
                let pool = Arc::clone(&model_pool);
                let scraped = Arc::clone(&scraped);
                async move {
                    // Acquire a model lease (waits if all models are busy)
                    let lease = pool.acquire().await;

                    log::info!(
                        "[{}/{}] Enriching batch for resource: {} ({} fields) using model '{}'",
                        idx + 1,
                        total_batches,
                        resource,
                        batch_fields.len(),
                        lease.model_name()
                    );

                    let result =
                        retry_with_backoff(&format!("Batch {} ({})", idx + 1, resource), 3, || {
                            Self::enrich_batch_with_lease(
                                &lease,
                                &resource,
                                &batch_fields,
                                &scraped,
                            )
                        })
                        .await;

                    // lease dropped here, model slot released
                    match &result {
                        Ok(descriptions) => {
                            log::info!(
                                "  Batch {}: enriched {}/{} fields",
                                idx + 1,
                                descriptions.len(),
                                batch_fields.len()
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "  Batch {} failed for resource '{}': {}",
                                idx + 1,
                                resource,
                                e
                            );
                        }
                    }

                    result
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // Apply all results to the cache
        for descriptions in results.into_iter().flatten() {
            for (field_name, (description, usage_notes)) in descriptions {
                if let Some(field) = cache.fields.get_mut(&field_name) {
                    if !description.is_empty() {
                        field.description = Some(description);
                    }
                    if let Some(notes) = usage_notes
                        && !notes.is_empty()
                    {
                        field.usage_notes = Some(notes);
                    }
                }
            }
        }

        // Stage 3: Key field selection per resource (run before resource description enrichment)
        log::info!("Selecting key fields for {} resources", resources.len());

        // Process key field selection concurrently using buffer_unordered(concurrency).
        let model_pool = Arc::clone(&self.model_pool);
        let key_field_results: Vec<_> = stream::iter(resources.iter())
            .map(|resource| {
                let pool = Arc::clone(&model_pool);
                let resource = resource.clone();
                // Need to pass cache data as read-only
                let resource_attrs = cache
                    .get_resource_fields(&resource)
                    .iter()
                    .filter(|f| f.is_attribute())
                    .map(|f| f.name.clone())
                    .collect::<Vec<String>>();
                let selectable_with = cache.get_resource_selectable_with(&resource);
                let resource_metrics = selectable_with
                    .iter()
                    .filter(|f| f.starts_with("metrics."))
                    .cloned()
                    .collect::<Vec<String>>();
                async move {
                    let lease = pool.acquire().await;
                    // Call static helper that takes lease + data
                    retry_with_backoff(&format!("Key fields for {}", resource), 3, || {
                        Self::select_key_fields_with_lease(
                            &lease,
                            &resource,
                            &resource_attrs,
                            &resource_metrics,
                        )
                    })
                    .await
                    .map(|result| (resource, result))
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // Apply results to cache
        for result in key_field_results.into_iter().flatten() {
            let (resource, (key_attrs, key_mets, uses_fallback)) = result;
            if let Some(rm) = cache
                .resource_metadata
                .as_mut()
                .and_then(|m| m.get_mut(&resource))
            {
                rm.key_attributes = key_attrs;
                rm.key_metrics = key_mets;
                rm.uses_fallback = uses_fallback;
            }
        }

        // Enrich resource-level metadata using concurrent processing
        log::info!(
            "Enriching resource-level metadata for {} resources",
            resources.len()
        );

        // Process resource description enrichment concurrently using buffer_unordered(concurrency).
        let resource_desc_results: Vec<_> = stream::iter(resources.iter())
            .map(|resource| {
                let pool = Arc::clone(&model_pool);
                let scraped = Arc::clone(&scraped);
                let resource = resource.clone();
                // Extract needed ResourceMetadata fields before async block
                let rm_data = cache
                    .resource_metadata
                    .as_ref()
                    .and_then(|m| m.get(&resource))
                    .cloned();
                async move {
                    if let Some(rm) = rm_data {
                        let lease = pool.acquire().await;
                        retry_with_backoff(&format!("Resource desc for {}", resource), 3, || {
                            Self::enrich_resource_with_lease(&lease, &resource, &rm, &scraped)
                        })
                        .await
                        .map(|desc| (resource, desc))
                        .ok()
                    } else {
                        None
                    }
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // Apply results to cache
        for result in resource_desc_results.into_iter().flatten() {
            let (resource, desc) = result;
            if !desc.is_empty() {
                if let Some(rm) = cache
                    .resource_metadata
                    .as_mut()
                    .and_then(|m| m.get_mut(&resource))
                {
                    rm.description = Some(desc);
                }
            }
        }

        let enriched = cache.enriched_field_count();
        log::info!(
            "LLM enrichment complete: {}/{} fields enriched",
            enriched,
            cache.fields.len()
        );

        Ok(())
    }

    /// Enrich a batch of fields using the model referenced by `lease`.
    async fn enrich_batch_with_lease(
        lease: &ModelLease,
        resource: &str,
        fields: &[FieldMetadata],
        scraped: &ScrapedDocs,
    ) -> Result<HashMap<String, (String, Option<String>)>> {
        let system_prompt = "\
You are a Google Ads API documentation expert. Your task is to write concise, \
technically accurate field descriptions optimized for use in a semantic search \
(RAG) system that helps generate GAQL queries.\n\
\n\
Your descriptions will be embedded as vectors and matched against user queries \
like \"show campaign names\", \"filter by status\", \"get impression metrics\". \
Make descriptions dense with relevant terms a user might use.\n\
\n\
Respond ONLY with a valid JSON object. No explanation, no markdown, no code blocks.\n\
Keys are field names. Each value is an object with:\n\
  \"description\": 1-2 sentence explanation of what the field represents and when to use it\n\
  \"usage_notes\": brief notes on filtering, sorting, or common patterns (optional, omit if nothing notable)\n\
\n\
Example:\n\
{\n\
  \"campaign.name\": {\n\
    \"description\": \"The display name of the campaign as shown in the Google Ads UI. \
Use in SELECT to label rows in reports.\",\n\
    \"usage_notes\": \"Filterable with = and LIKE operators. Sortable.\"\n\
  }\n\
}";

        let user_prompt = Self::build_batch_prompt_static(resource, fields, scraped);

        log::debug!(
            "Enriching {} fields (model={}, temp={})",
            fields.len(),
            lease.model_name(),
            lease.temperature()
        );
        log::trace!(
            "{}",
            format_llm_request_debug(&Some(system_prompt.to_string()), &user_prompt)
        );

        let agent = lease
            .create_agent(system_prompt)
            .context("Failed to create LLM agent for enrichment")?;

        let llm_start = std::time::Instant::now();
        let response = agent
            .prompt(&user_prompt)
            .await
            .map_err(|e| anyhow::anyhow!("LLM prompt failed: {}", e))?;
        log::debug!(
            "Enrichment LLM (model={}) responded in {}ms",
            lease.model_name(),
            llm_start.elapsed().as_millis()
        );
        log::trace!("{}", format_llm_response_debug(&response));

        Self::parse_enrichment_response_static(&response)
    }

    /// Static version of build_batch_prompt for use in concurrent contexts
    fn build_batch_prompt_static(
        resource: &str,
        fields: &[FieldMetadata],
        scraped: &ScrapedDocs,
    ) -> String {
        let mut prompt = format!(
            "Generate descriptions for these Google Ads API fields from the '{}' resource:\n\n",
            resource
        );

        for field in fields {
            prompt.push_str(&format!("Field: {}\n", field.name));
            prompt.push_str(&format!(
                "  Category: {}, DataType: {}\n",
                field.category, field.data_type
            ));

            let flags: Vec<&str> = [
                if field.selectable {
                    Some("selectable")
                } else {
                    None
                },
                if field.filterable {
                    Some("filterable")
                } else {
                    None
                },
                if field.sortable {
                    Some("sortable")
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            if !flags.is_empty() {
                prompt.push_str(&format!("  Flags: {}\n", flags.join(", ")));
            }

            if !field.enum_values.is_empty() {
                let values: Vec<&str> = field
                    .enum_values
                    .iter()
                    .take(20)
                    .map(String::as_str)
                    .collect();
                prompt.push_str(&format!("  Enum values: {}\n", values.join(", ")));
            }

            // Get full proto doc info if available (for proto-based enrichment)
            if let Some(proto_doc) = scraped.docs.get(&field.name) {
                // Proto type info
                if !proto_doc.proto_type.is_empty() {
                    prompt.push_str(&format!("  Proto type: {}\n", proto_doc.proto_type));
                }

                // Field behavior (OUTPUT_ONLY, REQUIRED, etc.)
                if !proto_doc.field_behavior.is_empty() {
                    prompt.push_str(&format!(
                        "  Field behavior: {}\n",
                        proto_doc.field_behavior.join(", ")
                    ));
                }

                // Description
                if !proto_doc.description.is_empty() {
                    prompt.push_str(&format!("  Documentation: {}\n", proto_doc.description));
                }

                // Enum value descriptions from proto (richer than just names)
                if !proto_doc.enum_value_descriptions.is_empty() {
                    let descs: Vec<&str> = proto_doc
                        .enum_value_descriptions
                        .iter()
                        .take(10)
                        .map(String::as_str)
                        .collect();
                    if !descs.is_empty() {
                        prompt.push_str(&format!("  Enum descriptions: {}\n", descs.join("; ")));
                    }
                }
            }

            prompt.push('\n');
        }

        prompt.push_str("\nRespond with JSON only:");
        prompt
    }

    /// Static version of parse_enrichment_response for use in concurrent contexts
    fn parse_enrichment_response_static(
        response: &str,
    ) -> Result<HashMap<String, (String, Option<String>)>> {
        let cleaned = strip_json_fences(response);

        let parsed: Value =
            serde_json::from_str(&cleaned).context("LLM returned invalid JSON for enrichment")?;

        let obj = parsed
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("LLM enrichment response is not a JSON object"))?;

        let mut result = HashMap::new();

        for (field_name, value) in obj {
            match value {
                Value::Object(field_obj) => {
                    let description = field_obj
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let usage_notes = field_obj
                        .get("usage_notes")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    result.insert(field_name.clone(), (description, usage_notes));
                }
                Value::String(s) => {
                    result.insert(field_name.clone(), (s.clone(), None));
                }
                _ => {
                    log::debug!(
                        "Unexpected JSON value type for field '{}', skipping",
                        field_name
                    );
                }
            }
        }

        Ok(result)
    }

    /// Generate a description for a resource (not a field) using a pre-acquired lease
    async fn enrich_resource_with_lease(
        lease: &ModelLease,
        resource_name: &str,
        rm: &ResourceMetadata,
        scraped: &ScrapedDocs,
    ) -> Result<String> {
        let system_prompt = "\
You are a Google Ads API expert. Write a single concise sentence (max 20 words) \
describing what a Google Ads API resource represents and what it is typically \
used to query. Return ONLY the sentence, no formatting.";

        let mut user_prompt = format!(
            "Describe the Google Ads API resource: '{}'\n",
            resource_name
        );
        user_prompt.push_str(&format!("Fields: {}\n", rm.field_count));

        if !rm.key_attributes.is_empty() {
            user_prompt.push_str(&format!(
                "Key attributes: {}\n",
                rm.key_attributes
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !rm.key_metrics.is_empty() {
            user_prompt.push_str(&format!(
                "Key metrics: {}\n",
                rm.key_metrics
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if let Some(scraped_desc) = scraped.get_description(resource_name) {
            user_prompt.push_str(&format!("Documentation: {}\n", scraped_desc));
        }

        log::debug!(
            "Enriching resource {} (model={}, temp={})",
            resource_name,
            lease.model_name(),
            lease.temperature()
        );
        log::trace!(
            "{}",
            format_llm_request_debug(&Some(system_prompt.to_string()), &user_prompt)
        );

        let agent = lease
            .create_agent(system_prompt)
            .context("Failed to create LLM agent for resource enrichment")?;

        let llm_start = std::time::Instant::now();
        let response = agent.prompt(&user_prompt).await.map_err(|e| {
            anyhow::anyhow!("LLM prompt failed for resource {}: {}", resource_name, e)
        })?;
        log::debug!(
            "Resource enrichment LLM (model={}) responded in {}ms",
            lease.model_name(),
            llm_start.elapsed().as_millis()
        );
        log::trace!("{}", format_llm_response_debug(&response));

        Ok(response.trim().to_string())
    }

    /// Select key attributes and metrics for a resource using LLM with a pre-acquired lease
    /// Returns (key_attributes, key_metrics, uses_fallback) or falls back to alphabetical first-N on failure
    async fn select_key_fields_with_lease(
        lease: &ModelLease,
        resource: &str,
        resource_attrs: &[String],
        resource_metrics: &[String],
    ) -> Result<(Vec<String>, Vec<String>, bool)> {
        if resource_attrs.is_empty() && resource_metrics.is_empty() {
            return Err(anyhow::anyhow!(
                "No attributes or metrics found for resource '{}'",
                resource
            ));
        }

        // Build prompt for LLM
        let system_prompt = "\
You are a Google Ads API expert. Given a list of GAQL field names, select the most \
commonly useful ones for typical reporting queries. Return ONLY valid JSON with two keys:\n\
- \"key_attributes\": array of 5-10 attribute field names (e.g., campaign.name, ad_group.status)\n\
- \"key_metrics\": array of 7-12 metric field names (e.g., metrics.clicks, metrics.impressions)\n\
\nSelect fields that are most commonly used in everyday Google Ads reporting. \
Do NOT include fields that are rarely used or very specialized.";

        let mut user_prompt = format!(
            "For the Google Ads resource '{}', select the most useful fields:\n\n",
            resource
        );

        if !resource_attrs.is_empty() {
            user_prompt.push_str(&format!(
                "Available attributes ({} total):\n{}\n\n",
                resource_attrs.len(),
                resource_attrs.join(", ")
            ));
        }

        if !resource_metrics.is_empty() {
            user_prompt.push_str(&format!(
                "Available metrics ({} total):\n{}\n\n",
                resource_metrics.len(),
                resource_metrics.join(", ")
            ));
        }

        user_prompt.push_str("Return JSON: {\"key_attributes\": [...], \"key_metrics\": [...]}");
        log::debug!(
            "Selecting key fields for {} (model={}, temp={})",
            resource,
            lease.model_name(),
            lease.temperature()
        );
        log::trace!(
            "{}",
            format_llm_request_debug(&Some(system_prompt.to_string()), &user_prompt)
        );

        let agent = lease
            .create_agent(system_prompt)
            .context("Failed to create LLM agent for key field selection")?;

        let llm_start = std::time::Instant::now();
        let response = agent.prompt(&user_prompt).await.map_err(|e| {
            anyhow::anyhow!(
                "LLM prompt failed for key field selection on {}: {}",
                resource,
                e
            )
        })?;
        log::debug!(
            "Key field selection LLM (model={}) responded in {}ms",
            lease.model_name(),
            llm_start.elapsed().as_millis()
        );
        log::trace!("{}", format_llm_response_debug(&response));

        // Parse JSON response (strip markdown fences first)
        let cleaned_response = strip_json_fences(&response);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned_response)
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response as JSON: {}", e))?;

        let mut key_attributes: Vec<String> = parsed
            .get("key_attributes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| resource_attrs.contains(s))
                    .take(10)
                    .collect()
            })
            .unwrap_or_default();

        let mut key_metrics: Vec<String> = parsed
            .get("key_metrics")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| resource_metrics.contains(s))
                    .take(12)
                    .collect()
            })
            .unwrap_or_default();

        // Fallback: if LLM returned nothing valid, use alphabetical first-N
        let mut uses_fallback = false;
        if key_attributes.is_empty() && !resource_attrs.is_empty() {
            let mut sorted_attrs = resource_attrs.to_vec();
            sorted_attrs.sort();
            key_attributes = sorted_attrs.into_iter().take(10).collect();
            uses_fallback = true;
        }

        if key_metrics.is_empty() && !resource_metrics.is_empty() {
            let mut sorted_metrics = resource_metrics.to_vec();
            sorted_metrics.sort();
            key_metrics = sorted_metrics.into_iter().take(12).collect();
            uses_fallback = true;
        }

        Ok((key_attributes, key_metrics, uses_fallback))
    }
}

/// Strip markdown code fences from a JSON string (LLM sometimes wraps output in ```json ... ```)
fn strip_json_fences(s: &str) -> String {
    let s = s.trim();

    let s = if s.starts_with("```json") {
        s.trim_start_matches("```json")
    } else if s.starts_with("```") {
        s.trim_start_matches("```")
    } else {
        s
    };

    let s = if s.ends_with("```") {
        s.trim_end_matches("```")
    } else {
        s
    };

    s.trim().to_string()
}

/// Run the full enrichment pipeline:
/// 1. Load or scrape documentation from the web
/// 2. Use LLM to synthesize descriptions for every field
/// 3. Enrich resource-level metadata
///
/// Returns the modified cache (caller is responsible for saving to disk).
pub async fn run_enrichment_pipeline(
    cache: &mut FieldMetadataCache,
    model_pool: Arc<ModelPool>,
    scrape_cache_path: &std::path::Path,
    scrape_ttl_days: i64,
    scrape_delay_ms: u64,
) -> Result<()> {
    let resources = cache.get_resources();

    // Stage 1: Web scraping
    println!(
        "Stage 1/3: Scraping Google Ads API reference docs for {} resources...",
        resources.len()
    );
    let scraped = crate::scraper::ScrapedDocs::load_or_scrape(
        &resources,
        &cache.api_version,
        scrape_cache_path,
        scrape_ttl_days,
        scrape_delay_ms,
    )
    .await
    .context("Failed to scrape Google Ads API reference docs")?;

    println!(
        "  Scraped {} resources, collected {} field docs",
        scraped.resources_scraped,
        scraped.docs.len()
    );

    // Stage 2: LLM enrichment
    println!(
        "Stage 2/3: Generating LLM descriptions for {} fields...",
        cache.fields.len()
    );
    let enricher = MetadataEnricher::new(model_pool);
    enricher.enrich(cache, &scraped).await?;

    println!(
        "  Enriched {}/{} fields",
        cache.enriched_field_count(),
        cache.fields.len()
    );

    // Stage 3: Key field selection (integrated in enrich())
    println!(
        "Stage 3/3: Key field selection complete for {} resources",
        resources.len()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_json_fences_clean() {
        let json = r#"{"campaign.name": {"description": "test"}}"#;
        assert_eq!(strip_json_fences(json), json);
    }

    #[test]
    fn test_strip_json_fences_with_backticks() {
        let json = "```json\n{\"campaign.name\": {\"description\": \"test\"}}\n```";
        let stripped = strip_json_fences(json);
        assert!(stripped.starts_with('{'));
        assert!(stripped.ends_with('}'));
    }

    #[test]
    fn test_parse_enrichment_response_object_format() {
        let response = r#"{
            "campaign.name": {
                "description": "The name of the campaign.",
                "usage_notes": "Filterable with = and LIKE."
            },
            "campaign.status": {
                "description": "Current serving status.",
                "usage_notes": "Filter with = for active campaigns."
            }
        }"#;

        let result = MetadataEnricher::parse_enrichment_response_static(response).unwrap();
        assert_eq!(result.len(), 2);

        let (desc, notes) = result.get("campaign.name").unwrap();
        assert_eq!(desc, "The name of the campaign.");
        assert_eq!(notes.as_deref(), Some("Filterable with = and LIKE."));
    }

    #[test]
    fn test_parse_enrichment_response_string_format() {
        let response = r#"{"campaign.name": "The name of the campaign."}"#;
        let result = MetadataEnricher::parse_enrichment_response_static(response).unwrap();
        assert_eq!(result.len(), 1);
        let (desc, notes) = result.get("campaign.name").unwrap();
        assert_eq!(desc, "The name of the campaign.");
        assert!(notes.is_none());
    }

    #[test]
    fn test_parse_enrichment_response_invalid_json() {
        let result = MetadataEnricher::parse_enrichment_response_static("not json");
        assert!(result.is_err());
    }
}
