# Multi-Step RAG Pipeline for Enhanced GAQL Generation

**Version:** 1.0
**Date:** 2026-03-10
**Status:** DRAFT - For Review
**Related:** `gaql-metadata-for-llm-design.md`, `rag-quality-improvement-plan.md`

---

## Executive Summary

This design spec details a multi-phase RAG architecture for GAQL generation that addresses the core limitations of the current `EnhancedRAGAgent`. The proposed `MultiStepRAGAgent` implements a 5-phase pipeline:

1. **Resource Selection** - LLM ranks and selects primary/related resources
2. **Field Candidate Retrieval** - RAG retrieves fields filtered by resource compatibility
3. **Field Selection & Ranking** - LLM selects appropriate fields for user intent
4. **Criteria & Segmentation** - Determines WHERE/DURING clauses
5. **GAQL Generation** - Assembles final query with validation

**Key Improvements:**
- Uses `selectable_with` metadata for field/resource compatibility validation
- Structured multi-phase reasoning instead of single-shot prompting
- Mutual exclusivity handling to prevent invalid field combinations
- Graceful degradation with fallback to keyword-based selection

---

## Background

### Current System Limitations

The existing `EnhancedRAGAgent` (in `crates/mcc-gaql-gen/src/rag.rs:809-1148`):

1. **Keyword-based resource detection** (lines 1027-1056): Uses simple string matching (`query.contains("campaign")`) instead of semantic understanding
2. **Generic field retrieval**: Retrieves 10 fields via RAG without filtering by resource compatibility
3. **No field compatibility validation**: The `selectable_with` metadata is available but not used
4. **Single-shot prompting**: All context combined into one LLM call without structured reasoning
5. **No mutual exclusivity handling**: Some fields cannot be selected together

### Available Metadata

**FieldMetadata** (from `mcc-gaql-common/src/field_metadata.rs`):
```rust
pub struct FieldMetadata {
    pub name: String,
    pub category: String,        // "METRIC", "SEGMENT", "ATTRIBUTE", "RESOURCE"
    pub data_type: String,
    pub selectable: bool,
    pub filterable: bool,
    pub sortable: bool,
    pub metrics_compatible: bool,
    pub resource_name: Option<String>,
    pub selectable_with: Vec<String>,    // Fields/resources this can query with
    pub attribute_resources: Vec<String>, // Resources this attribute belongs to
    pub enum_values: Vec<String>,
    pub description: Option<String>,
    pub usage_notes: Option<String>,
}
```

**ResourceMetadata**:
```rust
pub struct ResourceMetadata {
    pub name: String,
    pub selectable_with: Vec<String>,   // Compatible resources
    pub key_attributes: Vec<String>,
    pub key_metrics: Vec<String>,
    pub field_count: usize,
    pub description: Option<String>,
}
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     MULTI-STEP RAG PIPELINE                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   User Query: "show me Performance Max campaigns with declining ROAS"   │
│                              │                                          │
│                              ▼                                          │
│   ┌─────────────────────────────────────────────────────────────┐      │
│   │  PHASE 1: Resource Selection (LLM)                          │      │
│   │  Input: User query + all resources (~100)                   │      │
│   │  Output: primary_resource="campaign", related=["customer"]  │      │
│   └─────────────────────────────────────────────────────────────┘      │
│                              │                                          │
│                              ▼                                          │
│   ┌─────────────────────────────────────────────────────────────┐      │
│   │  PHASE 2: Field Candidate Retrieval (RAG + Filter)          │      │
│   │  Input: User query + resource selection                     │      │
│   │  Process:                                                   │      │
│   │    1. RAG retrieves top-30 fields by semantic similarity    │      │
│   │    2. Filter by selectable_with compatibility               │      │
│   │    3. Add primary resource fields directly                  │      │
│   │  Output: FieldCandidates {attributes, metrics, segments}    │      │
│   └─────────────────────────────────────────────────────────────┘      │
│                              │                                          │
│                              ▼                                          │
│   ┌─────────────────────────────────────────────────────────────┐      │
│   │  PHASE 3: Field Selection & Ranking (LLM)                   │      │
│   │  Input: User query + candidates + cookbook examples         │      │
│   │  Process:                                                   │      │
│   │    1. LLM selects appropriate fields for intent             │      │
│   │    2. Handle mutual exclusivity using selectable_with       │      │
│   │  Output: select_fields, filter_fields, order_by_fields      │      │
│   └─────────────────────────────────────────────────────────────┘      │
│                              │                                          │
│                              ▼                                          │
│   ┌─────────────────────────────────────────────────────────────┐      │
│   │  PHASE 4: Criteria & Segmentation (Local + Pattern Match)   │      │
│   │  Input: User query + selected fields                        │      │
│   │  Process:                                                   │      │
│   │    1. Detect temporal patterns → DURING clause              │      │
│   │    2. Build WHERE clauses from filter_fields                │      │
│   │    3. Determine segments for grouping                       │      │
│   │  Output: where_clauses, during_clause, limit                │      │
│   └─────────────────────────────────────────────────────────────┘      │
│                              │                                          │
│                              ▼                                          │
│   ┌─────────────────────────────────────────────────────────────┐      │
│   │  PHASE 5: GAQL Generation (Local Assembly + Validation)     │      │
│   │  Input: All phase outputs                                   │      │
│   │  Process:                                                   │      │
│   │    1. Assemble SELECT, FROM, WHERE, ORDER BY, LIMIT         │      │
│   │    2. Validate using field_cache.validate_field_selection   │      │
│   │  Output: Final GAQL query + validation result               │      │
│   └─────────────────────────────────────────────────────────────┘      │
│                                                                         │
│   Output:                                                               │
│   SELECT campaign.name, campaign.advertising_channel_type,              │
│          metrics.conversions_value_per_cost, segments.date              │
│   FROM campaign                                                         │
│   WHERE campaign.advertising_channel_type IN ('PERFORMANCE_MAX')        │
│         AND segments.date DURING LAST_30_DAYS                           │
│   ORDER BY segments.date                                                │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Struct Definitions

### 1.1 Core Agent Structure

```rust
/// Multi-step RAG agent for high-accuracy GAQL generation
pub struct MultiStepRAGAgent {
    /// LLM configuration
    llm_config: LlmConfig,

