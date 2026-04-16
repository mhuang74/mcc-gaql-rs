# Implementation Plan: Add `--generate-prompt-only` and `--resource` Flags

## Overview
This plan implements the ability to inspect LLM prompts without invoking the LLM, and to override resource selection. Full specification in `specs/inspect_prompts_used_for_gaql_generation.md`.

## Design Summary

### New CLI Flags
1. **`--resource <name>`** - Override Phase 1 resource selection
   - Validates resource exists in `field_cache`
   - Skips Phase 1 RAG steps when specified
   - Used alone: Force resource for normal generation
   - With `--generate-prompt-only`: Inspect Phase 3 prompt

2. **`--generate-prompt-only`** - Generate and print prompts without calling LLM
   - Without `--resource`: Runs Phase 1 RAG, prints Phase 1 prompt, stops
   - With `--resource <name>`: Runs Phase 2+2.5, prints Phase 3 prompt, stops

### New Enum: GenerateResult
```rust
pub enum GenerateResult {
    /// Full GAQL generation result
    Query(GAQLResult),
    /// Prompt-only output (system_prompt, user_prompt, phase_number)
    PromptOnly {
        system_prompt: String,
        user_prompt: String,
        phase: u8,  // 1 or 3
    },
}
```

### Behavior Matrix
| Flags | Behavior |
|-------|----------|
| `generate "prompt"` | Normal: Phase 1 → 5, full GAQL generation |
| `generate "prompt" --resource campaign` | Skip Phase 1, use "campaign", run Phases 2-5 |
| `generate "prompt" --generate-prompt-only` | Run Phase 1 RAG, print Phase 1 prompt, stop |
| `generate "prompt" --generate-prompt-only --resource campaign` | Skip Phase 1, run Phase 2+2.5, print Phase 3 prompt, stop |

## Implementation Steps

### Step 1: Modify `crates/mcc-gaql-gen/src/main.rs`

#### 1.1 Update `GenerateParams` struct (around line 51-61)
```rust
struct GenerateParams {
    prompt: String,
    queries: Option<String>,
    metadata: Option<PathBuf>,
    no_defaults: bool,
    use_query_cookbook: bool,
    explain: bool,
    verbose: bool,
    validate: bool,
    profile: Option<String>,
    resource: Option<String>,          // NEW
    generate_prompt_only: bool,        // NEW
}
```

#### 1.2 Add CLI flags to `Generate` command (around line 156-187)
```rust
Generate {
    /// Natural language query prompt
    prompt: String,

    /// Path to query cookbook TOML file
    #[arg(long)]
    queries: Option<String>,

    /// Path to enriched field metadata JSON (defaults to standard enriched cache path)
    #[arg(long)]
    metadata: Option<PathBuf>,

    /// Skip implicit default filters (e.g., status = ENABLED)
    #[arg(long)]
    no_defaults: bool,

    /// Enable RAG search for query cookbook examples in LLM prompts
    #[arg(long)]
    use_query_cookbook: bool,

    /// Print explanation of the LLM selection process to stdout
    #[arg(long)]
    explain: bool,

    /// Validate the generated query against Google Ads API (requires credentials)
    #[arg(long)]
    validate: bool,

    /// Profile to use for validation credentials (auto-detected if only one profile exists)
    #[arg(long)]
    profile: Option<String>,

    /// Override resource selection (skip Phase 1)  [NEW]
    #[arg(long)]
    resource: Option<String>,

    /// Stop after generating LLM prompt and print it (don't call LLM)  [NEW]
    #[arg(long)]
    generate_prompt_only: bool,
},
```

#### 1.3 Update command dispatch (around line 346-368)
Pass new flags to `GenerateParams` and `PipelineConfig`:
```rust
Commands::Generate {
    prompt,
    queries,
    metadata,
    no_defaults,
    use_query_cookbook,
    explain,
    validate,
    profile,
    resource,              // NEW
    generate_prompt_only,  // NEW
} => {
    cmd_generate(GenerateParams {
        prompt,
        queries,
        metadata,
        no_defaults,
        use_query_cookbook,
        explain,
        verbose: cli.verbose,
        validate,
        profile,
        resource,              // NEW
        generate_prompt_only,  // NEW
    })
    .await?;
}
```

