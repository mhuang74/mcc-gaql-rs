# Future Phases: mcc-gaql-mut Subcommands & Enhancements

**Date:** 2026-04-22
**Status:** Planned
**Depends on:** Phase 1 — Complete Mutation Validation (see `specs/phase1_complete_mutate_validation.md`)

---

## Phase 2: Additional Subcommands

### 2.1 UpdateBidding Command

Natural language + expert mode interface for updating bidding strategies on PMax campaigns.

**New files** (in `crates/mcc-gaql-mut/src/`):
- `bidding_types.rs` — `BiddingStrategyKind`, `BiddingStrategyUpdate`, `CurrentBiddingState`, `BiddingChangePreview`, `AuditLogEntry`, `InputMode`, `currency_to_micros()`, `micros_to_currency()`, `CAMPAIGN_BIDDING_STATE_QUERY`
- `nl_parser.rs` — Rule-based NL parser: command classification, entity extraction (campaign ID, strategy, values, direction), relative value resolution, ambiguity handling
- `update_bidding.rs` — Confirmation flow, `CampaignServiceClient::mutate_campaigns()` mutation, audit log append (JSONL), dry-run support

**Modified files**:
- `args.rs` — `Command::UpdateBidding` variant with NL positional arg, `--expert` flag, expert-mode flags (`--campaign-id`, `--strategy`, `--target-cpa`, `--target-cpa-micros`, `--target-roas`, `--cpc-bid-ceiling`, `--cpc-bid-floor`), `--yes`, `--no-audit`
- `main.rs` — UpdateBidding dispatch (NL and expert paths converge at preview/confirm/apply)
- `lib.rs` — Add new modules
- `Cargo.toml` — Add `serde`, `serde_json` dependencies
- `mcc-gaql-common/src/paths.rs` — Add `bidding_audit_log_path()`

**Full spec**: `specs/pmax_bidding_strategy_updates.md`

### 2.2 Pause/Resume Commands

Quick-commands for campaign and ad group state changes.

**New files**:
- `pause_resume.rs` — `execute_pause()` / `execute_resume()` using `DynamicMutationBuilder` to set `status=PAUSED` / `status=ENABLED`; confirmation prompt; dry-run support

**Modified files**:
- `args.rs` — `Command::Pause` and `Command::Resume` variants with `resource` (PauseResource enum: Campaign, AdGroup) and `resource_name`
- `main.rs` — Pause/Resume dispatch
- `lib.rs` — Add `pub mod pause_resume;`

---

## Phase 3: Enhancements

### 3.1 Comprehensive Field Metadata Validation

Integrate `FieldMetadataCache` into validation for schema-aware error messages:
- Validate field paths against resource metadata (not just descriptor pool)
- Cross-resource field compatibility checking
- Better error messages using field descriptions from enriched metadata

### 3.2 Dry-run with Simulation

Simulate mutation results without applying:
- Show affected entity states
- Conflict detection (concurrent modifications)
- Resource-state diff display

### 3.3 Bulk Mutations

- Batch operations via `--customer-ids-file` or `--from-query`
- Parallel execution with rate limiting
- Progress reporting
- Summary of successes/failures

---

## Phase 4: Operational

### 4.1 Audit Logging for Generic Mutate

JSONL audit log for all mutations (not just bidding):
- Timestamp, user, customer, resource, operation, field changes
- Append to `{cache_dir}/mutation_audit.log`

### 4.2 --from-query Flag

Execute GAQL query, use results as input for mutations:
- `--from-query "SELECT campaign.resource_name FROM campaign WHERE ..."` paired with `--set` for batch updates