    /// Vector stores for RAG retrieval
    query_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    field_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,

    /// Structured metadata for validation
    field_cache: FieldMetadataCache,

    /// Embedding model for queries
    embedding_model: rig_fastembed::EmbeddingModel,

    /// Keep embed client alive
    _embed_client: rig_fastembed::Client,
}
```

### 1.2 Phase 1 Output: Resource Selection

```rust
/// Result of Phase 1: Resource selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSelectionResult {
    /// Primary resource for FROM clause
    pub primary_resource: String,
    /// Related resources that can be joined (from selectable_with)
    pub related_resources: Vec<String>,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// LLM reasoning for selection
    pub reasoning: String,
}
```

### 1.3 Phase 2 Output: Field Candidates

```rust
/// Result of Phase 2: Field candidates grouped by category
#[derive(Debug, Clone)]
pub struct FieldCandidates {
    /// Attributes from primary and related resources
    pub attributes: Vec<FieldMetadata>,
    /// Metrics compatible with selected resources
    pub metrics: Vec<FieldMetadata>,
    /// Available segments
    pub segments: Vec<FieldMetadata>,
    /// Count of fields that passed compatibility filtering
    pub compatible_count: usize,
    /// Count of fields rejected due to incompatibility
    pub rejected_count: usize,
}
```

### 1.4 Phase 3 Output: Field Selection

```rust
/// Result of Phase 3: Selected fields with reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelectionResult {
    /// Fields to include in SELECT clause
    pub select_fields: Vec<String>,
    /// Fields identified as filter criteria
    pub filter_fields: Vec<FilterCriterion>,
    /// Fields for ORDER BY
    pub order_by_fields: Vec<OrderByField>,
    /// LLM reasoning
    pub reasoning: String,
}

/// Filter criterion with operator and value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCriterion {
    pub field_name: String,
    pub operator: String,       // =, !=, <, >, IN, LIKE, DURING, etc.
    pub value: String,          // Literal, enum value, or date range
    pub is_temporal: bool,      // True for date-based filters
}

/// Order by specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderByField {
    pub field_name: String,
    pub direction: String,      // ASC or DESC
}
```

### 1.5 Phase 4 Output: Criteria & Segmentation

```rust
/// Result of Phase 4: WHERE clause and segmentation decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriteriaResult {
    /// WHERE clause components (excluding DURING)
    pub where_clauses: Vec<String>,
    /// DURING clause if temporal (e.g., "LAST_30_DAYS")
    pub during_clause: Option<String>,
    /// Segments to include for proper grouping
    pub segments_for_grouping: Vec<String>,
    /// Whether date segment is needed for time-series
    pub needs_date_segment: bool,
    /// LIMIT value if specified
    pub limit: Option<u32>,
}
```

### 1.6 Phase 5 Output: Final GAQL

```rust
/// Final GAQL result with validation and trace
#[derive(Debug, Clone)]
pub struct GAQLResult {
    /// The generated GAQL query
    pub query: String,
    /// Validation result
    pub validation: GAQLValidationResult,
    /// Full pipeline trace for debugging
    pub pipeline_trace: PipelineTrace,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct GAQLValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Trace of all pipeline decisions for debugging
#[derive(Debug, Clone)]
pub struct PipelineTrace {
    pub resource_selection: ResourceSelectionResult,
    pub field_candidates_summary: FieldCandidateSummary,
    pub field_selection: FieldSelectionResult,
    pub criteria: CriteriaResult,
    pub generation_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct FieldCandidateSummary {
    pub attributes_count: usize,
    pub metrics_count: usize,
    pub segments_count: usize,
    pub compatible_count: usize,
    pub rejected_count: usize,
}
```

---

## 2. Phase-by-Phase Implementation

### Phase 1: Resource Selection

**Purpose**: Determine the primary resource (FROM clause) and compatible related resources.

**Why not use RAG**: The resource list (~100 items) is small enough for the LLM to process directly, and ranking all resources provides better global understanding than retrieving a subset.

**Implementation**:

```rust
impl MultiStepRAGAgent {
    async fn phase1_resource_selection(
        &self,
        user_query: &str,
    ) -> Result<ResourceSelectionResult> {
        // Get all resources with metadata
        let resources = self.field_cache.get_resources();
        let resource_summaries = self.build_resource_summaries(&resources);

        // Build prompt
        let prompt = self.build_resource_selection_prompt(user_query, &resource_summaries);

        // Call LLM
        let agent = self.llm_config.create_agent_for_model(
            self.llm_config.preferred_model(),
            RESOURCE_SELECTION_SYSTEM_PROMPT
        )?;
        let response = agent.prompt(&prompt).await?;

        // Parse JSON response
        let result: ResourceSelectionResult = serde_json::from_str(
            &strip_markdown_code_blocks(&response)
        )?;

        // Validate: ensure selected resources exist
        self.validate_resource_selection(&result)?;

        Ok(result)
    }