#### 1.4 Update `cmd_generate` (around line 894-1081)
Pass new flags to `PipelineConfig` and handle `PromptOnly` result:

```rust
async fn cmd_generate(params: GenerateParams) -> Result<()> {
    // LLM validation - skip if generate_prompt_only
    if !params.generate_prompt_only {
        validate_llm_env()?;
    }

    let llm_config = rag::LlmConfig::from_env();

    // Load query cookbook
    // ... (existing code)

    // Load field metadata
    // ... (existing code)

    // Build pipeline config with new flags
    let pipeline_config = rag::PipelineConfig {
        add_defaults: !params.no_defaults,
        use_query_cookbook: params.use_query_cookbook,
        explain: params.explain,
        resource_override: params.resource.clone(),      // NEW
        generate_prompt_only: params.generate_prompt_only, // NEW
    };

    // Generate using MultiStepRAGAgent
    let result = rag::convert_to_gaql(
        example_queries,
        field_cache,
        &params.prompt,
        &llm_config,
        pipeline_config,
    )
    .await?;

    // Handle PromptOnly result
    match result {
        rag::GenerateResult::PromptOnly { system_prompt, user_prompt, phase } => {
            println!("═══════════════════════════════════════════════════════════════");
            println!("               PHASE {} LLM PROMPT", phase);
            println!("═══════════════════════════════════════════════════════════════\n");
            println!("=== SYSTEM PROMPT ===\n{}\n", system_prompt);
            println!("=== USER PROMPT ===\n{}\n", user_prompt);
            return Ok(());
        }
        rag::GenerateResult::Query(gaql_result) => {
            // existing query output and validation logic
            println!("{}", gaql_result.query);

            // Validate if requested
            if params.validate {
                // ... existing validation code
            }

            // Print explanation if flag is set
            if params.explain {
                rag::print_selection_explanation(&gaql_result.pipeline_trace, &params.prompt);
            }

            // Log validation errors/warnings
            // ... existing logging code

            Ok(())
        }
    }
}
```

### Step 2: Modify `crates/mcc-gaql-gen/src/rag.rs`

#### 2.1 Add `GenerateResult` enum (add before `PipelineConfig`, around line 1544)
```rust
/// Result of GAQL generation
pub enum GenerateResult {
    /// Full GAQL generation result
    Query(GAQLResult),
    /// Prompt-only output (system_prompt, user_prompt, phase_number)
    PromptOnly {
        system_prompt: String,
        user_prompt: String,
        phase: u8,  // 1 or 3
    },
}
```

#### 2.2 Update `PipelineConfig` (around line 1544-1563)
```rust
/// Configuration for the multi-step RAG pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Whether to add implicit default filters (e.g., status = ENABLED)
    pub add_defaults: bool,
    /// Whether to use RAG search for query cookbook examples in LLM prompts
    pub use_query_cookbook: bool,
    /// Whether to print explanation of the LLM selection process
    pub explain: bool,
    /// Override resource selection (skip Phase 1)  [NEW]
    pub resource_override: Option<String>,
    /// Stop after generating LLM prompt and print it  [NEW]
    pub generate_prompt_only: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            add_defaults: true,
            use_query_cookbook: false,
            explain: false,
            resource_override: None,          // NEW
            generate_prompt_only: false,       // NEW
        }
    }
}
```

#### 2.3 Extract `build_phase1_prompt()` from `select_resource()`
Extract the prompt-building logic from `select_resource()` (lines 2154-2504) into a new function.

