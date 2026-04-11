use anyhow::{Context, Result, anyhow};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::path::Path;

use googleads_rs::google::ads::googleads::v23::services::SearchGoogleAdsFieldsRequest;
use googleads_rs::google::ads::googleads::v23::services::google_ads_field_service_client::GoogleAdsFieldServiceClient;

use crate::googleads::GoogleAdsAPIAccess;

// Re-export the common types so callers can use crate::field_metadata::FieldMetadata etc.
pub use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache, ResourceMetadata};

/// Load cache from file or fetch from API if stale/missing
pub async fn load_or_fetch(
    api_context: Option<&GoogleAdsAPIAccess>,
    cache_path: &Path,
    max_age_days: i64,
) -> Result<FieldMetadataCache> {
    // Try to load from cache
    if cache_path.exists() {
        match FieldMetadataCache::load_from_disk(cache_path).await {
            Ok(cache) => {
                let age = Utc::now() - cache.last_updated;
                if age < Duration::days(max_age_days) {
                    log::info!(
                        "Loaded field metadata cache from {:?} (age: {} days)",
                        cache_path,
                        age.num_days()
                    );
                    return Ok(cache);
                } else {
                    log::info!(
                        "Field metadata cache is stale (age: {} days), fetching fresh data",
                        age.num_days()
                    );
                }
            }
            Err(e) => {
                log::warn!("Failed to load cache from {:?}: {}", cache_path, e);
            }
        }
    }

    // Cache missing or stale, fetch from API
    if let Some(api) = api_context {
        let cache = fetch_from_api(api).await?;
        cache.save_to_disk(cache_path).await?;
        Ok(cache)
    } else {
        Err(anyhow!(
            "No cached field metadata found and no API context provided"
        ))
    }
}

/// Fetch field metadata from Google Ads Fields Service API
pub async fn fetch_from_api(api_context: &GoogleAdsAPIAccess) -> Result<FieldMetadataCache> {
    log::info!("Fetching field metadata from Google Ads Fields Service API");

    let mut client = GoogleAdsFieldServiceClient::with_interceptor(
        api_context.channel.clone(),
        api_context.clone(),
    );

    // Query all fields including extended metadata
    let query = "select name, category, data_type, selectable, filterable, sortable, \
                 selectable_with, enum_values, attribute_resources order by name";
    let response = client
        .search_google_ads_fields(SearchGoogleAdsFieldsRequest {
            query: query.to_owned(),
            page_token: String::new(),
            page_size: 10000,
        })
        .await
        .context("Failed to query Fields Service API")?
        .into_inner();

    let mut fields = HashMap::new();
    let mut resources: HashMap<String, Vec<String>> = HashMap::new();

    for row in response.results {
        // Convert category enum to string representation
        // Google Ads API enum values:
        // 0 = UNSPECIFIED, 1 = UNKNOWN, 2 = RESOURCE, 3 = ATTRIBUTE,
        // 5 = SEGMENT, 6 = METRIC
        let category = match row.category {
            2 => "RESOURCE",
            3 => "ATTRIBUTE",
            5 => "SEGMENT",
            6 => "METRIC",
            _ => {
                let name = row.name.as_ref().expect("GoogleAdsField must have a name");
                if name.starts_with("metrics.") {
                    "METRIC"
                } else if name.starts_with("segments.") {
                    "SEGMENT"
                } else {
                    "UNKNOWN"
                }
            }
        }
        .to_string();

        // Convert data_type enum to string representation
        // Google Ads API v23 GoogleAdsFieldDataType enum:
        // 0 = UNSPECIFIED, 1 = UNKNOWN, 2 = BOOLEAN, 3 = DATE,
        // 4 = DOUBLE, 5 = ENUM, 6 = FLOAT, 7 = INT32, 8 = INT64,
        // 9 = MESSAGE, 10 = RESOURCE_NAME, 11 = STRING, 12 = UINT64
        let data_type = match row.data_type {
            2 => "BOOLEAN",
            3 => "DATE",
            4 => "DOUBLE",
            5 => "ENUM",
            6 => "FLOAT",
            7 => "INT32",
            8 => "INT64",
            9 => "MESSAGE",
            10 => "RESOURCE_NAME",
            11 => "STRING",
            12 => "UINT64",
            _ => "UNKNOWN",
        }
        .to_string();

        let metrics_compatible = category == "ATTRIBUTE" || category == "SEGMENT";

        let field_name = row.name.clone().expect("GoogleAdsField must have a name");

        let field_meta = FieldMetadata {
            name: field_name.clone(),
            category,
            data_type,
            selectable: row.selectable.unwrap_or(false),
            filterable: row.filterable.unwrap_or(false),
            sortable: row.sortable.unwrap_or(false),
            metrics_compatible,
            resource_name: if row.resource_name.is_empty() {
                None
            } else {
                Some(row.resource_name.clone())
            },
            selectable_with: row.selectable_with.clone(),
            enum_values: row.enum_values.clone(),
            attribute_resources: row.attribute_resources.clone(),
            description: None,
            usage_notes: None,
        };

        // Organize by resource
        if let Some(resource) = field_meta.get_resource() {
            resources
                .entry(resource)
                .or_default()
                .push(field_name.clone());
        }

        fields.insert(field_name, field_meta);
    }

    log::info!(
        "Fetched {} fields from {} resources",
        fields.len(),
        resources.keys().len()
    );

    // Build resource metadata from fetched fields
    let resource_metadata = build_resource_metadata_from_fields(&fields, &resources);

    let cache = FieldMetadataCache {
        last_updated: Utc::now(),
        api_version: "v23".to_string(),
        fields,
        resources: Some(resources),
        resource_metadata: Some(resource_metadata),
    };

    // Validate that selectable_with is populated for all resources
    if let Err(empty_resources) = cache.validate_selectable_with() {
        log::error!(
            "CRITICAL: {} resources have empty selectable_with: {:?}",
            empty_resources.len(),
            empty_resources
        );
        return Err(anyhow::anyhow!(
            "Field metadata cache has {} resources with empty selectable_with. \
             This will break field compatibility validation. \
             Resources affected: {:?}",
            empty_resources.len(),
            empty_resources
        ));
    }

    Ok(cache)
}

