# DynamicMutationBuilder CLI Integration — POC Specification

## Overview

This spec defines how the `DynamicMutationBuilder` from `googleads-rs` (`/rust_dev_cache/projects/googleads/googleads-rs/src/lib.rs:77-218`) is exposed through the `mcc-gaql` CLI. The builder provides reflection-based mutation construction for **any** Google Ads API v23 resource type, supporting Update, Create, and Remove operations via dot-separated field paths and string values. The CLI currently lacks all mutation capability — it is read-only (search + validate). This POC bridges that gap with a `mutate` subcommand that reuses the existing auth, config, and client infrastructure.

**Key design principle**: The `DynamicMutationBuilder` is a *generic* engine — it knows nothing about Campaigns or BiddingStrategies specifically. The CLI integration must preserve this generality while providing enough guardrails to prevent user error on a write path. Domain-specific sugar (e.g., the `update-bidding` subcommand from `specs/pmax_bidding_strategy_updates.md`) layers on top of this generic primitive.

---

## 1. CLI Command Design

### 1.1 Subcommand: `mutate`

Add a `mutate` subcommand to the existing flat CLI. The current `mcc-gaql` CLI has no subcommands — all args are flat (`mcc-gaql "SELECT ..." --profile X`). Introducing subcommands requires a two-phase parse strategy:

```
mcc-gaql mutate --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=3.5" \
  --set "target_roas.cpc_bid_ceiling_micros=5000000" \
  --dry-run
```

### 1.2 Subcommand Enumeration

```
mcc-gaql mutate          — Generic mutation via DynamicMutationBuilder
mcc-gaql <GAQL query>    — Existing query flow (unchanged)
mcc-gaql --validate ...  — Existing validation flow (unchanged)
mcc-gaql --setup         — Existing setup wizard (unchanged)
...                      — All other existing flags unchanged
```

### 1.3 Subcommand Definition (clap derive)

```rust
// crates/mcc-gaql/src/args.rs

#[derive(Parser)]
pub enum Command {
    /// Mutate a Google Ads resource using reflection-based field paths
    Mutate {
        /// Resource type name (CamelCase protobuf name, e.g. Campaign, AdGroup, BiddingStrategy)
        #[clap(long)]
        resource: String,

        /// Full resource name (e.g. "customers/1234567890/campaigns/456")
        #[clap(long)]
        resource_name: String,

        /// Operation type: update (default), create, remove
        #[clap(long, default_value = "update")]
        operation: MutationOpCli,

        /// Field assignment in "field_path=value" format. Repeat for multiple fields.
        /// Nested paths use dot notation: "target_roas.target_roas=3.5"
        /// String values with spaces must be quoted: "name=My Campaign"
        #[clap(long = "set", multiple_occurrences(true))]
        field_set: Vec<String>,

        /// Dry-run: validate the mutation without applying it (sets validate_only=true)
        #[clap(long)]
        dry_run: bool,

        /// Read field assignments from a JSON file (alternative to multiple --set flags)
        /// Format: [{"field_path": "...", "value": "..."}, ...]
        #[clap(long)]
        field_set_file: Option<String>,

        /// Show the constructed mutation request without sending it (offline preview)
        #[clap(long)]
        preview: bool,

        /// Continue on partial failures (sets partial_failure=true, default)
        #[clap(long, default_value = "true")]
        partial_failure: bool,

        // --- Shared auth/config flags (same semantics as top-level) ---
        #[clap(short = 'c', long)]
        customer_id: Option<String>,

        #[clap(short = 'm', long = "mcc-id")]
        mcc_id: Option<String>,

        #[clap(short = 'p', long)]
        profile: Option<String>,

        #[clap(short = 'u', long)]
        user_email: Option<String>,

        #[clap(long)]
        remote_auth: bool,
    },
}
```

### 1.4 MutationOpCli Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOpCli {
    Update,
    Create,
    Remove,
}