**Add before `select_resource()` function:**
```rust
/// Build the Phase 1 prompt without calling LLM
async fn build_phase1_prompt(&self, user_query: &str) -> Result<(String, String), anyhow::Error> {
    // --- RAG pre-filter: Parallel search for resources AND fields ---
    let (resource_result, field_result) = tokio::join!(
        self.retrieve_relevant_resources(user_query, 20),
        self.retrieve_relevant_fields(user_query, 100)
    );

    let (resources, used_rag) = match resource_result {
        Ok(candidates) if !candidates.is_empty() => {
            let top_similarity = candidates[0].score;
            if top_similarity >= SIMILARITY_THRESHOLD {
                log::info!(
                    "Phase 1: RAG pre-filter selected {} resources (top similarity={:.3})",
                    candidates.len(),
                    top_similarity
                );
                let names: Vec<String> =
                    candidates.into_iter().map(|c| c.resource_name).collect();
                (names, true)
            } else {
                log::warn!(
                    "Phase 1: Low RAG confidence (similarity={:.3}), falling back to full resource list",
                    top_similarity
                );
                (self.field_cache.get_resources(), false)
            }
        }
        Ok(_) | Err(_) => {
            log::warn!("Phase 1: RAG resource search unavailable, using full resource list");
            (self.field_cache.get_resources(), false)
        }
    };

    let field_matches = field_result.unwrap_or_else(|e| {
        log::warn!("Phase 1: Field search failed: {}", e);
        vec![]
    });

    log::debug!(
        "Phase 1: Field search found {} matches above threshold",
        field_matches.len()
    );

    // Build resource information for sampling (with segment summaries)
    let resource_info: Vec<(String, String)> = resources
        .iter()
        .map(|r| {
            let rm = self
                .field_cache
                .resource_metadata
                .as_ref()
                .and_then(|m| m.get(r));
            let desc = rm.and_then(|m| m.description.as_deref()).unwrap_or("");

            // Add segment category summary
            let segment_summary = self.summarize_resource_segments(r);
            let full_desc = if segment_summary.is_empty() {
                desc.to_string()
            } else {
                format!("{} [Segments: {}]", desc, segment_summary)
            };

            (r.clone(), full_desc)
        })
        .collect();

    // Generate sample of 5 resources (prioritizing keyword matches)
    let resource_sample = create_resource_sample(user_query, &resource_info);

    // Retrieve cookbook examples for resource selection (if enabled)
    let cookbook_examples = if self.pipeline_config.use_query_cookbook {
        log::debug!("Phase 1: Retrieving cookbook examples for resource selection...");
        match self.retrieve_cookbook_examples(user_query, 2).await {
            Ok(examples) => {
                if !examples.is_empty() {
                    log::debug!("Phase 1: Retrieved cookbook examples for resource selection");
                    format!("\n\nSimilar Query Examples from Cookbook:\n{}", examples)
                } else {
                    String::new()
                }
            }
            Err(e) => {
                log::warn!("Phase 1: Failed to retrieve cookbook examples: {}", e);
                String::new()
            }
        }
    } else {
        String::new()
    };

    // Build categorized resource list for LLM
    let categorized_resources = self.build_categorized_resource_list(&resources);

    let resource_list_header = if used_rag {
        "IMPORTANT: You MUST select resources ONLY from the list below. \
         Do NOT invent or hallucinate resource names.\
         If no resource matches perfectly, choose the closest available option \
         and explain in reasoning.\
         Resources (selected by semantic similarity to your query):\n"
    } else {
        "IMPORTANT: You MUST select resources ONLY from the list below. \
         Do NOT invent or hallucinate resource names.\
         Resources (organized by category):\n"
    };

    let resource_guidance = self.domain_knowledge.section("Resource Selection Guidance");

    // Add field section to provide "bottom-up" signals for resource selection
    let field_section = if !field_matches.is_empty() {
        format!("\n\n{}", self.format_field_results_for_phase1(&field_matches))
    } else {
        String::new()
    };

    let combined_resources = format!("{}{}{}", categorized_resources, field_section, cookbook_examples);
    let system_prompt = format!(
        r#"You are a Google Ads Query Language (GAQL) expert. Given a user query, determine:
 1. The primary resource to query FROM (e.g., campaign, ad_group, keyword_view)
 2. Any related resources that might be needed (for JOINs or attributes)

Respond ONLY with valid JSON:
{{
  "primary_resource": "resource_name",
  "related_resources": ["related_resource1", "related_resource2"],
  "confidence": 0.95,
  "reasoning": "brief explanation"
}}

Resource selection guidance:
{resource_guidance}

RESOURCES section shows available resources organized by category.
FIELDS section shows individual fields that semantically match the query -
use these as hints for resource selection (e.g., if metrics.cost_micros
matches highly, campaign/ad_group resources are likely relevant).

{}
{}"#,
        resource_list_header, combined_resources
    );

    let user_prompt = format!("User query: {}", user_query);

    Ok((system_prompt, user_prompt))
}
```

