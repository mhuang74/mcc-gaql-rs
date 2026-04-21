# Performance Max Campaign Bidding Strategy Updates — Design Specification

## Overview

Add the ability to update bidding strategies on Performance Max (PMax) campaigns via a **natural language command interface** as the primary user experience. The tool accepts free-form English commands like "set target ROAS to 4.0 on campaign 1234567890" and handles parsing, current-state resolution, confirmation, and mutation automatically.

An explicit-flag **expert mode** (`--expert`) is available for CI/automation scripting but is not the primary interface.

Initial scope: single-campaign updates with mandatory confirmation prompt and audit logging.

---

## 1. Natural Language Interface (Primary)

### 1.1 Entry Point

The `update-bidding` subcommand takes a positional natural language string:

```
mcc-gaql update-bidding "<natural language command>"
```

Examples:

```bash
# Set a target CPA
mcc-gaql update-bidding "set target CPA to 50 on campaign 9876543210"

# Lower target ROAS by 20%
mcc-gaql update-bidding "lower target ROAS by 20% on PMax campaign 9876543210"

# Switch strategy entirely
mcc-gaql update-bidding "switch campaign 9876543210 from target CPA to maximize conversions"

# With profile and account context
mcc-gaql update-bidding --profile myprofile "change target ROAS to 3.5 on campaign 9876543210"

# Dry-run preview
mcc-gaql update-bidding --dry-run "increase target CPA by 10 on campaign 9876543210"
```

### 1.2 Parser Approach

Rule-based parser (regex + keyword extraction). This avoids an LLM dependency, is deterministic, fast, and testable. The parser follows a two-stage design:

**Stage 1 — Classify & Extract**: Determine if the input is a valid bidding update command, then extract structured entities.

**Stage 2 — Resolve & Validate**: For relative changes ("lower by 20%"), fetch the current value via GAQL and compute the absolute target. Validate the final resolved `BiddingStrategyUpdate`.

A future enhancement could integrate with the `mcc-gaql-gen` LLM pipeline for more complex/ambiguous intent resolution.

### 1.3 Command Detection

Input is classified as a "bidding update command" if it contains:
- A **trigger phrase**: `update`, `change`, `set`, `modify`, `adjust`, `switch`, `lower`, `increase`, `raise`, `reduce`, `drop` (and variants)
- A **target domain**: `bidding strategy`, `bid strategy`, `bidding`, `target cpa`, `cpa`, `target roas`, `roas`, `maximize conversions`, `maximize conversion value`, `maximize clicks`
- A **campaign reference**: campaign ID pattern (`\d{10}`) or `PMax` / `Performance Max`

If classification fails, the tool prints a helpful error with supported patterns (see Section 1.7).

### 1.4 Supported Patterns

```
# Direct absolute value
"set target CPA to 50 on campaign 1234567890"
"update target ROAS to 3.5 on campaign 1234567890"
"change campaign 9876543210 to maximize conversions"
"set PMax campaign 1234567890 bidding to target CPA 75"

# Relative value (requires current-state fetch)
"lower target ROAS by 20% on campaign 1234567890"
"increase target CPA by 10 on campaign 1234567890"
"reduce target CPA to 20% below current on campaign 1234567890"
"raise target ROAS by 0.5 on PMax campaign 1234567890"

# Strategy switch
"switch campaign 1234567890 from target CPA to maximize conversions"
"change bidding to target ROAS on PMax campaign 9876543210, set to 4.0"
```

### 1.5 Entity Extraction Rules

| Entity | Regex / Method | Example |
|--------|---------------|---------|
| Campaign ID | `\b(\d{10})\b` | `1234567890` |
| Strategy type | Keyword map: `{target cpa → target-cpa, maximize conversions → maximize-conversions, ...}` | — |
| Absolute value | `\b(\d+\.?\d*)\b` near strategy keyword (not preceded by "by") | `50`, `3.5` |
| Relative direction | `(increase|decrease|raise|lower|reduce|drop)\s+(by|to)` | `lower by` |
| Relative amount | `(\d+\.?\d*)%?` after direction | `20%`, `10` |
| "below/above current" | `(below|above|from)\s+current` | `20% below current` |