    fn build_resource_summaries(&self, resources: &[String]) -> Vec<ResourceSummary> {
        resources.iter().filter_map(|r| {
            self.field_cache.resource_metadata.as_ref()
                .and_then(|rm| rm.get(r))
                .map(|meta| ResourceSummary {
                    name: r.clone(),
                    description: meta.description.clone().unwrap_or_default(),
                    selectable_with: meta.selectable_with.clone(),
                    field_count: meta.field_count,
                    key_attributes: meta.key_attributes.clone(),
                    key_metrics: meta.key_metrics.clone(),
                })
        }).collect()
    }

    fn validate_resource_selection(&self, result: &ResourceSelectionResult) -> Result<()> {
        let all_resources = self.field_cache.get_resources();

        // Check primary resource exists
        if !all_resources.contains(&result.primary_resource) {
            return Err(anyhow::anyhow!(
                "Invalid primary resource: {}",
                result.primary_resource
            ));
        }

        // Check related resources exist and are compatible
        let primary_selectable = self.get_resource_selectable_with(&result.primary_resource);
        for related in &result.related_resources {
            if !all_resources.contains(related) {
                log::warn!("Unknown related resource: {}, ignoring", related);
                continue;
            }
            if !primary_selectable.contains(related) {
                log::warn!(
                    "Resource {} not in {}'s selectable_with, may cause query errors",
                    related, result.primary_resource
                );
            }
        }

        Ok(())
    }
}
```

### Phase 2: Field Candidate Retrieval

**Purpose**: Retrieve candidate fields using RAG, then filter by resource compatibility.

**Implementation**:

```rust
impl MultiStepRAGAgent {
    async fn phase2_field_candidate_retrieval(
        &self,
        user_query: &str,
        resource_selection: &ResourceSelectionResult,
    ) -> Result<FieldCandidates> {
        // Step 1: RAG retrieval with higher limit (we'll filter down)
        let rag_limit = 30;
        let rag_results = self.retrieve_fields_via_rag(user_query, rag_limit).await?;

        // Step 2: Get all fields for primary resource directly
        let primary_fields = self.field_cache.get_resource_fields(
            &resource_selection.primary_resource
        );

        // Step 3: Get fields from related resources
        let mut related_fields = Vec::new();
        for related in &resource_selection.related_resources {
            related_fields.extend(self.field_cache.get_resource_fields(related));
        }

        // Step 4: Filter RAG results by compatibility
        let primary_selectable = self.get_resource_selectable_with(
            &resource_selection.primary_resource
        );
        let (compatible, rejected) = self.filter_by_compatibility(
            &rag_results,
            &resource_selection.primary_resource,
            &primary_selectable,
        );

        // Step 5: Merge and deduplicate
        let mut all_candidates: HashMap<String, FieldMetadata> = HashMap::new();

        // Add RAG-retrieved compatible fields (highest priority)
        for field in compatible {
            all_candidates.insert(field.name.clone(), field);
        }

        // Add primary resource fields
        for field in primary_fields {
            all_candidates.entry(field.name.clone())
                .or_insert(field.clone());
        }

        // Add related resource fields
        for field in related_fields {
            all_candidates.entry(field.name.clone())
                .or_insert(field.clone());
        }

        // Add all metrics (metrics are generally compatible)
        for metric in self.field_cache.get_metrics(None) {
            all_candidates.entry(metric.name.clone())
                .or_insert(metric.clone());
        }

        // Add all segments
        for segment in self.field_cache.get_segments(None) {
            all_candidates.entry(segment.name.clone())
                .or_insert(segment.clone());
        }

        // Step 6: Categorize
        let attributes: Vec<_> = all_candidates.values()
            .filter(|f| f.is_attribute() && f.selectable)
            .cloned()
            .collect();
        let metrics: Vec<_> = all_candidates.values()
            .filter(|f| f.is_metric() && f.selectable)
            .cloned()
            .collect();
        let segments: Vec<_> = all_candidates.values()
            .filter(|f| f.is_segment() && f.selectable)
            .cloned()
            .collect();

        Ok(FieldCandidates {
            attributes,
            metrics,
            segments,
            compatible_count: all_candidates.len(),
            rejected_count: rejected.len(),
        })
    }