#### 2.4 Extract `build_phase3_prompt()` from `select_fields()`
Extract the prompt-building logic from `select_fields()` (lines 2963-3385) into a new function.

**Add before `select_fields()` function:**
```rust
/// Build the Phase 3 prompt without calling LLM
fn build_phase3_prompt(
    &self,
    user_query: &str,
    primary: &str,
    candidates: &[FieldMetadata],
    filter_enums: &[(String, Vec<String>)],
) -> Result<(String, String), anyhow::Error> {
    // Retrieve top cookbook examples only if enabled
    let examples = if self.pipeline_config.use_query_cookbook {
        log::debug!("Phase 3: Retrieving cookbook examples...");
        let cookbook_start = std::time::Instant::now();

        // Fetch examples from query index (.CopyTo clipboard to match the call)
        let ex = self.retrieve_cookbook_examples(user_query, 3).await?;

        log::debug!(
            "Phase 3: Cookbook examples retrieved in {}ms",
            cookbook_start.elapsed().as_millis()
        );
        ex
    } else {
        log::debug!("Phase 3: Skipping cookbook examples (use_query_cookbook=false)");
        String::new()
    };

    // Build candidate name set for validation (LLM may hallucinate fields not in candidates)
    let candidate_names: HashSet<String> = candidates.iter().map(|f| f.name.clone()).collect();

    // Build set of all valid fields for this resource (selectable_with)
    let valid_fields: HashSet<String> = self
        .field_cache
        .get_resource_selectable_with(primary)
        .into_iter()
        .collect();

    // Build candidate list for LLM
    let mut candidate_text = String::new();
    let mut categories = std::collections::HashMap::new();

    for field in candidates {
        let category = categories
            .entry(field.category.clone())
            .or_insert_with(Vec::new);
        category.push(field);
    }

    for (cat, fields) in categories {
        candidate_text.push_str(&format!("\n### {} ({})\n", cat, fields.len()));
        for f in &fields {
            let filterable_tag = if f.filterable { " [filterable]" } else { "" };
            let sortable_tag = if f.sortable { " [sortable]" } else { "" };

            // Check for pre-scanned enum values
            let enum_note = filter_enums
                .iter()
                .find(|(name, _)| name == &f.name)
                .map(|(_, enums)| format!(" (valid: {})", enums.join(", ")))
                .unwrap_or_default();

            candidate_text.push_str(&format!(
                "- {}{}{}: {}{}\n",
                f.name,
                filterable_tag,
                sortable_tag,
                f.description.as_deref().unwrap_or(""),
                enum_note
            ));
        }
    }

    // Pre-computed date ranges for prompt interpolation
    let dates = DateContext::new();
    let today = dates.today;
    let this_year_start = dates.this_year_start;
    let prev_year_start = dates.prev_year_start;
    let prev_year_end = dates.prev_year_end;
    let this_quarter_start = dates.this_quarter_start;
    let prev_quarter_start = dates.prev_quarter_start;
    let prev_quarter_end = dates.prev_quarter_end;
    let last_60_start = dates.last_60_start;
    let last_90_start = dates.last_90_start;
    let (this_summer_start, this_summer_end) = dates.this_summer;
    let (last_summer_start, last_summer_end) = dates.last_summer;
    let (this_winter_start, this_winter end) = dates.this_winter;
    let (last_winter_start, last_winter_end) = dates.last_winter;
    let (this_spring_start, this_spring_end) = dates.this_spring;
    let (last_spring_start, last_spring_end) = dates.last_spring;
    let (this_fall_start, this_fall_end) = dates.this_fall;
    let (last_fall_start, last_fall_end) = dates.last_fall;
    let (this_christmas_start, this_christmas_end) = dates.this_christmas;

    let metric_terminology = self.domain_knowledge.section("Metric Terminology");
    let numeric_monetary = self
        .domain_knowledge
        .section("Numeric and Monetary Conversion");
    let monetary_extraction = self.domain_knowledge.section("Monetary Value Extraction");
    let date_range_handling = self.domain_knowledge.section("Date Range Handling");
    let query_best_practices = self.domain_knowledge.section("Query Best Practices");

    // Build prompt conditionally based on whether cookbook is enabled
    let (system_prompt, user_prompt) = if self.pipeline_config.use_query_cookbook {
        let sys = format!(
            r#"You are a Google Ads Query Language (GAQL) expert. Given:
 1. A user query
 2. Cookbook examples
 3. Available fields categorized by type

Today's date: {today}

Select the appropriate fields and build WHERE filters.

Respond ONLY with valid JSON:
{{
  "select_fields": ["field1", "field2", ...],
  "filter_fields": [{{"field": "field_name", "operator": "=", "value": "value"}}],
  "order_by_fields": [{{"field": "field_name", "direction": "DESC"}}],
  "limit": null,
  "reasoning": "brief explanation"
}}

- Use ONLY fields from the provided list
- Add filter_fields for any WHERE clauses
- **IMPORTANT: For IN and NOT IN operators, wrap values in parentheses: IN ('VALUE1', 'VALUE2') not IN 'VALUE'**
- Example: {{"field": "campaign.status", "operator": "IN", "value": "('ENABLED', 'PAUSED')"}}
- Add order_by_fields for sorting (use DESC for "top", "best", "worst"; ASC for "first" if ascending)
- If the query asks for "top N", "first N", "best N", or "worst N" results, set "limit" to that number N. If "top" or "best" is used without a specific number, default "limit" to 10. Otherwise set "limit" to null.
- **MANDATORY: If the user query mentions ANY time period (last week, last 7 days, yesterday, this month, year to date, etc.), you MUST add a segments.date filter_field. Do NOT mention date ranges only in reasoning -- they MUST appear in filter_fields. A query missing a date filter when the user specified a time period is INCORRECT.**
- In an MCC (multi-client) environment, always include customer.id and customer.descriptive_name in select_fields when they are available, so results can be identified by account.
- **IMPORTANT: Always include identity fields** for the primary resource in select_fields. Identity fields are the ones that identify each row — such as the resource's ID, name, and parent resource identifiers. Include them even if the user didn't explicitly ask for them. Examples: a campaign query should include campaign.id and campaign.name; an ad_group query should include campaign.id, campaign.name, ad_group.id, and ad_group.name; a keyword_view query should include ad_group_criterion.criterion_id and ad_group_criterion.keyword.text.
{metric_terminology}

{numeric_monetary}

{date_range_handling}

  Example computed date ranges (today: {today}):
  - "this year" → operator: "BETWEEN", value: '{this_year_start} AND {today}'
  - "last year" → operator: "BETWEEN", value: '{prev_year_start} AND {prev_year_end}'
  - "this quarter" → operator: "BETWEEN", value: '{this_quarter_start} AND {today}'
  - "last quarter" → operator: "BETWEEN", value: '{prev_quarter_start} AND {prev_quarter_end}'
  - "this summer" → operator: "BETWEEN", value: '{this_summer_start} AND {this_summer_end}'
  - "last summer" → operator: "BETWEEN", value: '{last_summer_start} AND {last_summer_end}'
  - "this winter" → operator: "BETWEEN", value: '{this_winter_start} AND {this_winter_end}'
  - "last winter" → operator: "BETWEEN", value: '{last_winter_start} AND {last_winter_end}'
  - "this spring" → operator: "BETWEEN", value: '{this_spring_start} AND {this_spring_end}'
  - "last spring" → operator: "BETWEEN", value: '{last_spring_start} AND {last_spring_end}'
  - "this fall" / "this autumn" → operator: "BETWEEN", value: '{this_fall_start} AND {this_fall_end}'
  - "last fall" → operator: "BETWEEN", value: '{last_fall_start} AND {last_fall_end}'
  - "christmas holiday" → operator: "BETWEEN", value: '{this_christmas_start} AND {this_christmas_end}'
  - "last 60 days" → operator: "BETWEEN", value: '{last_60_start} AND {today}'
  - "last 90 days" → operator: "BETWEEN", value: '{last_90_start} AND {today}'

{monetary_extraction}

{query_best_practices}
"#
        );
        let user = format!(
            "User query: {}\n\nCookbook examples:\n{}\n\nAvailable fields:{}",
            user_query, examples, candidate_text
        );
        (sys, user)
    } else {
        let sys = format!(
            r#"You are a Google Ads Query Language (GAQL) expert. Given:
 1. A user query
 2. Available fields categorized by type

Today's date: {today}

Select the appropriate fields and build WHERE filters.

Respond ONLY with valid JSON:
{{
  "select_fields": ["field1", "field2", ...],
  "filter_fields": [{{"field": "field_name", "operator": "=", "value": "value"}}],
  "order_by_fields": [{{"field": "field_name", "direction": "DESC"}}],
  "limit": null,
  "reasoning": "brief explanation"
}}

- Use ONLY fields from the provided list
- Add filter_fields for any WHERE clauses
- **IMPORTANT: For IN and NOT IN operators, wrap values in parentheses: IN ('VALUE1', 'VALUE2') not IN 'VALUE'**
- Example: {{"field": "campaign.status", "operator": "IN", "value": "('ENABLED', 'PAUSED')"}}
- Add order_by_fields for sorting (use DESC for "top", "best", "worst"; ASC for "first" if ascending)
- If the query asks for "top N", "first N", "best N", or "worst N" results, set "limit" to that number N. If "top" or "best" is used without a specific number, default "limit" to 10. Otherwise set "limit" to null.
- **MANDATORY: If the user query mentions ANY time period (last week, last 7 days, yesterday, this month, year to date, etc.), you MUST add a segments.date filter_field. Do NOT mention date ranges only in reasoning -- they MUST appear in filter_fields. A query missing a date filter when the user specified a time period is INCORRECT.**
- When querying account-level data (FROM customer) or when the user asks about accounts, always include customer.id and customer.descriptive_name in select_fields if available in the field list.
- Always include identity fields for the primary resource in select_fields — fields that identify each row, such as {{resource}}.id, {{resource}}.name, and parent resource identifiers (e.g., campaign.id, campaign.name for ad_group queries). Include them even if not explicitly requested.
{metric_terminology}

{numeric_monetary}

{date_range_handling}

  Example computed date ranges (today: {today}):
  - "this year" → operator: "BETWEEN", value: '{this_year_start} AND {today}'
  - "last year" → operator: "BETWEEN", value: '{prev_year_start} AND {prev_year_end}'
  - "this quarter" → operator: "BETWEEN", value: '{this_quarter_start} AND {today}'
  - "last quarter" → operator: "BETWEEN", value: '{prev_quarter_start} AND {prev_quarter_end}'
  - "this summer" → operator: "BETWEEN", value: '{this_summer_start} AND {this_summer_end}'
  - "last summer" → operator: "BETWEEN", value: '{last_summer_start} AND {last_summer_end}'
  - "this winter" → operator: "BETWEEN", value: '{this_winter_start} AND {this_winter_end}'
  - "last winter" → operator: "BETWEEN", value: '{last_winter_start} AND {last_winter_end}'
  - "this spring" → operator: "BETWEEN", value: '{this_spring_start} AND {this_spring_end}'
  - "last spring" → operator: "BETWEEN", value: '{last_spring_start} AND {last_spring_end}'
  - "this fall" / "this autumn" → operator: "BETWEEN", value: '{this_fall_start} AND {this_fall_end}'
  - "last fall" → operator: "BETWEEN", value: '{last_fall_start} AND {last_fall_end}'
  - "christmas holiday" → operator: "BETWEEN", value: '{this_christmas_start} AND {this_christmas_end}'
  - "last 60 days" → operator: "BETWEEN", value: '{last_60_start} AND {today}'
  - "last 90 days" → operator: "BETWEEN", value: '{last_90_start} AND {today}'

{monetary_extraction}

{query_best_practices}
"#
        );
        let user = format!(
            "User query: {}\n\nAvailable fields:{}",
            user_query, candidate_text
        );
        (sys, user)
    };

    Ok((system_prompt, user_prompt))
}
```