### 1.6 Relative Value Resolution

When the user specifies a relative change (e.g., "20% lower than current"), the system must:

1. Query the campaign's current bidding strategy and value via GAQL:
   ```sql
   SELECT campaign.resource_name,
          campaign.bidding_strategy_type,
          campaign.target_cpa.target_cpa_micros,
          campaign.target_roas.target_roas
   FROM campaign
   WHERE campaign.id = <CAMPAIGN_ID>
   ```
   Note: `campaign.resource_name` is included because it is required for the mutation call (see Section 3.3).

2. Read the current value from `row.campaign` (direct struct access, not `row.get()`).

3. Compute the new value:
   - "lower by 20%" → `current * 0.80`
   - "increase by 10" → `current + 10`
   - "20% below current" → `current * 0.80`

4. Construct the `BiddingStrategyUpdate` and proceed to confirmation.

### 1.7 Error & Help Output

When the parser cannot classify the input:

```
Error: Could not interpret command as a bidding strategy update.

Supported patterns:
  "set target CPA to <value> on campaign <ID>"
  "set target ROAS to <value> on campaign <ID>"
  "lower/increase target CPA/ROAS by <amount>% on campaign <ID>"
  "switch campaign <ID> from <strategy> to <strategy>"
  "change campaign <ID> to maximize conversions [with target CPA <value>]"

Use --expert for explicit flag-based input (automation/scripts).
Run with --help for full option list.
```

### 1.8 Ambiguity Handling

When the parser cannot fully resolve the command (e.g., campaign ID not found, conflicting signals), it should:

1. **Missing campaign ID**: Error with "Specify a campaign ID (10-digit number) in your command."
2. **Missing target value for a value-required strategy**: Error with "Target CPA/ROAS requires a value. Try: 'set target CPA to 50 on campaign ...'"
3. **Relative change on a strategy that has no current value**: Error with "Campaign has no current target CPA set. Use an absolute value: 'set target CPA to 50 on campaign ...'"
4. **Multiple campaign IDs in input**: Use the first one; warn about extras.

---

## 2. Expert Mode (Secondary — Automation/CI)

### 2.1 When to Use Expert Mode

Expert mode provides explicit flag-based input for:
- CI/CD pipelines and scripting where natural language is impractical
- Automation tools that construct commands programmatically
- Cases where the NL parser cannot interpret the command

### 2.2 Activation

Add `--expert` flag to the `update-bidding` subcommand. When present, the positional NL argument is ignored and explicit flags are required instead:

```bash
# Expert mode: all parameters via flags
mcc-gaql update-bidding --expert \
  --customer-id 1234567890 \
  --campaign-id 9876543210 \
  --strategy target-cpa \
  --target-cpa 50.00

# Expert mode with dry-run
mcc-gaql update-bidding --expert \
  --profile myprofile \
  --campaign-id 9876543210 \
  --strategy target-roas \
  --target-roas 4.0 \
  --dry-run
```

Without `--expert`, the positional argument is parsed as natural language. With `--expert`, it is ignored (or an error if no positional is provided is fine too — the flags are the source of truth).

### 2.3 Expert Mode Flags

| Flag | Short | Required (expert) | Description |
|------|-------|-------------------|-------------|
| `--campaign-id` | `-c` | Yes | Campaign ID to update (10-digit, hyphens OK) |
| `--strategy` | | Yes | Bidding strategy type |
| `--target-cpa` | | Conditional | Target CPA in account currency (e.g. `50.00`) |
| `--target-roas` | | Conditional | Target ROAS as decimal (e.g. `3.5` for 350%) |
| `--target-cpa-micros` | | Conditional | Target CPA in micros (alternative to `--target-cpa`) |
| `--cpc-bid-ceiling` | | No | CPC bid ceiling in account currency |
| `--cpc-bid-floor` | | No | CPC bid floor in account currency |
| `--customer-id` | | Yes* | Customer (child) account ID |
| `--profile` | `-p` | No | Config profile |
| `--mcc-id` | `-m` | No | MCC ID |
| `--user-email` | `-u` | No | OAuth2 user email |
| `--remote-auth` | | No | Remote OAuth flow |
| `--dry-run` | | No | Validate-only: show proposed changes without applying |
| `--yes` | `-y` | No | Skip confirmation prompt (CI use) |
| `--no-audit` | | No | Skip audit log entry (not recommended) |