impl FromStr for MutationOpCli {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "update" => Ok(Self::Update),
            "create" => Ok(Self::Create),
            "remove" => Ok(Self::Remove),
            _ => Err(format!(
                "Invalid operation '{}'. Valid: update, create, remove",
                s
            )),
        }
    }
}
```

### 1.5 Backward Compatibility: Two-Phase Parse

The existing flat CLI must continue to work unchanged. The approach:

1. Try to parse as `mutate` subcommand first (clap `#[command(subcommand)]`).
2. If no subcommand matched, fall back to the existing flat `Cli` struct (positional GAQL query + flags).
3. If neither a subcommand nor a positional query is provided, try reading from stdin (existing behavior at `args.rs:139-161`).

```rust
#[derive(Parser)]
pub struct Cli {
    /// Subcommand (mutate, etc.)
    #[command(subcommand)]
    pub command: Option<Command>,

    // --- All existing flat args remain unchanged ---
    pub gaql_query: Option<String>,
    #[clap(short = 'q', long)]
    pub stored_query: Option<String>,
    // ... rest of existing fields unchanged ...
}
```

When `command` is `Some(Command::Mutate { .. })`, dispatch to the mutation handler. Otherwise, the existing flat-arg flow runs unchanged.

---

## 2. Argument Parsing for Field Paths and Values

### 2.1 `--set` Flag Syntax

Each `--set` flag is a `key=value` pair where:

- **key**: Dot-separated field path matching the protobuf field name hierarchy (e.g., `target_roas.target_roas`, `name`, `status`)
- **value**: String representation of the value; type coercion happens at build time via `DynamicMutationBuilder::set_field()` → `coerce_value()` (`lib.rs:231-283`)

```
--set "target_roas.target_roas=3.5"
--set "name=My Campaign Name"
--set "status=PAUSED"
--set "target_cpa.target_cpa_micros=50000000"
```

### 2.2 Parsing Rules

```rust
fn parse_field_set(raw: &str) -> anyhow::Result<FieldUpdate> {
    // Split on first '=' only — values may contain '=' characters
    let (path, value) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("Invalid --set format: '{}'. Expected field_path=value", raw))?;

    let path = path.trim();
    let value = value.trim();

    if path.is_empty() {
        bail!("Empty field path in --set '{}'", raw);
    }

    Ok(FieldUpdate {
        field_path: path.to_string(),
        value: value.to_string(),
    })
}
```

**Key rule**: Split on the *first* `=` only. Values like `campaign.app_campaign_setting.app_id=com.example.app` must not be split at the app ID's dot pattern, and values containing `=` (unlikely but possible in string fields) must survive.

### 2.3 `--field-set-file` JSON Format

For bulk or complex mutations, field assignments can be loaded from a JSON file:

```json
[
  {"field_path": "target_roas.target_roas", "value": "3.5"},
  {"field_path": "target_roas.cpc_bid_ceiling_micros", "value": "5000000"},
  {"field_path": "name", "value": "PMax - Q2 Launch"}
]
```

Parsing:

```rust
fn parse_field_set_file(path: &Path) -> anyhow::Result<Vec<FieldUpdate>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read field-set file: {}", path.display()))?;
    let updates: Vec<FieldUpdate> = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse field-set file as JSON: {}", path.display()))?;
    Ok(updates)
}
```

### 2.4 Type Coercion Reference

The `coerce_value()` function in `googleads-rs` (`lib.rs:231-283`) handles string→protobuf conversion:

| Protobuf Kind | CLI string example | Coerced to |
|---------------|-------------------|------------|
| `Double` | `3.5` | `f64` |
| `Float` | `0.5` | `f32` |
| `Int32` | `100` | `i32` |
| `Int64` | `50000000` | `i64` |
| `UInt32` | `1` | `u32` |
| `UInt64` | `2` | `u64` |
| `Bool` | `true` | `bool` |
| `String` | `My Campaign` | `String` (pass-through) |
| `Enum` | `PAUSED` | Enum number (name lookup first, then numeric fallback) |

**Enum handling**: The coercion first tries to match by enum value name (case-sensitive protobuf name, e.g., `PAUSED`), then falls back to numeric parsing. This means users can write `--set "status=PAUSED"` rather than needing to know the numeric enum value.

### 2.5 Nested Message Auto-Creation

The `set_field_path_value()` traversal (`lib.rs:285-339`) automatically creates intermediate messages when traversing a nested path. For example:

```
--set "target_roas.target_roas=3.5"
```