/// Build ResourceMetadata entries from the fetched fields
fn build_resource_metadata_from_fields(
    fields: &HashMap<String, FieldMetadata>,
    resources: &HashMap<String, Vec<String>>,
) -> HashMap<String, ResourceMetadata> {
    let mut resource_metadata = HashMap::new();

    for (resource_name, field_names) in resources {
        let resource_fields: Vec<&FieldMetadata> =
            field_names.iter().filter_map(|n| fields.get(n)).collect();

        // Collect key attributes (selectable + filterable)
        let mut key_attributes: Vec<String> = resource_fields
            .iter()
            .filter(|f| f.is_attribute() && f.selectable && f.filterable)
            .take(10)
            .map(|f| f.name.clone())
            .collect();
        key_attributes.sort();

        // Get selectable_with from the RESOURCE-category field if present
        let selectable_with = fields
            .get(resource_name.as_str())
            .map(|f| f.selectable_with.clone())
            .unwrap_or_default();

        // Collect key metrics (selectable)
        // For views and other resources without own metrics, use metrics from selectable_with
        let own_metrics: Vec<String> = resource_fields
            .iter()
            .filter(|f| f.is_metric() && f.selectable)
            .map(|f| f.name.clone())
            .collect();

        let mut key_metrics = if own_metrics.is_empty() && !selectable_with.is_empty() {
            // For views with no own metrics, use metrics from selectable_with
            // Prioritize common metrics
            let priority_metrics = [
                "metrics.clicks",
                "metrics.impressions",
                "metrics.cost_micros",
                "metrics.conversions",
                "metrics.conversion_value",
                "metrics.all_conversions",
                "metrics.average_cpc",
                "metrics.ctr",
                "metrics.roas",
                "metrics.cost_per_conversion",
            ];

            let mut prioritized: Vec<String> = priority_metrics
                .iter()
                .filter(|m| selectable_with.contains(&m.to_string()))
                .map(|s| s.to_string())
                .collect();

            // Add any remaining selectable metrics alphabetically
            let mut remaining: Vec<String> = selectable_with
                .iter()
                .filter(|f| f.starts_with("metrics.") && !prioritized.contains(f))
                .cloned()
                .collect();
            remaining.sort();

            // Combine and limit
            prioritized.extend(remaining);
            prioritized.into_iter().take(10).collect()
        } else {
            own_metrics
        };

        key_metrics.sort();

        let identity_fields =
            mcc_gaql_common::field_metadata::compute_identity_fields(resource_name, fields, &selectable_with);

        resource_metadata.insert(
            resource_name.clone(),
            ResourceMetadata {
                name: resource_name.clone(),
                selectable_with,
                key_attributes,
                key_metrics,
                field_count: resource_fields.len(),
                description: None,
                uses_fallback: false,
                identity_fields,
            },
        );
    }

    resource_metadata
}