\* `--customer-id` can be omitted if the config profile provides it.

### 2.4 Strategy Values (Expert Mode)

| `--strategy` value | Required params | Optional params | PMax compatible |
|---------------------|----------------|-----------------|----------------|
| `maximize-conversions` | — | `--target-cpa` / `--target-cpa-micros`, `--cpc-bid-ceiling`, `--cpc-bid-floor` | Yes |
| `maximize-conversion-value` | — | `--target-roas`, `--cpc-bid-ceiling`, `--cpc-bid-floor` | Yes |
| `target-cpa` | `--target-cpa` or `--target-cpa-micros` | `--cpc-bid-ceiling`, `--cpc-bid-floor` | Yes |
| `target-roas` | `--target-roas` | `--cpc-bid-ceiling`, `--cpc-bid-floor` | Yes |
| `maximize-clicks` | — | `--cpc-bid-ceiling` | No (not PMax; included for future use) |

### 2.5 Validation Rules (Expert Mode)

1. `--campaign-id` is always required.
2. `--strategy` must be one of the 5 supported values.
3. `--target-cpa` / `--target-cpa-micros` required when `--strategy` is `target-cpa`; must be > 0.
4. `--target-roas` required when `--strategy` is `target-roas`; must be > 0.
5. `--target-cpa` and `--target-cpa-micros` are mutually exclusive.
6. `--cpc-bid-ceiling` and `--cpc-bid-floor` only valid for portfolio-compatible strategies; ceiling must be >= floor when both present.
7. `--yes` without `--dry-run` prints an extra warning to stderr.

---

## 3. Data Model

### 3.1 Core Types

```rust
// crates/mcc-gaql-common/src/bidding.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiddingStrategyKind {
    MaximizeConversions,
    MaximizeConversionValue,
    TargetCpa,
    TargetRoas,
    MaximizeClicks,
}

impl BiddingStrategyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MaximizeConversions => "maximize-conversions",
            Self::MaximizeConversionValue => "maximize-conversion-value",
            Self::TargetCpa => "target-cpa",
            Self::TargetRoas => "target-roas",
            Self::MaximizeClicks => "maximize-clicks",
        }
    }
}

impl std::str::FromStr for BiddingStrategyKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "maximize-conversions" => Ok(Self::MaximizeConversions),
            "maximize-conversion-value" => Ok(Self::MaximizeConversionValue),
            "target-cpa" => Ok(Self::TargetCpa),
            "target-roas" => Ok(Self::TargetRoas),
            "maximize-clicks" => Ok(Self::MaximizeClicks),
            _ => Err(format!(
                "Unknown strategy '{}'. Valid: maximize-conversions, maximize-conversion-value, target-cpa, target-roas, maximize-clicks",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiddingStrategyUpdate {
    pub campaign_id: String,
    pub customer_id: String,
    pub strategy: BiddingStrategyKind,
    pub target_cpa_micros: Option<i64>,
    pub target_roas: Option<f64>,
    pub cpc_bid_ceiling_micros: Option<i64>,
    pub cpc_bid_floor_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentBiddingState {
    pub campaign_id: String,
    pub campaign_name: String,
    pub strategy_type: String,
    pub target_cpa_micros: Option<i64>,
    pub target_roas: Option<f64>,
    pub cpc_bid_ceiling_micros: Option<i64>,
    pub cpc_bid_floor_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiddingChangePreview {
    pub campaign_id: String,
    pub campaign_name: String,
    pub current: CurrentBiddingState,
    pub proposed: BiddingStrategyUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: String,        // ISO 8601 UTC
    pub user_email: Option<String>,
    pub customer_id: String,
    pub campaign_id: String,
    pub campaign_name: String,
    pub old_strategy: String,
    pub old_value: Option<String>,
    pub new_strategy: String,
    pub new_value: Option<String>,
    pub changes_applied: Vec<String>,
    pub dry_run: bool,
    pub input_mode: InputMode,
    pub raw_input: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMode {
    NaturalLanguage,
    Expert,
}
```