This creates a `TargetRoas` message instance, sets `target_roas=3.5` on it, then sets `target_roas` on the parent `Campaign`. The user does not need to explicitly create intermediate messages.

### 2.6 Oneof Handling

When a field belongs to a protobuf `oneof`, setting it automatically clears the previous variant (prost-reflect behavior). For example, on `Campaign.campaign_bidding_strategy`:

```
# If the campaign currently has target_cpa, setting target_roas clears it
--set "target_roas.target_roas=3.5"    # switches from TARGET_CPA → TARGET_ROAS
```

No explicit oneof management is needed from the CLI. This is a critical safety property — the user cannot accidentally set two oneof variants simultaneously.

---

## 3. Integration with Existing mcc-gaql Query Flow

### 3.1 Auth & Config Reuse

The `mutate` subcommand reuses the exact same auth and config resolution path as the query flow:

1. **Config loading**: `config::load(profile)` → `ResolvedConfig::from_args_and_config()`
2. **Auth**: `googleads::get_api_access(&ApiAccessConfig { ... })` → `GoogleAdsAPIAccess`
3. **Client construction**: `GoogleAdsServiceClient::with_interceptor(channel, api_context)`

No new auth mechanisms, token storage, or config fields are needed. The `GoogleAdsServiceClient` already exposes the `mutate()` RPC method — it's just never called from the CLI today.

### 3.2 The `mutate()` RPC

The `GoogleAdsService` protobuf includes a `MutateGoogleAds` RPC that accepts `MutateGoogleAdsRequest` — this is exactly the type that `DynamicMutationBuilder::build()` produces. This is the **unified mutation API** that accepts operations for any resource type.

Current query flow in `googleads.rs` (`googleads.rs:303-411`):

```
GoogleAdsServiceClient::search_stream() → Streaming<SearchGoogleAdsStreamResponse>
```

New mutation flow:

```
GoogleAdsServiceClient::mutate(MutateGoogleAdsRequest) → MutateGoogleAdsResponse
```

Both use the same `GoogleAdsServiceClient` with the same interceptor. The only difference is the RPC method.

### 3.3 New Function: `googleads::mutate_resource()`

```rust
// crates/mcc-gaql/src/googleads.rs

use googleads_rs::{DynamicMutationBuilder, FieldUpdate, MutationOp};
use googleads_rs::google::ads::googleads::v23::services::{
    MutateGoogleAdsRequest, MutateGoogleAdsResponse,
};

pub async fn mutate_resource(
    api_context: GoogleAdsAPIAccess,
    resource_type: &str,
    customer_id: &str,
    resource_name: &str,
    operation: MutationOp,
    field_updates: Vec<FieldUpdate>,
    validate_only: bool,
    partial_failure: bool,
) -> Result<MutateGoogleAdsResponse> {
    let mut builder = DynamicMutationBuilder::new(resource_type, customer_id);
    builder.operation_type(operation);
    builder.validate_only(validate_only);
    builder.partial_failure(partial_failure);

    for update in field_updates {
        builder.set_field(&update.field_path, &update.value);
    }

    let request: MutateGoogleAdsRequest = builder
        .build(resource_name)
        .context("Failed to build mutation request")?;

    let mut client = GoogleAdsServiceClient::with_interceptor(
        api_context.channel.clone(),
        api_context,
    );

    let response = client
        .mutate(request)
        .await
        .map_err(|status| {
            let details = String::from_utf8_lossy(status.details())
                .trim()
                .replace(|c: char| !c.is_ascii(), "")
                .replace("%", " ")
                .replace("\n", " ")
                .replace("\r", " ");
            if details.is_empty() {
                anyhow::anyhow!("{}", status.message())
            } else {
                anyhow::anyhow!("{}: {}", status.message(), details)
            }
        })?;

    Ok(response.into_inner())
}
```

### 3.4 Query-Then-Mutate Pattern (for future domain-specific subcommands)

A common pattern is: query current state → compute new values → mutate. The `DynamicMutationBuilder` POC supports this by allowing the existing query flow and the new mutation flow to share the same `GoogleAdsAPIAccess` and `GoogleAdsServiceClient`.

Example flow for a future `update-bidding` subcommand:

