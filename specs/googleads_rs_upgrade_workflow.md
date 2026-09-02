# googleads-rs auto-upgrade workflow — implementation plan

Merged to `main` via PR [#70](https://github.com/mhuang74/mcc-gaql-rs/pull/70) (merge commit `7459d8f`); keep/revert decision pending (revert: `git revert -m 1 7459d8f`). This document is the as-built record: design, deviations from the approved plan, verification evidence, and activation prerequisites.

## Context

mcc-gaql-rs consumes `googleads-rs` as a git dependency pinned by `rev` in three Cargo.tomls. This workflow mirrors googleads-rs's own automated Google-Ads-API upgrade pipeline (its `CICD.md` + `.github/workflows/google-ads-upgrade.yml`): detect new commits on `mhuang74/googleads-rs` `main`, bump this repo's pin, and gate on `mcc-gaql-gen parse-protos` proving the new protos are parsable.

Facts at design time (verified 2026-09-01):

- Pins: `crates/mcc-gaql/Cargo.toml:26`, `crates/mcc-gaql-common/Cargo.toml:22`, `crates/mcc-gaql-mut/Cargo.toml:20` → `rev = "3d36a5a840a7fa7c473bbed92a99c5d10b712dd9"` (= tag v24.2.0).
- googleads-rs `main` HEAD = `81c005f028bb0e1857435e3948d96a23af5472cb` (2026-09-01), crate version `25.1.0` → first real run is a **major** v24→v25 upgrade.
- Version single source: `crates/mcc-gaql-common/src/version.rs` — `GOOGLEADS_API_VERSION: &str = "v24"` (line 3), `RAG_BUNDLE_KEY: &str = "mcc-gaql-rag-bundle-v24.tar.gz"` (line 7), guard test `rag_bundle_key_contains_api_version` enforces they agree.
- parse-protos: subcommand of mcc-gaql-gen (`mcc-gaql-gen parse-protos [--force] [--output <path>]`); locates protos via `GOOGLEADS_PROTO_DIR` env or cargo git checkout scan for `proto/google/ads/googleads/{GOOGLEADS_API_VERSION}` (`crates/mcc-gaql-gen/src/proto_locator.rs:16-37`). Building mcc-gaql-gen requires compile-time `MCC_GAQL_R2_PUBLIC_ID` (`crates/mcc-gaql-gen/src/r2.rs:13` `env!`) — already a repo Actions variable used by rust.yml/release.yml.
- googleads-rs reference for the AI-repair pattern: `google-ads-upgrade.yml:297-440` — pi CLI (`@earendil-works/pi-coding-agent`), flags `--provider/--model/--system-prompt/--api-key/--print/--no-session/--approve`, `PI_MAX_ATTEMPTS=5`, `PI_API_KEY` secret step-scoped, `PI_MODEL`/`PI_PROVIDER` repo variables with defaults `glm-5.3-flash`/`ollama-cloud`.
- This repo's CI conventions: `.github/workflows/rust.yml` (Swatinem/rust-cache@v2 shared-key `mcc-gaql-rs-gen`, ort cache purge, apt `protobuf-compiler`, `--profile ci`, `--test-threads=1`), `.github/workflows/code-review.yml` (AI PR review on PR events).

## Evaluation: GitHub Agentic Workflows vs plain Actions (decided)

`gh aw` (github/gh-aw, technical preview Feb 2026, public preview ~Jun 2026) compiles markdown+frontmatter into `.lock.yml` and runs a Copilot/Pi/Claude/Codex agent with safe-outputs. Fit analysis:

- ~95% of this pipeline is deterministic (ls-remote + grep pins + `cargo update` + cargo build/test): plain Actions is the exact tool; `gh aw` would put these inside an LLM sandbox, losing determinism and Actions-UI debuggability.
- `gh aw` costs: new toolchain (gh extension + `gh aw compile` committing `.lock.yml` files), engine auth setup, preview-stage churn (versions ≥0.83.3,<0.85.4 already retired for a CVE; engines removed across majors).
- The only agentic step — repairing compile breakage after a major bump — is already solved in the sibling repo with a bounded pi loop. Reuse it verbatim inside plain Actions.

Decision: **single plain-Actions workflow with a pi repair step**. `gh aw` rejected; do not re-litigate.

## Approach

One file, `.github/workflows/googleads-rs-upgrade.yml` (531 lines), committed on branch `auto_upgrade_googleads_rs` (commit `9c78f1e`) and merged to `main` (PR #70). No other repo files changed. The Cargo.lock pin remains the serialized state (one workflow run at a time), and — post-merge of the tracking-issue change — each detected upgrade also opens a tracking issue whose comments hold the error trail (runner /tmp logs are ephemeral).

### Triggers and guards

```yaml
name: googleads-rs-upgrade
on:
  schedule:
    - cron: '0 7 * * 4'   # Thu 07:00 UTC, after googleads-rs's Thu detect(04:30)+worker(05:00) cycle
  workflow_dispatch:
    inputs:
      target_rev: { description: 'Optional googleads-rs commit sha to pin (default: main HEAD)', required: false, default: '' }
permissions:
  contents: write        # push/delete upgrade branch
  pull-requests: write   # open PR
concurrency:
  group: googleads-rs-upgrade
  cancel-in-progress: false
timeout: job-level `timeout-minutes: 45`
env: CARGO_TERM_COLOR: always
```

Single job `upgrade` on `ubuntu-latest`, steps in this order:

1. **Checkout** — `actions/checkout@v4`, `fetch-depth: 0` (history helps pi repair context).
2. **Install deps** — `sudo apt-get -y install protobuf-compiler` (same as rust.yml).
3. **Rust toolchain + cache** — `dtolnay/rust-toolchain@stable`; `Swatinem/rust-cache@v2` with `shared-key: "mcc-gaql-rs-gen"`, `cache-on-failure: true`, same `cache-directories:` list as rust.yml build-gen; then `rm -rf ~/.cache/ort.pyke.io` (rust.yml convention).
4. **Detect — id: detect** (bash, `set -euo pipefail`; jq/curl preinstalled):
   - Current pin from `Cargo.lock` (authoritative resolved state): `grep -A2 '^name = "googleads-rs"$'` block → `version` + `rev=<40-hex>` from the `source` line.
   - Target: manual `target_rev` input (short SHAs normalized to 40-hex via `gh api repos/.../commits/<sha>`) or `git ls-remote https://github.com/mhuang74/googleads-rs.git refs/heads/main`.
   - Crate version at target commit fetched from `raw.githubusercontent.com` (the effective "release" — googleads-rs bumps it per Google API release). Curl calls carry an `Authorization: Bearer $UPGRADE_PAT` header when the secret exists, so the pipeline survives googleads-rs going private.
   - Skip guards (each writes a `$GITHUB_STEP_SUMMARY` row and exits with `changed=false`): pin == target; docs-only drift (compare API `pin..target` shows only `CICD.md|README.md|CHANGELOG.md|specs/|reports/|.github/|.gitignore|tests/.*\.md|docs/` paths AND crate version unchanged; unknown file list → treated as material).
   - Outputs: `changed`, `latest_ver`, `latest_sha`, `cur_ver`, `cur_rev`, `major_bump` (`CUR_VER%%.*` != `LATEST_VER%%.*`), `branch` = `bot/googleads-rs-<LATEST_VER>`.
   - Stale-branch policy: after `git fetch origin "$BRANCH"` succeeds — if `gh pr list --head "$BRANCH" --state open` is NON-empty → step summary "PR open, awaiting review", `changed=false`, exit; if no open PR → `git push origin --delete "$BRANCH"` (self-healing retry; attempt logs live in Actions run history).
   - Step summary table: current rev/ver, target rev/ver, major bump, branch name.
5. **Migrate — id: migrate** (`if: steps.detect.outputs.changed == 'true'`); branch from main:
   ```bash
   git checkout -B "$BRANCH"
   git config user.name "mcc-gaql-upgrade-bot"; git config user.email "mhuang74@users.noreply.github.com"
   for f in crates/mcc-gaql/Cargo.toml crates/mcc-gaql-common/Cargo.toml crates/mcc-gaql-mut/Cargo.toml; do
     sed -i -E "s/rev = \"[0-9a-f]{40}\"/rev = \"${LATEST_SHA}\"/" "$f"
     grep -q "rev = \"${LATEST_SHA}\"" "$f" || { echo "::error::pin update failed in $f"; exit 1; }
   done
   cargo update -p googleads-rs
   grep -q "rev=${LATEST_SHA}" Cargo.lock || { echo "::error::Cargo.lock not updated"; exit 1; }
   # major bump only:
   sed -i -E 's/pub const GOOGLEADS_API_VERSION: &str = "v[0-9]+";/.../' crates/mcc-gaql-common/src/version.rs
   sed -i -E 's/mcc-gaql-rag-bundle-v[0-9]+\.tar\.gz/.../' crates/mcc-gaql-common/src/version.rs
   git add Cargo.lock <3 Cargo.tomls> crates/mcc-gaql-common/src/version.rs
   git commit -m "Feature: Upgrade to googleads-rs version ${LATEST_VER} (rev ${LATEST_SHA:0:9})"
   ```
   Commit WITHOUT push here; push happens once after validation so a failing run never lands an unvalidated branch. (`git checkout -B` handles the recreated-branch case.)
6. **First validation — id: first_validate** (`if: changed == 'true'`); failures appended to `/tmp/first-validation-failures.txt` inside `::group::` wrappers:
   ```bash
   run cargo check --profile ci --workspace
   run cargo test --profile ci --workspace -- --test-threads=1   # serial: repo convention (race conditions)
   run cargo run --profile ci -p mcc-gaql-gen -- parse-protos --force --output /tmp/proto_docs.json
   ```
   `MCC_GAQL_R2_PUBLIC_ID` from repo vars. parse-protos run last: needs workspace+gen compile anyway; its implicit `GOOGLEADS_API_VERSION`-keyed proto lookup doubles as the version-consistency check (if version.rs says v25 but pin still v24, locator fails).
7. **PR on success — id: open_pr** (`if: steps.first_validate.outputs.failed == 'false'`):
   `env: GH_TOKEN: ${{ secrets.UPGRADE_PAT }}` — PAT not GITHUB_TOKEN, so PR events trigger rust.yml/code-review.yml required checks (GITHUB_TOKEN PRs never fire `pull_request` workflows).
   Push branch, `gh pr create` titled "Feature: Upgrade to googleads-rs version <ver>" with was/now rev+crate table, workflow-run link, upstream commit link; release-notes URL `https://developers.google.com/google-ads/api/docs/release-notes/vNN` only when `major_bump == 'true'`.
8. **Install pi** (`if: steps.first_validate.outputs.failed == 'true'`): `npm install -g @earendil-works/pi-coding-agent`.
9. **AI repair (pi) — id: ai_repair** (`if: steps.first_validate.outputs.failed == 'true'`), env:
   ```yaml
   GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}      # step-local; used only for git push
   PI_API_KEY: ${{ secrets.PI_API_KEY }}      # step-local; never job-level
   PI_PROVIDER: ${{ vars.PI_PROVIDER || 'ollama-cloud' }}
   PI_MODEL: ${{ vars.PI_MODEL || 'glm-5.3-flash' }}
   PI_MAX_ATTEMPTS: '5'
   ```
   Assembles `/tmp/mcc-gaql-migration.md` (heredoc): old/new versions+shas, major-bump y/n (+ release-notes URL when true), repo layout (4 crates; `current_gads_version` alias is the versioned-module entrypoint — use it, never raw vNN paths; `version.rs` pre-updated by the workflow — do NOT re-edit), the 3 validation commands, hard rules (no test deletion/weakening/`#[ignore]`; no clippy disables/`#[allow]`; do NOT edit `.github/**`, `specs/**`, rev pins, `Cargo.lock`, `version.rs`; allowed: `crates/*/src` + `crates/*/tests`).
   Loop (adapted verbatim from googleads-rs upgrade job 352-437): pi `--provider/--model/--system-prompt/--api-key/--print/--no-session/--approve` per attempt into `/tmp/ai-repair-attempt-<N>.log`; `git diff HEAD` appended; re-validate the same 3 commands; on pass: commit `fix: repair googleads-rs <ver> upgrade failures (attempt N)` + push + `repair_succeeded=true` + break; on fail: commit partial `|| true` + push `|| true`. System prompt duplicates the hard rules + upgrade failure patterns (renamed/removed fields → update match arms/callers; new required fields → defaults in test constructors; enum variant changes → match arms; `current_gads_version` alias repoint check). Failure after 5/5: `exit 1` (branch remains pushed for inspection; next run's detect deletes it via the stale policy since no PR exists).
10. **Post-repair PR** (`if: first_validate.failed == 'true' && ai_repair.outputs.repair_succeeded == 'true'`): identical PR step; body adds "Validation passed after AI repair (5-attempt budget)."
11. **Final summary** (`if: always()`): outcome line in `$GITHUB_STEP_SUMMARY` — no-op / PR opened / PR after repair / FAILED (branch pushed for inspection; next run deletes it).

### Retry contract (documented in workflow header)

- Success → PR on main; human merges. An open PR for the same branch blocks further upgrades until merged/closed (pruned at detect).
- Failure → branch pushed WITHOUT PR (for inspection); next weekly run (or manual dispatch) auto-deletes the stale branch and retries fresh from main with a full 5-attempt budget.
- Tracking issue → created at detect when `changed=true` (reused via open-issue title search on retries for the same target version); validation/repair errors are posted as comments; the PR carries `Closes #N` so merging closes the issue, or a human closes it manually; a failed upgrade leaves the issue open for review.
- Optional overrides: manual dispatch `target_rev=<sha>` re-runs against a chosen commit; repo vars `PI_MODEL`/`PI_PROVIDER` retune the repair model without workflow edits.

## Deviations from the original plan (as built)

1. **Stale-branch check moved inside detect** (plan sketched it as a separate step-4 block): one step, one `set -euo pipefail`, `gh pr list --head "$BRANCH"` consumed via `GH_TOKEN` (default token) — only branch deletion needs `contents: write`, which the job-level `permissions` already grants.
2. **PAT-auth'd curl in detect from day one** (plan §209 contingency applied up front): `CURL_AUTH` array injected into raw.githubusercontent + compare-API calls when `UPGRADE_PAT` is set, so the pipeline survives googleads-rs going private.
3. **Short-sha normalization for `target_rev`** added: sub-40-hex input resolved to full SHA via `gh api repos/.../commits/<sha>` before use (defensive; `gh pr create`-style callers may pass short SHAs).
4. **Migration commit carries only the 5 pinned files** (`git add` enumerated paths): the checked-out workflow ref can contain unrelated dirty state; explicit add keeps the bot commit minimal. (Local working tree's untracked dotfiles made this non-hypothetical.)
5. **Repair-loop validation is functionally identical but structurally self-contained** (`runv` helper writing `/tmp/attempt-failures.txt`) instead of re-invoking the first-validation step: inside one bash step, per-attempt logs must be isolated.
6. **`git checkout -B` instead of `checkout -b`** for branch creation: tolerates a locally-resurrected branch name after a partial previous run.

## Verification (2026-09-01)

1. **YAML sanity**: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/googleads-rs-upgrade.yml'))"` → OK (no actionlint on host).
2. **Local scratch migration smoke test** (plan §189; detached worktree at `07d4ff7` since `auto_upgrade_googleads_rs` was checked out in the main worktree):
   - seds applied cleanly: 3× rev pin `3d36a5a8…` → `81c005f0…`, `GOOGLEADS_API_VERSION`/`RAG_BUNDLE_KEY` v24→v25.
   - `cargo update -p googleads-rs` → **googleads-rs 24.2.0 → 25.1.0** @ `81c005f0` (real major bump).
   - `cargo check --profile ci --workspace` → **exit 0, zero source changes**, 16.3 min fresh build (private target dir; shared warm `target/` was root-owned and unwritable — see side findings).
3. **Not locally verified — host disk hit 0 bytes free:** serial `cargo test` + `parse-protos` against v25. Both run verbatim in the workflow's First validation step; the first real run proves them. (Triage freed ~34G of regenerable caches — uv 32.9G, pip 1.2G, Homebrew 0.4G — which background processes re-consumed; no unsanctioned system tuning performed.)
4. **Registration + end-to-end:** workflow registered `active` on GitHub; **zero runs executed** (checked `gh run list --workflow=googleads-rs-upgrade.yml` → empty). End-to-end proof lands with the first run (scheduled Thu 2026-09-03 07:00 UTC, or manual dispatch).
5. **PR #70 checks:** `Build Core` 2m37s ✓, `Detect Changes` ✓, `review` ✓; gen build correctly skipped (paths-filter; no gen paths touched).

## Assumptions, contingencies, prerequisites

- Secrets `UPGRADE_PAT` (classic PAT with repo scope or fine-grained Contents+PR RW) and `PI_API_KEY` exist or user adds them; `gh secret list`/`gh secret set` fail under the current token (HTTP 403) → admin must set them (web UI or `gh secret set`). Without `UPGRADE_PAT`, a run passes validation then fails at the PR step; without `PI_API_KEY`, only the repair path fails. Detection/migration/validation run regardless.
- Vars: `MCC_GAQL_R2_PUBLIC_ID` present (rust.yml depends on it); `PI_MODEL`/`PI_PROVIDER` optional with inline defaults `glm-5.3-flash`/`ollama-cloud`.
- Cron `Thu 07:00 UTC` follows googleads-rs's Thu pipeline; later commits caught next week; manual dispatch covers urgency.
- googleads-rs `proto/` retains previous majors (update.sh stages target tree additively) → same-major pin bumps keep the `vNN` dir; version.rs catches up only on major bumps. Verified implicitly by the local check (workspace compiled green against v25 protos via the new pin).
- Known first-run reality: **v24→v25 major bump**, workspace compiles green with zero source changes → AI-repair path likely idle; parse-protos against v25 protos is the remaining unknown.