### 3.2 Currency Conversion Helper

The system accepts human-readable currency values (e.g., "target CPA 50" in NL or `--target-cpa 50.00` in expert mode), but the Google Ads API uses micros (integers):

```rust
// crates/mcc-gaql-common/src/bidding.rs

pub fn currency_to_micros(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}

pub fn micros_to_currency(micros: i64) -> f64 {
    micros as f64 / 1_000_000.0
}
```

### 3.3 Campaign Query for Current State

GAQL query to fetch a campaign's current bidding state:

```sql
SELECT
    campaign.resource_name,
    campaign.id,
    campaign.name,
    campaign.bidding_strategy_type,
    campaign.target_cpa.target_cpa_micros,
    campaign.target_roas.target_roas,
    campaign.maximize_conversions.target_cpa_micros,
    campaign.maximize_conversion_value.target_roas
FROM campaign
WHERE campaign.id = {CAMPAIGN_ID}
LIMIT 1
```

**Important**: The query must include `campaign.resource_name` — it is required as the identifier when constructing the `CampaignOperation::Update` mutation. The `GoogleAdsRow` returned by the streaming search has a `pub campaign: Option<Campaign>` field providing direct struct access. Use `row.campaign` to read the current bidding state, **not** the existing `row.get(path)` method (which only returns `String`).

### 3.4 Fetch-for-Reading, Construct-Fresh-for-Writing Pattern

Research into the generated prost types confirms that `GoogleAdsRow.campaign` returns a full `Campaign` struct with all `pub` fields, and the `CampaignBiddingStrategy` oneof can be directly set. However, the recommended pattern is a **hybrid approach**:

- **Read** current state from `row.campaign` (direct struct access from GAQL results) — used for the confirmation preview and for relative value computation in NL commands.
- **Write** by constructing a **new minimal `Campaign`** with only `resource_name` + the new `campaign_bidding_strategy` variant, using `..Default::default()` for all other fields.

**Why not mutate the fetched Campaign in-place?**

1. **Stale unselected fields** — A GAQL-returned `Campaign` only populates fields that were in the SELECT clause. Unselected fields default to `0`, `None`, or empty. Mutating in-place and sending the modified object risks overwriting real values with these defaults if the `update_mask` is overly broad.
2. **Non-optional scalar ambiguity** — Some fields on `Campaign` and on strategy sub-types (e.g., `MaximizeConversions { cpc_bid_ceiling_micros: i64 }`) use plain `i64`/`f64` rather than `Option<T>`. A default of `0` is indistinguishable from "intentionally set to zero." A fresh construction with `..Default::default()` is explicit about which fields are intentionally set.
3. **Clean separation of concerns** — The read path (display current state) and write path (construct desired state) have different shapes. Keeping them separate avoids coupling the SELECT clause to the mutation contract.

**Example — switching to Target ROAS:**

```rust
// READ: access current state from GAQL result
let current_campaign = row.campaign.as_ref()
    .ok_or_else(|| anyhow!("Campaign not found in GAQL row"))?;
let current_strategy_type = current_campaign.bidding_strategy_type;

// WRITE: construct a fresh Campaign for the mutation
let campaign = Campaign {
    resource_name: current_campaign.resource_name.clone(),
    campaign_bidding_strategy: Some(
        campaign::CampaignBiddingStrategy::TargetRoas(TargetRoas {
            target_roas: Some(3.5),
            ..Default::default()
        })
    ),
    ..Default::default()
};

let operation = CampaignOperation {
    operation: Some(campaign_operation::Operation::Update(campaign)),
    update_mask: Some(FieldMask {
        paths: vec!["target_roas".to_string()],
    }),
};
```

---