```
1. GAQL query: SELECT campaign.resource_name, campaign.target_roas.target_roas FROM campaign WHERE campaign.id = X
2. Read current value from GoogleAdsRow
3. Compute new value (e.g., current * 1.2)
4. DynamicMutationBuilder::new("Campaign", customer_id).set_field("target_roas.target_roas", new_value).build(resource_name)
5. client.mutate(request)
```

Steps 1-2 use the existing `gaql_query_with_client()`. Steps 3-5 use `mutate_resource()`. Both share `api_context`.

### 3.5 DynamicMutationBuilder vs. Per-Resource Service Clients

| Approach | Pros | Cons |
|----------|------|------|
| **DynamicMutationBuilder + GoogleAdsService.Mutate** (chosen) | Single client; any resource type; no irregular pluralization; supports multi-resource transactions; reflection handles field masks automatically | Serialization overhead (DynamicMessage → transcode → static type); no compile-time field safety |
| **Per-resource service clients** (e.g., CampaignServiceClient) | Compile-time field safety; direct field access; no reflection overhead | 67+ service clients to manage; irregular plural names (BiddingStrategies, AdGroupCriteria); each client needs its own interceptor setup; no cross-resource transactions |

The DynamicMutationBuilder approach is the correct choice for a **generic CLI tool** where the user specifies resource types at runtime. Domain-specific subcommands (like `update-bidding`) may later use per-resource clients for compile-time safety on well-trodden paths, but the generic `mutate` subcommand should use the builder.

---

## 4. Error Handling and Validation

### 4.1 Validation Layers

Validation occurs at three layers, from earliest to latest:

| Layer | When | What | Failure Mode |
|-------|------|------|-------------|
| **L1: CLI parsing** | `clap` parse | Required args present; `--operation` value valid; `--set` format correct; `--field-set-file` readable JSON | Exit with usage message |
| **L2: Pre-flight** | Before `build()` | Resource type exists in DescriptorPool; field paths exist on the resource type; value coercion succeeds for each field | `anyhow::Error` with field-level detail |
| **L3: API validation** | `mutate()` response | Google Ads API rejects the request (field immutable, value out of range, permission denied, etc.) | `tonic::Status` with error details |

### 4.2 L1: CLI Parsing Errors

Handled by `clap` automatically:

```
# Missing required flags
$ mcc-gaql mutate --resource Campaign
error: the following required arguments were not provided:
  --resource-name <RESOURCE_NAME>

# Invalid operation type
$ mcc-gaql mutate --resource Campaign --resource-name X --operation delete
error: Invalid operation 'delete'. Valid: update, create, remove

# Malformed --set
$ mcc-gaql mutate --resource Campaign --resource-name X --set "no_equals_sign"
error: Invalid --set format: 'no_equals_sign'. Expected field_path=value
```

### 4.3 L2: Pre-flight Validation (Offline, Before API Call)

This is a **new capability** enabled by the `DescriptorPool` inside `googleads-rs`. Before sending any request, we validate field paths and value types against the protobuf schema.