**Note**: The function above should be async for `retrieve_cookbook_examples` call. Let me fix that:

```rust
/// Build the Phase 3 prompt without calling LLM
async fn build_phase3_prompt(
    &self,
    user_query: &str,
    primary: &str,
    candidates: &[FieldMetadata],
    filter_enums: &[(String, Vec<String>)],
) -> Result<(String, String), anyhow::Error> {
    // ... implementation
}
```

#### 2.5 Update `generate()` method (lines 1826-1949)
Modify to handle the prompt-only modes:

```rust
/// Main entry point: generate GAQL query from user prompt
pub async fn generate(
    &self,
    user_query: &str,
) -> Result<GenerateResult, anyhow::Error> {
    // If generate_prompt_only WITHOUT resource_override: show Phase 1 prompt
    if self.pipeline_config.generate_prompt_only && self.pipeline_config.resource_override.is_none() {
        let (system_prompt, user_prompt) = self.build_phase1_prompt(user_query).await?;
        return Ok(GenerateResult::PromptOnly { system_prompt, user_prompt, phase: 1 });
    }

    // Phase 1: Resource selection (or use override)
    let primary_resource = if let Some(ref resource) = self.pipeline_config.resource_override {
        // Validate resource exists
        if !self.field_cache.get_resources().contains(resource) {
            return Err(anyhow::anyhow!("Unknown resource: '{}'", resource));
        }
        resource.clone()
    } else {
        let (primary, _, _, _, _) = self.select_resource(user_query).await?;
        primary
    };

    // Phase 2 + 2.5: Field candidate retrieval and pre-scan
    let (candidates, ..) = self.retrieve_field_candidates(user_query, &primary_resource, &[]).await?;
    let filter_enums = self.prescan_filters(user_query, &candidates);

    // If generate_prompt_only WITH resource_override: show Phase 3 prompt
    if self.pipeline_config.generate_prompt_only {
        let (system_prompt, user_prompt) = self.build_phase3_prompt(
            user_query, &primary_resource, &candidates, &filter_enums
        ).await?;
        return Ok(GenerateResult::PromptOnly { system_prompt, user_prompt, phase: 3 });
    }

    // Continue with normal pipeline...
    let start = std::time::Instant::now();

    // Phase 1: Resource selection (if not using override)
    let phase1_start = std::time::Instant::now();
    let (primary_resource, related_resources, dropped_resources, reasoning, resource_sample) =
        if self.pipeline_config.resource_override.is_some() {
            // Already set primary_resource above, use default values
            (primary_resource.clone(), vec![], vec![], String::new(), vec![])
        } else {
            self.select_resource(user_query).await?
        };
    let phase1_time_ms = phase1_start.elapsed().as_millis() as u64;
    log::info!(
        "Phase 1 complete: {} ({}ms)",
        primary_resource,
        phase1_time_ms
    );

    // ... rest of existing generate() logic
    // Phase 2: Field candidate retrieval
    let phase2_start = std::time::Instant::now();
    let (candidates, candidate_count, rejected_count) = self
        .retrieve_field_candidates(user_query, &primary_resource, &related_resources)
        .await?;
    let phase2_time_ms = phase2_start.elapsed().as_millis() as u64;

    // Phase 2.5: Pre-scan for filter keywords
    let filter_enums = self.prescan_filters(user_query, &candidates);

    // Phase 3: Field selection via LLM
    let phase3_start = std::time::Instant::now();
    let field_selection = self
        .select_fields(user_query, &primary_resource, &candidates, &filter_enums)
        .await?;
    let phase3_time_ms = phase3_start.elapsed().as_millis() as u64;

    // Phase 4: Assemble WHERE, ORDER BY, LIMIT
    let (where_clauses, limit, implicit_filters) =
        self.assemble_criteria(user_query, &field_selection, &primary_resource);

    // Phase 5: Generate final GAQL query
    let result = self
        .generate_gaql(&primary_resource, &field_selection, &where_clauses, limit)
        .await?;

    let generation_time_ms = start.elapsed().as_millis() as u64;

    // Build pipeline trace
    let phase1_model = self.llm_config.preferred_model().to_string();
    let phase3_model = self.llm_config.preferred_model().to_string();
    let pipeline_trace = mcc_gaql_common::field_metadata::PipelineTrace {
        phase1_primary_resource: primary_resource.clone(),
        phase1_related_resources: related_resources,
        phase1_dropped_resources: dropped_resources,
        phase1_reasoning: reasoning,
        phase1_model_used: phase1_model,
        phase1_timing_ms: phase1_time_ms,
        phase1_resource_sample: resource_sample,
        phase2_candidate_count: candidate_count,
        phase2_rejected_count: rejected_count,
        phase2_timing_ms: phase2_time_ms,
        phase25_pre_scan_filters: filter_enums.clone(),
        phase3_selected_fields: field_selection.select_fields.clone(),
        phase3_filter_fields: field_selection.filter_fields.clone(),
        phase3_order_by_fields: field_selection.order_by_fields.clone(),
        phase3_reasoning: field_selection.reasoning.clone(),
        phase3_model_used: phase3_model,
        phase3_timing_ms: phase3_time_ms,
        phase4_where_clauses: where_clauses,
        phase4_limit: limit,
        phase4_implicit_filters: implicit_filters,
        generation_time_ms,
    };

    // Validate the field selection against the primary resource
    let all_fields: Vec<String> = field_selection
        .select_fields
        .iter()
        .chain(field_selection.filter_fields.iter().map(|f| &f.field_name))
        .cloned()
        .collect();
    let validation = self
        .field_cache
        .validate_field_selection_for_resource(&all_fields, &primary_resource);

    Ok(GenerateResult::Query(mcc_gaql_common::field_metadata::GAQLResult {
        query: result,
        validation,
        pipeline_trace,
    }))
}
```