## 4. Confirmation Flow

### 4.1 Interactive Flow

```
╔══════════════════════════════════════════════════════╗
║          BIDDING STRATEGY UPDATE PREVIEW            ║
╠══════════════════════════════════════════════════════╣
║  Campaign:     PMax - Holiday Sale (9876543210)     ║
║  Account:      1234567890                            ║
╠══════════════════════════════════════════════════════╣
║  CURRENT                                            ║
║  Strategy:     TARGET_CPA                            ║
║  Target CPA:   $50.00                                ║
╠══════════════════════════════════════════════════════╣
║  PROPOSED                                           ║
║  Strategy:     TARGET_ROAS                           ║
║  Target ROAS:  4.0 (400%)                            ║
╠══════════════════════════════════════════════════════╣
║  CHANGES                                            ║
║  • Strategy: TARGET_CPA → TARGET_ROAS               ║
║  • New target ROAS: 4.0                              ║
╚══════════════════════════════════════════════════════╝

Apply this change? [y/N]:
```

### 4.2 Flow Steps

Both NL and expert modes converge at this point. The flow is identical regardless of input mode:

1. **Fetch current state** — Run GAQL query to get campaign's current bidding strategy and parameters.
2. **Build preview** — Compare current vs proposed, generate `BiddingChangePreview`.
3. **Display preview** — Render formatted diff table to stdout.
4. **Prompt** — Use `dialoguer::Confirm` (already a dependency) for yes/no.
5. **Apply** — If confirmed, call `CampaignServiceClient::mutate_campaigns()`.
6. **Log** — Append `AuditLogEntry` to audit log file.
7. **Report** — Print result (resource name or error).

### 4.3 Dry-Run Mode

With `--dry-run`:
- Steps 1–3 execute normally.
- Step 4 is skipped (no prompt needed).
- Step 5 uses `validate_only: true` on the `MutateCampaignsRequest`, which validates the mutation without persisting it.
- Step 6 logs with `dry_run: true`.
- Step 7 reports validation result.

### 4.4 Non-Interactive Mode

With `--yes` (expert mode only):
- Step 4 is skipped.
- The mutation is applied directly.
- A warning is logged at `warn` level.
- Intended for CI/automation pipelines where the caller has already reviewed changes.

---

## 5. Audit Logging

### 5.1 Log Format

JSONL (one JSON object per line), append-only. This format is:
- Machine-parseable for downstream tooling.
- Resilient to partial writes (each line is independent).
- Compatible with log aggregation systems.

### 5.2 Log Entry Schema

```json
{
  "timestamp": "2026-04-21T14:32:00.123Z",
  "user_email": "user@example.com",
  "customer_id": "1234567890",
  "campaign_id": "9876543210",
  "campaign_name": "PMax - Holiday Sale",
  "old_strategy": "TARGET_CPA",
  "old_value": "50000000 micros ($50.00)",
  "new_strategy": "TARGET_ROAS",
  "new_value": "4.0",
  "changes_applied": [
    "target_cpa → target_roas",
    "target_roas: 4.0"
  ],
  "dry_run": false,
  "input_mode": "NaturalLanguage",
  "raw_input": "set target ROAS to 4.0 on campaign 9876543210"
}
```

The `input_mode` and `raw_input` fields distinguish NL-driven updates from expert-mode updates in the audit trail.

### 5.3 Storage Location

```
{cache_dir}/bidding_audit.log
```

Where `{cache_dir}` is the platform-specific cache directory already defined in `mcc-gaql-common/src/paths.rs`:
- **macOS**: `~/Library/Caches/mcc-gaql/bidding_audit.log`
- **Linux**: `~/.cache/mcc-gaql/bidding_audit.log`

A new path helper will be added:

```rust
// mcc-gaql-common/src/paths.rs
pub fn bidding_audit_log_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("bidding_audit.log"))
}
```

### 5.4 Retention & Rotation