```rust
fn validate_mutation_locally(
    resource_type: &str,
    field_updates: &[FieldUpdate],
) -> anyhow::Result<()> {
    let pool = googleads_rs::descriptor_pool();

    // 1. Resource type exists
    let resource_fqn = format!("google.ads.googleads.v23.resources.{}", resource_type);
    let resource_desc = pool.get_message_by_name(&resource_fqn)
        .ok_or_else(|| anyhow!(
            "Unknown resource type '{}'. Check spelling (case-sensitive CamelCase, e.g. Campaign, AdGroup, BiddingStrategy). \
             Use --show-resources to list available resources.",
            resource_type
        ))?;

    // 2. Each field path resolves on the resource
    for update in field_updates {
        validate_field_path(&resource_desc, &update.field_path, &update.value)?;
    }

    // 3. Operation message type exists
    let op_fqn = format!("google.ads.googleads.v23.services.{}Operation", resource_type);
    if pool.get_message_by_name(&op_fqn).is_none() {
        bail!(
            "No mutation operation type found for resource '{}' (expected '{}'). \
             This resource may not support mutations via the unified API.",
            resource_type, op_fqn
        );
    }

    Ok(())
}

fn validate_field_path(
    msg_desc: &MessageDescriptor,
    path: &str,
    value: &str,
) -> anyhow::Result<()> {
    let segments: Vec<&str> = path.split('.').collect();
    validate_field_path_recursive(msg_desc, &segments, path, value)
}

fn validate_field_path_recursive(
    msg: &MessageDescriptor,
    segments: &[&str],
    full_path: &str,
    value: &str,
) -> anyhow::Result<()> {
    let segment = segments[0];
    let field = msg.get_field_by_name(segment)
        .ok_or_else(|| anyhow!(
            "Field '{}' not found on resource '{}'. Available fields: {}",
            segment,
            msg.name(),
            msg.fields().map(|f| f.name().to_string()).collect::<Vec<_>>().join(", ")
        ))?;

    if segments.len() == 1 {
        // Leaf: validate value coercion
        googleads_rs::coerce_value(value, &field)
            .with_context(|| format!("Invalid value '{}' for field '{}' (type: {:?})", value, full_path, field.kind()))?;
        Ok(())
    } else {
        // Intermediate: must be a message field
        match field.kind() {
            Kind::Message(nested) => {
                validate_field_path_recursive(&nested, &segments[1..], full_path, value)
            }
            _ => bail!(
                "Cannot traverse into non-message field '{}' (type: {:?}) in path '{}'",
                segment, field.kind(), full_path
            ),
        }
    }
}
```

**Error messages are designed for CLI users**: they list available fields on the resource, suggest corrections, and reference the full path that failed.

### 4.4 L3: API Validation Errors

The `tonic::Status` from the `mutate()` call contains Google Ads API error details. These are parsed and displayed in the same format as existing query errors (`googleads.rs:352-366`):

```
Error: GoogleAdsClient mutate error. Account: 1234567890, Message: 'Field modification not allowed', Details: '...
  errorCode { field_error: IMMUTABLE_FIELD }
  location { field_path_elements { field_name: "campaign.id" } }
...'
```

Common API errors and their CLI-friendly messages:

| API Error | Scenario | CLI Message |
|-----------|----------|-------------|
| `IMMUTABLE_FIELD` | Trying to set `id`, `resource_name` | `"Field '{}' is immutable and cannot be updated"` |
| `REQUIRED_FIELD_MISSING` | Create without required fields | `"Missing required field '{}' for create operation"` |
| `FIELD_NOT_SET` | Update with empty field mask | `"No fields specified for update. Use --set to specify fields."` |
| `PERMISSION_DENIED` | No write access to account | `"Permission denied for account '{}'. Check OAuth scope and account access."` |
| `RESOURCE_NOT_FOUND` | Invalid resource name | `"Resource '{}' not found. Verify the resource name format: customers/{cid}/campaigns/{id}"` |

### 4.5 Dry-Run Behavior

`--dry-run` sets `validate_only: true` on the `MutateGoogleAdsRequest`. The API validates the mutation (field types, value ranges, required fields, permissions) without persisting changes.

```rust
let validate_only = args.dry_run;
```

Output for dry-run:

```
[dry-run] Validating mutation...
[dry-run] Resource: Campaign
[dry-run] Operation: Update
[dry-run] Resource name: customers/1234567890/campaigns/456
[dry-run] Fields:
[dry-run]   target_roas.target_roas = 3.5
[dry-run]   target_roas.cpc_bid_ceiling_micros = 5000000
[dry-run] Validation PASSED — mutation would succeed if applied
```

### 4.6 `--preview` Behavior

`--preview` constructs the `MutateGoogleAdsRequest` but does **not** send it. This is an offline-only mode for inspecting the constructed request:

```rust
if preview {
    let request = builder.build(resource_name)?;
    println!("MutateGoogleAdsRequest:");
    println!("  customer_id: {}", request.customer_id);
    println!("  validate_only: {}", request.validate_only);
    println!("  partial_failure: {}", request.partial_failure);
    println!("  operations: {} operation(s)", request.mutate_operations.len());
    // ... print field mask paths, field values
    return Ok(());
}
```

This is useful for debugging field paths and verifying the DynamicMutationBuilder's output before any API call.

---