    fn filter_by_compatibility(
        &self,
        fields: &[FieldMetadata],
        primary_resource: &str,
        primary_selectable_with: &[String],
    ) -> (Vec<FieldMetadata>, Vec<FieldMetadata>) {
        let mut compatible = Vec::new();
        let mut rejected = Vec::new();

        for field in fields {
            if self.is_field_compatible(field, primary_resource, primary_selectable_with) {
                compatible.push(field.clone());
            } else {
                rejected.push(field.clone());
            }
        }

        (compatible, rejected)
    }

    fn is_field_compatible(
        &self,
        field: &FieldMetadata,
        primary_resource: &str,
        primary_selectable_with: &[String],
    ) -> bool {
        // Metrics and segments are generally compatible
        if field.is_metric() || field.is_segment() {
            return true;
        }

        // Check field's resource
        if let Some(field_resource) = field.get_resource() {
            // Direct match with primary
            if field_resource == primary_resource {
                return true;
            }

            // Field's resource is in primary's selectable_with
            if primary_selectable_with.contains(&field_resource) {
                return true;
            }

            // Check field's own selectable_with
            if field.selectable_with.contains(&primary_resource.to_string()) {
                return true;
            }
        }

        false
    }
}
```

### Phase 3: Field Selection & Ranking

**Purpose**: Use LLM to select appropriate fields from candidates.

**Implementation**:

```rust
impl MultiStepRAGAgent {
    async fn phase3_field_selection(
        &self,
        user_query: &str,
        resource_selection: &ResourceSelectionResult,
        candidates: &FieldCandidates,
    ) -> Result<FieldSelectionResult> {
        // Get similar cookbook queries for examples
        let cookbook_examples = self.retrieve_cookbook_examples(user_query, 3).await?;

        // Build prompt
        let prompt = self.build_field_selection_prompt(
            user_query,
            resource_selection,
            candidates,
            &cookbook_examples,
        );

        // Call LLM
        let agent = self.llm_config.create_agent_for_model(
            self.llm_config.preferred_model(),
            FIELD_SELECTION_SYSTEM_PROMPT
        )?;
        let response = agent.prompt(&prompt).await?;

        // Parse response
        let mut result: FieldSelectionResult = serde_json::from_str(
            &strip_markdown_code_blocks(&response)
        )?;

        // Validate selected fields exist and are selectable
        result.select_fields = self.validate_and_filter_fields(&result.select_fields);

        // Handle mutual exclusivity
        result = self.handle_mutual_exclusivity(result)?;

        Ok(result)
    }

    fn validate_and_filter_fields(&self, fields: &[String]) -> Vec<String> {
        fields.iter()
            .filter(|f| {
                if let Some(meta) = self.field_cache.get_field(f) {
                    if !meta.selectable {
                        log::warn!("Field {} is not selectable, removing", f);
                        return false;
                    }
                    true
                } else {
                    log::warn!("Unknown field {}, removing", f);
                    false
                }
            })
            .cloned()
            .collect()
    }