- **Max file size**: 10 MB
- **Rotation**: When file exceeds max size, rename to `bidding_audit.log.1`, `bidding_audit.log.2`, etc.
- **Max rotated files**: 5 (total ~50 MB worst case)
- **Implementation**: Check file size before each append; rotate if needed. Use `std::fs::metadata` for size check and `std::fs::rename` for rotation.
- **No automatic deletion** of rotated files — manual cleanup only.

---

## 6. Implementation Phases

### Phase 1: NL Parser + Core Mutation Engine

**Goal**: Working `update-bidding` subcommand with NL as the primary input, confirmation, and audit logging.

1. Add `bidding.rs` to `mcc-gaql-common` with data model types.
2. Add `bidding_audit_log_path()` to `mcc-gaql-common/src/paths.rs`.
3. Add `nl_parser.rs` to `mcc-gaql/src/` with:
   - Command classification
   - Entity extraction (campaign ID, strategy, values, direction)
   - Relative value resolution (fetch current, compute new)
   - Validation of resolved `BiddingStrategyUpdate`
4. Add `update_bidding.rs` to `mcc-gaql/src/` with:
   - Confirmation flow using `dialoguer`
   - Mutation via `CampaignServiceClient::mutate_campaigns()`
   - Audit log append
   - Dry-run support
5. Refactor `args.rs` to add `UpdateBidding` subcommand with NL positional arg + `--expert` flag + expert-mode flags.
6. Wire up `update-bidding` in `main.rs`.
7. Unit tests for NL parser (each supported pattern), data model, currency conversion.
8. Integration test against API (manual/CI with test account).

### Phase 2: Expert Mode & Automation Support

**Goal**: Add `--expert` flag for explicit parameter input, enabling CI/automation use.

1. Add expert-mode flag parsing and validation in `args.rs`.
2. Add expert-mode → `BiddingStrategyUpdate` construction in `update_bidding.rs`.
3. Add `--yes` non-interactive mode.
4. Tests for expert-mode validation rules.

### Phase 3: Bulk Updates (NL + Expert)

**Goal**: Update bidding strategies across multiple campaigns.

1. NL support: "lower target CPA by 10% on all PMax campaigns" or "set target ROAS to 4.0 on campaigns in campaigns.txt"
2. Expert support: `--campaign-ids-file` and `--filter` options.
3. Batch confirmation showing all campaigns with proposed changes.
4. Sequential mutations with `partial_failure: true`.
5. Summary report of successes/failures.
6. Audit log entries for each campaign.

---

## 7. File Modifications

### New Files

| File | Crate | Purpose |
|------|-------|---------|
| `crates/mcc-gaql-common/src/bidding.rs` | mcc-gaql-common | Data model: `BiddingStrategyKind`, `BiddingStrategyUpdate`, `CurrentBiddingState`, `BiddingChangePreview`, `AuditLogEntry`, `InputMode`, currency helpers |
| `crates/mcc-gaql/src/nl_parser.rs` | mcc-gaql | Natural language parser: classification, entity extraction, relative value resolution |
| `crates/mcc-gaql/src/update_bidding.rs` | mcc-gaql | Shared handler: confirmation, mutation, audit log (consumed by both NL and expert paths) |

### Modified Files

| File | Crate | Changes |
|------|-------|---------|
| `crates/mcc-gaql-common/src/lib.rs` | mcc-gaql-common | Add `pub mod bidding;` |
| `crates/mcc-gaql-common/src/paths.rs` | mcc-gaql-common | Add `bidding_audit_log_path()` |
| `crates/mcc-gaql/src/args.rs` | mcc-gaql | Add `UpdateBidding` subcommand with NL positional arg, `--expert` flag, and expert-mode flags. Existing flat args unchanged (backwards-compatible). |
| `crates/mcc-gaql/src/lib.rs` | mcc-gaql | Add `pub mod nl_parser;`, `pub mod update_bidding;` |
| `crates/mcc-gaql/src/main.rs` | mcc-gaql | Add `update-bidding` dispatch branch |
| `crates/mcc-gaql/src/googleads.rs` | mcc-gaql | Add `fetch_campaign_bidding_state()` helper and `mutate_campaign_bidding_strategy()` function. Add `CampaignServiceClient` import. |
| `crates/mcc-gaql/src/config.rs` | mcc-gaql | No changes expected (resolved config already carries all needed fields) |
| `crates/mcc-gaql/Cargo.toml` | mcc-gaql | No new dependencies needed (dialoguer, chrono, serde already present) |