## 5. Example Usage Scenarios

### 5.1 Basic Campaign Update

```bash
# Update target ROAS on a campaign
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=3.5"
```

### 5.2 Multi-Field Update

```bash
# Update target ROAS and bid ceiling simultaneously
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=3.5" \
  --set "target_roas.cpc_bid_ceiling_micros=5000000" \
  --customer-id 1234567890 \
  --profile myprofile
```

### 5.3 Strategy Switch (Oneof Handling)

```bash
# Switch from Target CPA to Target ROAS — oneof clears the old variant automatically
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=4.0" \
  --dry-run
```

### 5.4 Create Operation

```bash
# Create a new budget (required fields only)
mcc-gaql mutate \
  --resource CampaignBudget \
  --resource-name "customers/1234567890/campaignBudgets/-1" \
  --operation create \
  --set "name=Q2 Budget" \
  --set "amount_micros=50000000" \
  --set "delivery_method=STANDARD"
```

### 5.5 Remove Operation

```bash
# Remove a campaign (only resource_name needed)
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/789" \
  --operation remove \
  --dry-run
```

### 5.6 Enum Field by Name

```bash
# Pause a campaign using enum name (not numeric value)
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "status=PAUSED"
```

### 5.7 Field Set from JSON File

```bash
# Complex update from a JSON file
cat > /tmp/fields.json << 'EOF'
[
  {"field_path": "target_roas.target_roas", "value": "3.5"},
  {"field_path": "target_roas.cpc_bid_ceiling_micros", "value": "5000000"},
  {"field_path": "name", "value": "PMax - Q2 Launch"}
]
EOF

mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --field-set-file /tmp/fields.json \
  --dry-run
```

### 5.8 Preview Mode (Offline)

```bash
# Preview the request without calling the API
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=3.5" \
  --preview
```

### 5.9 Query-Then-Mutate (Two-Step)

```bash
# Step 1: Query current state
mcc-gaql "SELECT campaign.resource_name, campaign.target_roas.target_roas FROM campaign WHERE campaign.id = 456" -c 1234567890 -o current.json --format json

# Step 2: Apply mutation based on query result
mcc-gaql mutate \
  --resource Campaign \
  --resource-name "customers/1234567890/campaigns/456" \
  --set "target_roas.target_roas=4.2" \
  --customer-id 1234567890
```

### 5.10 Bidding Strategy Resource (Portfolio)

```bash
# Update a portfolio bidding strategy
mcc-gaql mutate \
  --resource BiddingStrategy \
  --resource-name "customers/1234567890/biddingStrategies/789" \
  --set "target_roas.target_roas_micros=3500000" \
  --profile myprofile \
  --dry-run
```

### 5.11 Ad Group Criterion Update

```bash
# Update a bid on an ad group criterion
mcc-gaql mutate \
  --resource AdGroupCriterion \
  --resource-name "customers/1234567890/adGroupCriteria/456~789" \
  --set "cpc_bid_micros=2500000" \
  --customer-id 1234567890
```

---

## 6. Implementation Phases and Estimated Effort

### Phase 1: Minimal Viable Mutation (3-4 days)

**Goal**: `mutate` subcommand that can send a single Update/Create/Remove operation with `--set` flags and `--dry-run`.

| Task | Effort | Details |
|------|--------|---------|
| Refactor `args.rs` for subcommand support | 0.5 day | Add `Command` enum with `Mutate` variant; two-phase parse (subcommand first, then flat args); update `Cli::validate()` |
| Add `mutate_resource()` to `googleads.rs` | 0.5 day | New async function wrapping `DynamicMutationBuilder::build()` + `client.mutate()`. Reuse existing error formatting pattern from `googleads.rs:352-366`. |
| Add mutation dispatch in `main.rs` | 0.5 day | Match on `Command::Mutate { .. }`, resolve config/auth, call `mutate_resource()`, format output |
| Add `--dry-run` and output formatting | 0.5 day | `validate_only` flag; success/failure output; dry-run banner |
| Manual testing against test account | 1 day | Test Campaign update, Create, Remove with `--dry-run`; verify via Google Ads UI |