#### 2.6 Update `convert_to_gaql()` public entry point
Update return type from `Result<GAQLResult>` to `Result<GenerateResult>`:

```rust
/// Public entry point for GAQL generation
pub async fn convert_to_gaql(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
    pipeline_config: PipelineConfig,
) -> Result<GenerateResult, anyhow::Error> {
    let agent =
        MultiStepRAGAgent::init(example_queries, field_cache, config, pipeline_config).await?;
    agent.generate(prompt).await
}
```

## Verification

### Test Phase 1 prompt only:
```bash
cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only
```
Expected output:
- Prints Phase 1 system and user prompts
- Does NOT call LLM
- Does NOT generate GAQL query

### Test Phase 3 prompt only:
```bash
cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only --resource campaign
```
Expected output:
- Prints Phase 3 system and user prompts
- Does NOT call LLM
- Does NOT generate GAQL query

### Test resource override without prompt-only:
```bash
cargo run -p mcc-gaql-gen -- generate "show ad performance" --resource ad_group
```
Expected output:
- Skips Phase 1, uses "ad_group" as resource
- Completes full pipeline
- Generates final GAQL query

### Test invalid resource validation:
```bash
cargo run -p mcc-gaql-gen -- generate "test" --resource invalid_resource
```
Expected output:
- Errors with "Unknown resource: 'invalid_resource'"

### Run existing tests:
```bash
cargo test -p mcc-gaql-gen -- --test-threads=1
```

## Notes & Considerations

1. **Breaking Change**: The return type of `convert_to_gaql()` changes from `Result<GAQLResult>` to `Result<GenerateResult>`. This affects all callers.

2. **LLM Validation**: With `--generate-prompt-only`, we skip LLM environment validation via `validate_llm_env()`. This allows users to inspect prompts even when API credentials aren't configured.

3. **Cache Validation**: We still validate field_cache and embeddings even in prompt-only mode. This ensures users have up-to-date data before inspecting prompts.

4. **Error Handling**: Invalid resource names return a clear error: `"Unknown resource: 'invalid_resource'"`.

5. **Resource Sample**: When using `--resource`, the `resource_sample` in the pipeline trace will be empty (since we skip Phase 1). This is intentional for this mode.

6. **Date Context**: The `build_phase3_prompt()` function uses the same `DateContext` logic as `select_fields()`, ensuring consistent date information in prompts.

7. **Async Building**: `build_phase3_prompt()` is async because it needs to call `retrieve_cookbook_examples()`.

8. **Prompt-only mode**: When `generate_prompt_only` is true, we generate prompts but don't call the LLM, making it safe to use without API credentials.