---

## 8. Dependencies

### 8.1 Google Ads API Services

| Service | Method | Proto Path |
|---------|--------|------------|
| **CampaignService** | `mutate_campaigns` | `google.ads.googleads.v23.services.CampaignService` |
| **GoogleAdsService** | `search_stream` | `google.ads.googleads.v23.services.GoogleAdsService` (already used; for fetching current state) |

### 8.2 Key Generated Types (from `googleads-rs`)

| Rust Type | Import Path | Usage |
|-----------|-------------|-------|
| `CampaignServiceClient` | `googleads_rs::google::ads::googleads::v23::services::campaign_service_client` | gRPC client for mutations |
| `MutateCampaignsRequest` | `googleads_rs::google::ads::googleads::v23::services` | Mutation request wrapper |
| `CampaignOperation` | `googleads_rs::google::ads::googleads::v23::services` | Update operation with field mask |
| `Campaign` | `googleads_rs::google::ads::googleads::v23::resources` | Campaign resource with `campaign_bidding_strategy` oneof |
| `CampaignBiddingStrategy` | `googleads_rs::google::ads::googleads::v23::resources::campaign` | Oneof: `TargetCpa`, `TargetRoas`, `MaximizeConversions`, `MaximizeConversionValue`, etc. |
| `TargetCpa` | `googleads_rs::google::ads::googleads::v23::common` | `{ target_cpa_micros: Option<i64>, cpc_bid_ceiling_micros, cpc_bid_floor_micros }` |
| `TargetRoas` | `googleads_rs::google::ads::googleads::v23::common` | `{ target_roas: Option<f64>, cpc_bid_ceiling_micros, cpc_bid_floor_micros }` |
| `MaximizeConversions` | `googleads_rs::google::ads::googleads::v23::common` | `{ cpc_bid_ceiling_micros, cpc_bid_floor_micros, target_cpa_micros }` |
| `MaximizeConversionValue` | `googleads_rs::google::ads::googleads::v23::common` | `{ target_roas, cpc_bid_ceiling_micros, cpc_bid_floor_micros }` |
| `TargetSpend` | `googleads_rs::google::ads::googleads::v23::common` | `{ cpc_bid_ceiling_micros }` (used for "Maximize Clicks") |

### 8.3 Mutation Request Construction

To update a campaign's bidding strategy, the code must:

1. **Construct a fresh minimal `Campaign`** (see Section 3.4) with:
   - `resource_name`: `"customers/{customer_id}/campaigns/{campaign_id}"` (from the GAQL-fetched campaign)
   - `campaign_bidding_strategy`: Set the appropriate `CampaignBiddingStrategy` variant
   - All other fields: `..Default::default()`
2. Build a `CampaignOperation` with:
   - `operation`: `Update(campaign)` 
   - `update_mask`: `FieldMask` with the **individual variant name** as the path (e.g., `"target_roas"`, not `"campaign_bidding_strategy"`). See Appendix B for the full path table.
3. Build a `MutateCampaignsRequest` with:
   - `customer_id`: The child account ID
   - `operations`: `[operation]`
   - `validate_only`: `true` for dry-run, `false` for real
   - `partial_failure`: `false` (single campaign)
   - `response_content_type`: `MUTABLE_RESOURCE` (value `2`) to get the updated campaign back

### 8.4 Existing Infrastructure (No New Dependencies)

All required crate dependencies already exist in `Cargo.toml`:
- `dialoguer` — confirmation prompts
- `chrono` — timestamps for audit log
- `serde` / `serde_json` — audit log serialization
- `tonic` — gRPC client (channel + interceptor pattern already established)
- `googleads-rs` — generated API types including `CampaignServiceClient`

---

## Appendix A: Subcommand Structure

The `update-bidding` subcommand supports both NL and expert modes in a single subcommand:

```rust
#[derive(Subcommand)]
pub enum Command {
    // ... existing Query subcommand (backwards-compatible) ...

    /// Update bidding strategy on a campaign (natural language or expert mode)
    UpdateBidding {
        /// Natural language command (e.g., "set target CPA to 50 on campaign 1234567890")
        /// Required unless --expert is specified.
        nl_command: Option<String>,

        /// Use explicit flag-based input instead of natural language (automation/scripts)
        #[clap(long)]
        expert: bool,

        // --- Expert-mode flags (only used when --expert is set) ---

        #[clap(short, long, requires = "expert")]
        campaign_id: Option<String>,

        #[clap(long, requires = "expert")]
        strategy: Option<BiddingStrategyKind>,

        #[clap(long, requires = "expert")]
        target_cpa: Option<f64>,

        #[clap(long, requires = "expert")]
        target_cpa_micros: Option<i64>,

        #[clap(long, requires = "expert")]
        target_roas: Option<f64>,

        #[clap(long, requires = "expert")]
        cpc_bid_ceiling: Option<f64>,

        #[clap(long, requires = "expert")]
        cpc_bid_floor: Option<f64>,

        // --- Shared flags (both modes) ---

        #[clap(short, long)]
        customer_id: Option<String>,

        #[clap(long)]
        dry_run: bool,

        #[clap(short = 'y', long, requires = "expert")]
        yes: bool,

        #[clap(long)]
        no_audit: bool,
    },
}
```

### Backwards Compatibility

The existing flat CLI structure (no subcommand, positional GAQL query) is preserved. The `update-bidding` subcommand is additive:

1. Existing usage `mcc-gaql "SELECT ... FROM campaign"` continues to work unchanged.
2. New usage: `mcc-gaql update-bidding "set target CPA to 50 on campaign 1234567890"`.
3. Expert usage: `mcc-gaql update-bidding --expert --campaign-id X --strategy Y ...`.

The two-phase parse strategy from the existing codebase (try subcommand, fall back to flat args) handles this naturally.

---

## Appendix B: Field Mask Paths for Bidding Strategy Updates

When constructing the `FieldMask` for `CampaignOperation.update_mask`, the path uses the **individual oneof variant name** (snake_case, matching the protobuf field name), not the oneof group name `"campaign_bidding_strategy"`. This convention matches existing working mutation code in `gads_make_changes_via_api`.

| Scenario | Field mask `paths` | CampaignBiddingStrategy variant |
|----------|-------------------|-------------------------------|
| Switch to / update Target CPA | `["target_cpa"]` | `CampaignBiddingStrategy::TargetCpa(TargetCpa { ... })` |
| Switch to / update Target ROAS | `["target_roas"]` | `CampaignBiddingStrategy::TargetRoas(TargetRoas { ... })` |
| Switch to Maximize Conversions | `["maximize_conversions"]` | `CampaignBiddingStrategy::MaximizeConversions(MaximizeConversions { ... })` |
| Switch to Maximize Conversion Value | `["maximize_conversion_value"]` | `CampaignBiddingStrategy::MaximizeConversionValue(MaximizeConversionValue { ... })` |
| Switch to Maximize Clicks | `["target_spend"]` | `CampaignBiddingStrategy::TargetSpend(TargetSpend { ... })` |
| Switch to a portfolio strategy | `["bidding_strategy"]` | `CampaignBiddingStrategy::BiddingStrategy("customers/.../biddingStrategies/...")` |

**Why variant names instead of the oneof group name?** The protobuf field mask convention for oneof fields uses the individual variant's field tag/name, not the oneof container. The Google Ads API validates field mask paths against the proto field numbers. Using `"campaign_bidding_strategy"` may also be accepted by the API (since it is the oneof declaration name), but the variant-name convention is the proto-standard approach and is confirmed working in existing code.

**Note for implementation**: Both conventions should be tested against the live API during Phase 1. If `"campaign_bidding_strategy"` also works and is simpler (single path regardless of variant), the implementation may prefer it. Otherwise, use the variant-specific paths above.