    fn handle_mutual_exclusivity(
        &self,
        mut result: FieldSelectionResult,
    ) -> Result<FieldSelectionResult> {
        let mut to_remove = Vec::new();

        // Check each pair of fields for compatibility
        for (i, field_name) in result.select_fields.iter().enumerate() {
            if let Some(field) = self.field_cache.get_field(field_name) {
                // Only check if field has limited selectable_with
                if !field.selectable_with.is_empty() {
                    for (j, other_name) in result.select_fields.iter().enumerate() {
                        if i < j {
                            if let Some(other_field) = self.field_cache.get_field(other_name) {
                                if !self.can_select_together(field, other_field) {
                                    log::warn!(
                                        "Mutual exclusivity: {} conflicts with {}, removing {}",
                                        field_name, other_name, other_name
                                    );
                                    to_remove.push(j);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove conflicting fields (in reverse order to maintain indices)
        to_remove.sort();
        to_remove.dedup();
        to_remove.reverse();
        for idx in to_remove {
            result.select_fields.remove(idx);
        }

        Ok(result)
    }

    fn can_select_together(&self, field1: &FieldMetadata, field2: &FieldMetadata) -> bool {
        // Metrics and segments are generally compatible
        if field1.is_metric() || field1.is_segment()
            || field2.is_metric() || field2.is_segment() {
            return true;
        }

        // Check selectable_with compatibility
        if !field1.selectable_with.is_empty() && !field2.selectable_with.is_empty() {
            if let Some(r2) = field2.get_resource() {
                if !field1.selectable_with.contains(&r2) {
                    return false;
                }
            }
            if let Some(r1) = field1.get_resource() {
                if !field2.selectable_with.contains(&r1) {
                    return false;
                }
            }
        }

        true
    }
}
```

### Phase 4: Criteria & Segmentation

**Purpose**: Determine WHERE clauses, DURING clause, and grouping segments.

**Implementation**:

```rust
impl MultiStepRAGAgent {
    fn phase4_criteria_segmentation(
        &self,
        user_query: &str,
        field_selection: &FieldSelectionResult,
    ) -> CriteriaResult {
        let query_lower = user_query.to_lowercase();

        // Detect temporal requirements
        let (needs_date, during_clause) = self.detect_temporal_requirements(&query_lower);

        // Build WHERE clauses from filter_fields
        let where_clauses = self.build_where_clauses(&field_selection.filter_fields);

        // Determine segments for proper metric grouping
        let segments_for_grouping = self.determine_segments_for_grouping(
            &query_lower,
            &field_selection.select_fields,
        );

        // Detect limit requirement
        let limit = self.detect_limit_requirement(&query_lower);

        CriteriaResult {
            where_clauses,
            during_clause,
            segments_for_grouping,
            needs_date_segment: needs_date,
            limit,
        }
    }

    fn detect_temporal_requirements(&self, query_lower: &str) -> (bool, Option<String>) {
        let temporal_patterns = [
            ("last 7 days", "LAST_7_DAYS"),
            ("last week", "LAST_7_DAYS"),
            ("past week", "LAST_7_DAYS"),
            ("last 14 days", "LAST_14_DAYS"),
            ("last two weeks", "LAST_14_DAYS"),
            ("last 30 days", "LAST_30_DAYS"),
            ("last month", "LAST_30_DAYS"),
            ("past month", "LAST_30_DAYS"),
            ("this month", "THIS_MONTH"),
            ("last quarter", "LAST_BUSINESS_QUARTER"),
            ("yesterday", "YESTERDAY"),
            ("today", "TODAY"),
            ("this year", "THIS_YEAR"),
            ("ytd", "THIS_YEAR"),
            ("last 90 days", "LAST_90_DAYS"),
        ];

        for (pattern, during_value) in temporal_patterns {
            if query_lower.contains(pattern) {
                return (true, Some(during_value.to_string()));
            }
        }

        // Check for trend/time-series indicators
        let needs_date = query_lower.contains("trend")
            || query_lower.contains("over time")
            || query_lower.contains("by date")
            || query_lower.contains("daily")
            || query_lower.contains("weekly")
            || query_lower.contains("monthly")
            || query_lower.contains("declining")
            || query_lower.contains("increasing")
            || query_lower.contains("performance");

        // Default to LAST_30_DAYS if temporal but no specific period
        if needs_date {
            return (true, Some("LAST_30_DAYS".to_string()));
        }

        (false, None)
    }

    fn build_where_clauses(&self, filter_fields: &[FilterCriterion]) -> Vec<String> {
        let mut clauses = Vec::new();

        for criterion in filter_fields {
            // Skip temporal criteria (handled via DURING)
            if criterion.is_temporal {
                continue;
            }

            if let Some(field) = self.field_cache.get_field(&criterion.field_name) {
                if !field.filterable {
                    log::warn!("Field {} is not filterable, skipping", criterion.field_name);
                    continue;
                }
            }

            clauses.push(format!(
                "{} {} {}",
                criterion.field_name,
                criterion.operator,
                criterion.value
            ));
        }

        clauses
    }

    fn determine_segments_for_grouping(
        &self,
        query_lower: &str,
        select_fields: &[String],
    ) -> Vec<String> {
        let mut segments = Vec::new();

        // Add device segment if device-related query
        if query_lower.contains("device") || query_lower.contains("mobile")
            || query_lower.contains("desktop") {
            segments.push("segments.device".to_string());
        }

        // Add ad_network_type if network-related
        if query_lower.contains("network") || query_lower.contains("search")
            || query_lower.contains("display") {
            segments.push("segments.ad_network_type".to_string());
        }

        // Check if we have metrics but no grouping
        let has_metrics = select_fields.iter().any(|f| f.starts_with("metrics."));
        let has_segments = select_fields.iter().any(|f| f.starts_with("segments."));
        let has_attributes = select_fields.iter().any(|f| {
            !f.starts_with("metrics.") && !f.starts_with("segments.")
        });

        // Metrics require grouping by attributes or segments
        if has_metrics && !has_segments && !has_attributes {
            // Default to date segment for grouping
            segments.push("segments.date".to_string());
        }

        segments
    }

    fn detect_limit_requirement(&self, query_lower: &str) -> Option<u32> {
        // Check for explicit "top N" or "first N" patterns
        let patterns = [
            (r"top (\d+)", true),
            (r"first (\d+)", true),
            (r"limit (\d+)", true),
            (r"(\d+) results", true),
        ];

        for (pattern, _) in patterns {
            if let Some(caps) = regex::Regex::new(pattern).ok()
                .and_then(|re| re.captures(query_lower))
            {
                if let Some(num) = caps.get(1).and_then(|m| m.as_str().parse().ok()) {
                    return Some(num);
                }
            }
        }

        // Default limit for certain query types
        if query_lower.contains("top") || query_lower.contains("best")
            || query_lower.contains("worst") {
            return Some(100);
        }

        None
    }
}
```

### Phase 5: GAQL Generation

**Purpose**: Assemble final GAQL with proper syntax and validation.

**Implementation**:

```rust
impl MultiStepRAGAgent {
    fn phase5_gaql_generation(
        &self,
        resource_selection: &ResourceSelectionResult,
        field_selection: &FieldSelectionResult,
        criteria: &CriteriaResult,
    ) -> Result<GAQLResult> {
        let mut query_parts = Vec::new();

        // Build SELECT clause
        let mut select_fields = field_selection.select_fields.clone();

        // Add required segments for grouping
        for segment in &criteria.segments_for_grouping {
            if !select_fields.contains(segment) {
                select_fields.push(segment.clone());
            }
        }

        // Add date segment if needed
        if criteria.needs_date_segment
            && !select_fields.iter().any(|f| f.contains("segments.date"))
        {
            select_fields.push("segments.date".to_string());
        }

        // Format SELECT with indentation
        query_parts.push(format!(
            "SELECT\n  {}",
            select_fields.join(",\n  ")
        ));

        // FROM clause
        query_parts.push(format!("FROM {}", resource_selection.primary_resource));

        // WHERE clause
        let mut where_clauses = criteria.where_clauses.clone();

        // Add DURING clause
        if let Some(during) = &criteria.during_clause {
            where_clauses.push(format!("segments.date DURING {}", during));
        }

        if !where_clauses.is_empty() {
            query_parts.push(format!(
                "WHERE\n  {}",
                where_clauses.join("\n  AND ")
            ));
        }

        // ORDER BY clause
        if !field_selection.order_by_fields.is_empty() {
            let order_parts: Vec<String> = field_selection.order_by_fields
                .iter()
                .map(|o| format!("{} {}", o.field_name, o.direction))
                .collect();
            query_parts.push(format!("ORDER BY {}", order_parts.join(", ")));
        }

        // LIMIT clause
        if let Some(limit) = criteria.limit {
            query_parts.push(format!("LIMIT {}", limit));
        }

        let query = query_parts.join("\n");

        // Validate using existing infrastructure
        let validation_result = self.field_cache.validate_field_selection(&select_fields);

        let validation = GAQLValidationResult {
            is_valid: validation_result.is_valid,
            errors: validation_result.errors.iter().map(|e| e.to_string()).collect(),
            warnings: validation_result.warnings.iter().map(|w| w.to_string()).collect(),
        };

        Ok(GAQLResult {
            query,
            validation,
            pipeline_trace: PipelineTrace::default(), // Populated by caller
        })
    }
}
```

---

## 3. LLM Prompt Templates

### 3.1 Resource Selection System Prompt

```rust
const RESOURCE_SELECTION_SYSTEM_PROMPT: &str = r#"
You are a Google Ads GAQL expert. Your task is to select the most appropriate
resource(s) for a user's query from the Google Ads API.

GUIDELINES:
1. The PRIMARY RESOURCE goes in the FROM clause and determines the grain of data
2. RELATED RESOURCES can be implicitly joined if they are in the primary's
   selectable_with list
3. Choose the most SPECIFIC resource for the user's intent:
   - "keywords" → keyword_view (not campaign)
   - "search terms" → search_term_view
   - "ad performance" → ad_group_ad (not campaign)
   - "campaign performance" → campaign
   - "account overview" → customer

COMMON RESOURCES:
- customer: Account-level data
- campaign: Campaign configuration and performance
- ad_group: Ad group structure
- ad_group_ad: Ad-level performance
- keyword_view: Keyword performance
- search_term_view: Search query reports
- location_view: Geographic performance
- asset_field_type_view: Asset/extension performance

Respond with ONLY valid JSON (no markdown fences):
{
  "primary_resource": "resource_name",
  "related_resources": ["related1", "related2"],
  "confidence": 0.95,
  "reasoning": "Brief explanation"
}
"#;
```

### 3.2 Field Selection System Prompt

```rust
const FIELD_SELECTION_SYSTEM_PROMPT: &str = r#"
You are a Google Ads GAQL expert. Select the specific fields needed for
the user's query from the candidate fields provided.

GUIDELINES:
1. SELECT_FIELDS: Include in SELECT clause
   - Include identifying attributes (e.g., campaign.name, campaign.id)
   - Include requested metrics (e.g., metrics.clicks, metrics.impressions)
   - Include segments for time-series analysis (e.g., segments.date)

2. FILTER_FIELDS: Use in WHERE clause (must be filterable)
   - Use for status filters (e.g., campaign.status = 'ENABLED')
   - Use for type filters (e.g., campaign.advertising_channel_type IN ('PERFORMANCE_MAX'))
   - Use for performance thresholds (e.g., metrics.clicks > 100)

3. ORDER_BY_FIELDS: Use in ORDER BY clause (must be sortable)
   - Use metrics for "top" or "best" queries
   - Use date for time-series trends

IMPORTANT:
- Only select fields marked as selectable
- Only filter on fields marked as filterable
- Always include at least one identifying field (name or id)
- For trend queries, include segments.date

Respond with ONLY valid JSON (no markdown fences):
{
  "select_fields": ["campaign.name", "metrics.clicks"],
  "filter_fields": [
    {"field_name": "campaign.status", "operator": "=", "value": "'ENABLED'", "is_temporal": false}
  ],
  "order_by_fields": [
    {"field_name": "metrics.clicks", "direction": "DESC"}
  ],
  "reasoning": "Brief explanation"
}
"#;
```

---

## 4. Error Handling & Graceful Degradation

### 4.1 Phase-Specific Fallbacks

```rust
impl MultiStepRAGAgent {
    pub async fn generate_gaql(&self, user_query: &str) -> Result<GAQLResult> {
        let start = std::time::Instant::now();

        // Phase 1 with fallback
        let resource_selection = match self.phase1_resource_selection(user_query).await {
            Ok(rs) => rs,
            Err(e) => {
                log::warn!("Phase 1 failed: {}, using keyword fallback", e);
                self.fallback_resource_selection(user_query)
            }
        };

        // Phase 2
        let candidates = self.phase2_field_candidate_retrieval(
            user_query, &resource_selection
        ).await?;

        if candidates.attributes.is_empty() && candidates.metrics.is_empty() {
            return Err(anyhow::anyhow!(
                "No compatible fields found for resource '{}'",
                resource_selection.primary_resource
            ));
        }

        // Phase 3 with fallback
        let field_selection = match self.phase3_field_selection(
            user_query, &resource_selection, &candidates
        ).await {
            Ok(fs) => fs,
            Err(e) => {
                log::warn!("Phase 3 failed: {}, using default selection", e);
                self.fallback_field_selection(&resource_selection, &candidates)
            }
        };

        // Phase 4 (local, unlikely to fail)
        let criteria = self.phase4_criteria_segmentation(user_query, &field_selection);

        // Phase 5
        let mut result = self.phase5_gaql_generation(
            &resource_selection, &field_selection, &criteria
        )?;

        // Populate trace
        result.pipeline_trace.generation_time_ms = start.elapsed().as_millis() as u64;

        Ok(result)
    }

    /// Fallback: keyword-based resource detection (existing logic)
    fn fallback_resource_selection(&self, user_query: &str) -> ResourceSelectionResult {
        let query_lower = user_query.to_lowercase();
        let mut resources = Vec::new();

        if query_lower.contains("keyword") {
            resources.push("keyword_view".to_string());
        }
        if query_lower.contains("search term") {
            resources.push("search_term_view".to_string());
        }
        if query_lower.contains("ad group") || query_lower.contains("adgroup") {
            resources.push("ad_group".to_string());
        }
        if query_lower.contains("ad ") || query_lower.contains("ads ") {
            resources.push("ad_group_ad".to_string());
        }
        if query_lower.contains("campaign") || resources.is_empty() {
            resources.insert(0, "campaign".to_string());
        }

        ResourceSelectionResult {
            primary_resource: resources.first().cloned().unwrap_or("campaign".to_string()),
            related_resources: resources.into_iter().skip(1).collect(),
            confidence: 0.5,
            reasoning: "Fallback: keyword-based detection".to_string(),
        }
    }

    /// Fallback: default field selection from resource metadata
    fn fallback_field_selection(
        &self,
        resource_selection: &ResourceSelectionResult,
        candidates: &FieldCandidates,
    ) -> FieldSelectionResult {
        let mut select_fields = Vec::new();

        // Add key attributes from resource metadata
        if let Some(rm) = self.field_cache.resource_metadata.as_ref()
            .and_then(|m| m.get(&resource_selection.primary_resource))
        {
            select_fields.extend(rm.key_attributes.iter().take(3).cloned());
        }

        // Add common metrics
        let common_metrics = ["metrics.impressions", "metrics.clicks", "metrics.cost_micros"];
        for metric in common_metrics {
            if candidates.metrics.iter().any(|m| m.name == metric) {
                select_fields.push(metric.to_string());
            }
        }

        FieldSelectionResult {
            select_fields,
            filter_fields: Vec::new(),
            order_by_fields: Vec::new(),
            reasoning: "Fallback: default field selection".to_string(),
        }
    }
}
```

---

## 5. Latency Analysis & Parallelization

### 5.1 Estimated Latency by Phase

| Phase | Description | Latency | Notes |
|-------|-------------|---------|-------|
| Phase 1 | Resource Selection (LLM) | 500-1000ms | Depends on LLM provider |
| Phase 2 | Field Retrieval (RAG + filter) | 50-100ms | Local vector search |
| Phase 3 | Field Selection (LLM) | 500-1000ms | Can run with cookbook fetch |
| Phase 4 | Criteria (local) | 10-20ms | Pattern matching |
| Phase 5 | Generation (local) | 10-20ms | String assembly |

**Total**: 1.1-2.2 seconds (vs current ~0.5-1s for single-shot)

### 5.2 Parallelization Opportunities

```rust
impl MultiStepRAGAgent {
    pub async fn generate_gaql_optimized(&self, user_query: &str) -> Result<GAQLResult> {
        // Phase 1 must complete first
        let resource_selection = self.phase1_resource_selection(user_query).await?;

        // Phase 2 (RAG) and cookbook retrieval can run in parallel
        let (candidates, cookbook_examples) = tokio::join!(
            self.phase2_field_candidate_retrieval(user_query, &resource_selection),
            self.retrieve_cookbook_examples(user_query, 3)
        );
        let candidates = candidates?;

        // Phase 3 uses both results
        let field_selection = self.phase3_field_selection_with_examples(
            user_query,
            &resource_selection,
            &candidates,
            &cookbook_examples?,
        ).await?;

        // Phase 4 & 5 are fast local operations
        let criteria = self.phase4_criteria_segmentation(user_query, &field_selection);
        self.phase5_gaql_generation(&resource_selection, &field_selection, &criteria)
    }
}
```

---

## 6. Public API

### 6.1 New Functions in rag.rs

```rust
/// Convert natural language to GAQL using multi-step RAG pipeline
pub async fn convert_to_gaql_multistep(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
) -> Result<String> {
    let agent = MultiStepRAGAgent::init(example_queries, field_cache, config).await?;
    let result = agent.generate_gaql(prompt).await?;
    Ok(result.query)
}

/// Convert with full trace for debugging
pub async fn convert_to_gaql_multistep_with_trace(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
) -> Result<GAQLResult> {
    let agent = MultiStepRAGAgent::init(example_queries, field_cache, config).await?;
    agent.generate_gaql(prompt).await
}
```

### 6.2 CLI Integration

Add flag to `main.rs`:

```rust
/// Use multi-step RAG for improved accuracy (slower)
#[arg(long)]
multistep: bool,

// In generate command handler:
let query = if args.multistep {
    convert_to_gaql_multistep(queries, field_cache, &prompt, &config).await?
} else {
    convert_to_gaql_enhanced(queries, Some(field_cache), &prompt, &config).await?
};
```

---

## 7. Testing Strategy

### 7.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_detect_temporal_requirements() {
        let agent = create_test_agent();

        assert_eq!(
            agent.detect_temporal_requirements("last 7 days"),
            (true, Some("LAST_7_DAYS".to_string()))
        );

        assert_eq!(
            agent.detect_temporal_requirements("show campaigns"),
            (false, None)
        );
    }

    #[test]
    fn test_can_select_together() {
        let agent = create_test_agent();
        let metric = create_field_metadata("metrics.clicks", "METRIC");
        let segment = create_field_metadata("segments.date", "SEGMENT");

        assert!(agent.can_select_together(&metric, &segment));
    }

    #[test]
    fn test_filter_by_compatibility() {
        // Test that campaign attributes pass for campaign resource
        // Test that ad_group attributes fail for keyword_view resource
    }
}
```

### 7.2 Integration Tests

```rust
#[tokio::test]
async fn test_multistep_pipeline_end_to_end() {
    let config = LlmConfig::from_env();
    let field_cache = load_test_field_cache().await;
    let queries = load_test_cookbook();

    let result = convert_to_gaql_multistep_with_trace(
        queries,
        field_cache,
        "show me Performance Max campaigns with ROAS last 30 days",
        &config,
    ).await.unwrap();

    assert!(result.query.contains("FROM campaign"));
    assert!(result.query.contains("PERFORMANCE_MAX"));
    assert!(result.query.contains("DURING LAST_30_DAYS"));
    assert!(result.validation.is_valid);
}
```

---

## 8. Migration & Backwards Compatibility

- **New code path**: `MultiStepRAGAgent` is added alongside existing `EnhancedRAGAgent`
- **Default unchanged**: Existing `convert_to_gaql_enhanced` remains default
- **Opt-in flag**: Use `--multistep` CLI flag for new pipeline
- **No breaking changes**: All existing APIs preserved

---

## 9. Open Questions

1. **Model routing**: Should Phase 1 (critical) use a more capable model than Phase 3?
2. **Caching**: Should we cache resource selection results for similar queries?
3. **Streaming**: Should we stream partial results as each phase completes?
4. **Validation strictness**: Should validation errors be blocking or just warnings?

---

## Appendix A: Example Pipeline Trace

```json
{
  "user_query": "show me Performance Max campaigns with declining ROAS last 30 days",
  "resource_selection": {
    "primary_resource": "campaign",
    "related_resources": ["customer"],
    "confidence": 0.92,
    "reasoning": "User wants campaign-level performance data with ROAS metric"
  },
  "field_candidates_summary": {
    "attributes_count": 45,
    "metrics_count": 120,
    "segments_count": 28,
    "compatible_count": 193,
    "rejected_count": 12
  },
  "field_selection": {
    "select_fields": [
      "campaign.name",
      "campaign.advertising_channel_type",
      "metrics.conversions_value_per_cost",
      "segments.date"
    ],
    "filter_fields": [
      {"field_name": "campaign.advertising_channel_type", "operator": "IN", "value": "('PERFORMANCE_MAX')", "is_temporal": false}
    ],
    "order_by_fields": [
      {"field_name": "segments.date", "direction": "ASC"}
    ],
    "reasoning": "Selected campaign identifying fields + ROAS metric + date for trend"
  },
  "criteria": {
    "where_clauses": ["campaign.advertising_channel_type IN ('PERFORMANCE_MAX')"],
    "during_clause": "LAST_30_DAYS",
    "segments_for_grouping": [],
    "needs_date_segment": true,
    "limit": null
  },
  "generation_time_ms": 1847
}
```

---

**Document Version:** 1.0
**Last Updated:** 2026-03-10
**Next Steps:** Review and approve before implementation