**Files modified**:
- `crates/mcc-gaql/src/args.rs` — Add `Command` enum, `MutationOpCli`, `Mutate` variant
- `crates/mcc-gaql/src/main.rs` — Add mutation dispatch branch
- `crates/mcc-gaql/src/googleads.rs` — Add `mutate_resource()`, `DynamicMutationBuilder`/`MutationOp`/`FieldUpdate` imports

**Files unchanged**:
- `crates/mcc-gaql-common/` — No changes (all mutation types come from `googleads-rs`)
- `crates/mcc-gaql/src/config.rs` — `ResolvedConfig` already carries all needed fields
- `Cargo.toml` — No new dependencies (`googleads-rs` already a dependency)

### Phase 2: Pre-flight Validation & Preview (2-3 days)

**Goal**: Offline field-path validation and request preview before any API call.

| Task | Effort | Details |
|------|--------|---------|
| Add `validate_mutation_locally()` | 1 day | Validate resource type in DescriptorPool; walk field paths; coerce values; suggest alternatives on unknown fields |
| Add `--preview` flag | 0.5 day | Build request without sending; print structured output (field mask, values, operation type) |
| Improve error messages | 0.5 day | Map common `tonic::Status` error codes to CLI-friendly messages; list available fields on validation failure |
| Unit tests for validation | 1 day | Test all `coerce_value` types through CLI; test invalid field paths; test unknown resource types |

**Files added**:
- `crates/mcc-gaql/src/mutation_validate.rs` — `validate_mutation_locally()`, `validate_field_path()`

### Phase 3: Batch & File Input (2 days)

**Goal**: `--field-set-file` for JSON-based multi-field input; foundation for batch mutations.

| Task | Effort | Details |
|------|--------|---------|
| Add `--field-set-file` parsing | 0.5 day | Parse JSON file into `Vec<FieldUpdate>`; merge with `--set` flags (CLI flags take precedence on conflict) |
| Add `FieldUpdate` serde support | 0.5 day | Add `Serialize`/`Deserialize` derives to `FieldUpdate` in `googleads-rs`, or create a local mirror type |
| Batch mutation support | 1 day | Accept multiple `--resource-name` entries; build `MutateOperation` per resource via `build_operation()`; combine into single `MutateGoogleAdsRequest`; report per-resource results |

**Files modified**:
- `crates/mcc-gaql/src/args.rs` — Add `--field-set-file` flag
- `crates/mcc-gaql/src/googleads.rs` — Add batch mutation function
- `googleads-rs/src/lib.rs` — Add serde derives to `FieldUpdate` (if not already present)

### Phase 4: Confirmation & Safety (1-2 days)

**Goal**: Interactive confirmation prompt before applying mutations (non-dry-run), with `--yes` escape hatch.

| Task | Effort | Details |
|------|--------|---------|
| Add confirmation prompt | 0.5 day | Use `dialoguer::Confirm` (already a dependency); display operation summary; skip if `--dry-run` or `--preview` |
| Add `--yes` flag | 0.5 day | Skip confirmation; log warning to stderr |
| Add audit logging | 0.5 day | Append JSONL entry to `{cache_dir}/mutation_audit.log` with timestamp, operation, fields, result |

### Phase 5: Query-Mutate Integration (2-3 days)

**Goal**: Enable `mutate` to receive field values computed from a prior GAQL query result, laying groundwork for domain-specific subcommands like `update-bidding`.

| Task | Effort | Details |
|------|--------|---------|
| Add `--from-query` flag | 1.5 days | Accept a GAQL query; execute it; use the result's `resource_name` and selected fields as input to `DynamicMutationBuilder`; support `--set` overrides for computed fields |
| Add `--transform` flag (optional) | 1 day | Simple expression language for computing new values from query results: `--transform "target_roas=target_roas*1.2"` (arithmetic on numeric fields only) |
| Integration tests | 0.5 day | End-to-end: query campaign → compute new ROAS → mutate with dry-run → verify field mask and values |

### Total Estimated Effort

| Phase | Days | Cumulative |
|-------|------|-----------|
| Phase 1: Minimal Viable Mutation | 3-4 | 3-4 |
| Phase 2: Pre-flight Validation & Preview | 2-3 | 5-7 |
| Phase 3: Batch & File Input | 2 | 7-9 |
| Phase 4: Confirmation & Safety | 1-2 | 8-11 |
| Phase 5: Query-Mutate Integration | 2-3 | 10-14 |

