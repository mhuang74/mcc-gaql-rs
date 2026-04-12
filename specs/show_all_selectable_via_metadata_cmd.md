# Plan: Add Selectable Fields Display to Metadata Command

## Context

**User Request**: When running `mcc-gaql-gen metadata geographic_view`, show all the fields from `selectable_with` that can be used with this resource, organized into sections: SEGMENTS, METRICS, and OTHER (auto-joined fields).

**Current Behavior**: The metadata command shows only the resource's own fields (attributes, metrics, segments that belong to the resource itself), but NOT the fields from `selectable_with` that can be queried together with the resource.

**Goal**: Display new sections showing selectable fields from `ResourceMetadata.selectable_with`, categorized as:
- **SELECTABLE SEGMENTS**: Fields starting with `segments.*`
- **SELECTABLE METRICS**: Fields starting with `metrics.*`  
- **SELECTABLE OTHER**: Remaining fields (auto-joined from other resources like `customer.*`, `campaign.*`)

## Implementation

### File to Modify

**`crates/mcc-gaql-gen/src/formatter.rs`**

### Changes

#### 1. Add helper function to categorize selectable_with fields

```rust
/// Categorize selectable_with fields into segments, metrics, and other
fn categorize_selectable_with(selectable_with: &[String]) -> (Vec<&str>, Vec<&str>, Vec<&str>) {
    let mut segments = Vec::new();
    let mut metrics = Vec::new();
    let mut other = Vec::new();
    
    for field in selectable_with {
        if field.starts_with("segments.") {
            segments.push(field.as_str());
        } else if field.starts_with("metrics.") {
            metrics.push(field.as_str());
        } else {
            other.push(field.as_str());
        }
    }
    
    segments.sort();
    metrics.sort();
    other.sort();
    
    (segments, metrics, other)
}
```

#### 2. Update `format_full()` for `QueryResult::Resource` (after line 696)

After the resource's own fields display, add:

```rust
// Show selectable_with fields categorized
let (selectable_segments, selectable_metrics, selectable_other) = 
    categorize_selectable_with(&metadata.selectable_with);

if !selectable_segments.is_empty() || !selectable_metrics.is_empty() || !selectable_other.is_empty() {
    output.push_str("\n--- SELECTABLE WITH (auto-joined fields) ---\n\n");
    
    if !selectable_segments.is_empty() {
        output.push_str(&format!("### SELECTABLE SEGMENTS ({})\n", selectable_segments.len()));
        for seg in &selectable_segments {
            output.push_str(&format!("  - {}\n", seg));
        }
        output.push('\n');
    }
    
    if !selectable_metrics.is_empty() {
        output.push_str(&format!("### SELECTABLE METRICS ({})\n", selectable_metrics.len()));
        for metric in &selectable_metrics {
            output.push_str(&format!("  - {}\n", metric));
        }
        output.push('\n');
    }
    
    if !selectable_other.is_empty() {
        output.push_str(&format!("### SELECTABLE OTHER ({})\n", selectable_other.len()));
        for field in &selectable_other {
            output.push_str(&format!("  - {}\n", field));
        }
    }
}
```

#### 3. Update `format_llm()` for `QueryResult::Resource` (after line 536)

Add compact version:

```rust
// Show selectable_with counts
let (selectable_segments, selectable_metrics, selectable_other) = 
    categorize_selectable_with(&metadata.selectable_with);

if !selectable_segments.is_empty() {
    output.push_str(&format!(
        "Selectable segments ({}): {}\n",
        selectable_segments.len(),
        selectable_segments.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
    ));
    if selectable_segments.len() > 10 {
        output.push_str(&format!("  ... and {} more\n", selectable_segments.len() - 10));
    }
}

if !selectable_metrics.is_empty() {
    output.push_str(&format!(
        "Selectable metrics ({}): {}\n",
        selectable_metrics.len(),
        selectable_metrics.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
    ));
    if selectable_metrics.len() > 10 {
        output.push_str(&format!("  ... and {} more\n", selectable_metrics.len() - 10));
    }
}
```

## Verification

```bash
cargo build -p mcc-gaql-gen
mcc-gaql-gen metadata geographic_view --format full
```

Expected output should include:
```
--- SELECTABLE WITH (auto-joined fields) ---

### SELECTABLE SEGMENTS (25)
  - segments.ad_network_type
  - segments.conversion_action
  - segments.date
  - segments.device
  - segments.geo_target_city
  - segments.geo_target_country
  - segments.geo_target_metro
  - segments.geo_target_region
  ...

### SELECTABLE METRICS (33)
  - metrics.all_conversions
  - metrics.clicks
  - metrics.conversions
  - metrics.cost_micros
  - metrics.impressions
  ...

### SELECTABLE OTHER (8)
  - customer.currency_code
  - customer.descriptive_name
  - customer.id
  ...
```

Compare with:
```bash
mcc-gaql-gen metadata location_view --format full
```

Which should show different segments (no `segments.geo_target_*` fields).