**Minimum viable POC**: Phase 1 only (3-4 days). This gets a working `mutate` subcommand with dry-run support into the CLI.

**Recommended POC scope**: Phases 1-2 (5-7 days). Adds pre-flight validation which is critical for a write path — catching typos in field paths before making API calls saves time, quota, and prevents confusing API error messages.

---

## Appendix A: Relationship to Domain-Specific Subcommands

The `mutate` subcommand is a **low-level primitive**. Domain-specific subcommands (like `update-bidding` from `specs/pmax_bidding_strategy_updates.md`) provide a higher-level interface with:

- Natural language input parsing
- Strategy-specific validation (e.g., "target ROAS requires a value > 0")
- Current-state fetching and relative value computation
- Domain-specific confirmation previews

The relationship is:

```
┌──────────────────────────────┐
│  update-bidding subcommand   │  ← Domain-specific (NL input, bidding validation, current-state fetch)
│  (pmax_bidding_strategy_     │
│   updates.md)                │
│                              │
│  Uses: mutate_resource()     │  ← Calls the same backend function
│  OR: CampaignServiceClient   │  ← May use per-resource client for compile-time safety
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│  mutate subcommand           │  ← Generic (any resource, any field path)
│  (this spec)                │
│                              │
│  Uses: DynamicMutationBuilder│  ← Reflection-based, no compile-time field safety
│       + mutate_resource()    │
└──────────────────────────────┘
```

Both paths converge at `GoogleAdsServiceClient::mutate()`.

## Appendix B: Resource Name Format Reference

Google Ads API resource names follow a predictable pattern:

| Resource | Resource Name Format | Example |
|----------|---------------------|---------|
| Campaign | `customers/{cid}/campaigns/{id}` | `customers/1234567890/campaigns/456` |
| AdGroup | `customers/{cid}/adGroups/{id}` | `customers/1234567890/adGroups/789` |
| CampaignBudget | `customers/{cid}/campaignBudgets/{id}` | `customers/1234567890/campaignBudgets/101` |
| BiddingStrategy | `customers/{cid}/biddingStrategies/{id}` | `customers/1234567890/biddingStrategies/202` |
| AdGroupCriterion | `customers/{cid}/adGroupCriteria/{ad_group_id}~{criterion_id}` | `customers/1234567890/adGroupCriteria/789~303` |
| New resource (Create) | `customers/{cid}/{resource}s/-1` | `customers/1234567890/campaignBudgets/-1` |

The `-1` placeholder for Create operations is a Google Ads API convention — the API assigns the actual ID on creation.

## Appendix C: DynamicMutationBuilder API Quick Reference

Source: `googleads-rs/src/lib.rs:77-218`

```rust
// Construction
let mut builder = DynamicMutationBuilder::new("Campaign", "1234567890");

// Configuration
builder.operation_type(MutationOp::Update);  // or Create, Remove
builder.set_field("target_roas.target_roas", "3.5");  // adds FieldUpdate
builder.set_field("name", "My Campaign");
builder.validate_only(true);    // dry-run
builder.partial_failure(true);  // continue on per-operation errors

// Build
let request: MutateGoogleAdsRequest = builder.build("customers/123/campaigns/456")?;

// Or build just the operation (for batching)
let operation: DynamicMessage = builder.build_operation("customers/123/campaigns/456")?;
```

## Appendix D: DescriptorPool Coverage

The `DESCRIPTOR_POOL` (`googleads-rs/src/lib.rs:49-51`) is loaded from the embedded `file_descriptor_set.bin` generated at build time from all v23 proto files. It contains descriptors for:

- **~80 resource types** (Campaign, AdGroup, BiddingStrategy, CampaignBudget, AdGroupCriterion, Customer, etc.)
- **~67 service operation types** (CampaignOperation, AdGroupOperation, etc.)
- **All field descriptors** with type information for coercion

This means `--resource` accepts any valid v23 resource type name, and `--set` accepts any valid field path on that resource. No hardcoded resource/field lists are needed in the CLI code.
