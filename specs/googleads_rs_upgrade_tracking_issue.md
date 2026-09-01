# googleads-rs upgrade workflow — tracking issue + error-comment logging (implementation plan)

**Status: IMPLEMENTED 2026-09-01** on branch `add_issue_to_upgrade_workflow` (PR: created after implementation; workflow YAML verified with PyYAML + per-step `bash -n` + stub-run of the new bash blocks against a mock `gh`). Approved planning artifact: `local://upgrade-issue-tracking-plan.md`. This document is the in-repo record of that plan; anchors were re-verified on-disk immediately before editing.

## Context

The auto-upgrade workflow (`.github/workflows/googleads-rs-upgrade.yml`) captures validation failures into `/tmp/first-validation-failures.txt` and `/tmp/ai-repair-attempt-<N>.log` — files that vanish with the runner. When an upgrade is detected, create a GitHub Issue to track it; log validation/repair errors as issue comments for human review; the upgrade PR auto-closes the issue on merge. End state: every detected upgrade has one issue thread containing the run link, was/now table, per-attempt repair errors, the PR link, and auto-close-on-merge.

## Approach

All workflow edits are in `.github/workflows/googleads-rs-upgrade.yml` (530 lines on disk; anchors verified 2026-09-01 against `auto_upgrade_googleads_rs` working tree, clean at HEAD). New step ids: `tracking_issue`, `report_first_fail`. Comment bodies are written to `/tmp/*.md` files and passed via `--body-file` (avoids shell-quoting bugs). Issue-API calls use the default `GITHUB_TOKEN` (issue events trigger no workflows in this repo; `UPGRADE_PAT` stays PR-only). Each new/edited `gh issue` step sets `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` step-locally (existing convention, cf. detect step line 107).

### 1. Permissions + header docs

- `permissions:` block (lines 59–61): add `issues: write` with comment `# create/comment tracking issue`.
- `Retry contract:` header comment (lines 37–45): add one bullet after the Failure bullet:

```
#   Tracking issue → an issue titled `Upgrade googleads-rs to <ver>
#     (automated)` is created when an upgrade starts; validation/repair
#     errors are posted as comments on it; the upgrade PR contains
#     `Closes #<issue>` so merging closes it; failed runs leave the issue
#     open for human review.
```

### 2. New step `Create tracking issue` (id: `tracking_issue`)

Insert between Detect (outputs end at line 204) and Migrate (line 206). `if: steps.detect.outputs.changed == 'true'`. env: `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`, `LATEST_VER/LATEST_SHA/CUR_VER/CUR_REV/MAJOR_BUMP` from `steps.detect.outputs.*`, `RUN_URL: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}`. Bash (`set -euo pipefail`):

```bash
TITLE="Upgrade googleads-rs to ${LATEST_VER} (automated)"

# bot-controlled titles → exact-phrase title search is a reliable dedupe;
# version in the title prevents matching older upgrades' issues
EXISTING="$(gh issue list --state open --search "\"${TITLE}\" in:title" --json number --jq '.[0].number // empty')"

if [ -z "$EXISTING" ]; then
  cat > /tmp/issue-body.md <<ISSUE_EOF
Automated googleads-rs upgrade detected ([workflow run](${RUN_URL})).

| | rev | crate |
|---|---|---|
| was | \`${CUR_REV:0:9}\` | ${CUR_VER} |
| now | \`${LATEST_SHA:0:9}\` | ${LATEST_VER} |

Upstream commit: https://github.com/mhuang74/googleads-rs/commit/${LATEST_SHA}
ISSUE_EOF
  if [ "$MAJOR_BUMP" == "true" ]; then
    echo "Release notes: https://developers.google.com/google-ads/api/docs/release-notes/v${LATEST_VER%%.*}" >> /tmp/issue-body.md
  fi
  cat >> /tmp/issue-body.md <<ISSUE_EOF

Validation failures and AI-repair progress will be posted as comments on this issue. The upgrade PR references this issue with \`Closes #N\` — merging the PR closes it. If a run fails, the branch stays pushed for inspection and the next scheduled run deletes it and retries (comments continue here).
ISSUE_EOF
  ISSUE_URL="$(gh issue create --title "$TITLE" --body-file /tmp/issue-body.md)"
  ISSUE_NUM="${ISSUE_URL##*/}"
else
  gh issue comment "$EXISTING" --body "New upgrade run for the same target (${LATEST_VER}): ${RUN_URL}. Previous attempt failed without a PR; stale branch deleted, retrying fresh."
  ISSUE_URL="https://github.com/${GITHUB_REPOSITORY}/issues/${EXISTING}"
  ISSUE_NUM="$EXISTING"
fi
{
  echo "number=${ISSUE_NUM}"
  echo "url=${ISSUE_URL}"
} >> "$GITHUB_OUTPUT"
```

Notes: heredoc terminator lands at column 0 after YAML strips the run-block indent (same mechanics as the existing migration heredoc, lines 333–383). Unquoted heredoc ⇒ `${}` interpolates; backticks MUST be escaped (`\``). No `continue-on-error` — a broken token/permission must surface (weekly cron retries). This dedupe/comment path only triggers when detect deleted a stale branch (previous run failed without PR), which is the correct context for the "retrying fresh" message.

### 3. New step `Report first-validation failures` (id: `report_first_fail`)

Insert immediately after the First validation step (ends line 262), i.e. before `Open PR (validation passed)` at line 264 — NOT before Install pi; `open_pr` sits between them in the file, and the mutually exclusive `if` conditions make order functionally neutral. `if: steps.first_validate.outputs.failed == 'true'`. env: `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`, `ISSUE: ${{ steps.tracking_issue.outputs.number }}`, `RUN_URL` as above. Bash:

```bash
{
  echo "## First validation failed"
  echo ""
  echo "Run: ${RUN_URL}"
  echo ""
  echo '```'
  sed -n '1,300p' /tmp/first-validation-failures.txt
  echo '```'
  if [ "$(wc -l < /tmp/first-validation-failures.txt)" -gt 300 ]; then
    echo ""
    echo "…(truncated — full log in run ${RUN_URL})"
  fi
  echo ""
  echo "AI repair starting (≤5 attempts); per-attempt results will be commented here."
} > /tmp/fail-comment.md
gh issue comment "$ISSUE" --body-file /tmp/fail-comment.md
```

Truncation check is `wc -l` (exact for the 300-line cap; a byte-count check cannot know the printed portion's size).

### 4. AI repair step edits (lines 306–469)

- env (lines 309–321): add `ISSUE: ${{ steps.tracking_issue.outputs.number }}` **and** `RUN_URL: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}` (comment text references RUN_URL).
- **Per-attempt failure** — after the `echo "Repair attempt ${ATTEMPT}: validation still failing" | tee -a "${ATTEMPT_LOG}"` line (461), before the partial commit (463):
  ```bash
  {
    echo "Repair attempt ${ATTEMPT}/${PI_MAX_ATTEMPTS} failed — run ${RUN_URL}"
    echo ""
    echo '```'
    sed -n '1,300p' /tmp/attempt-failures.txt
    echo '```'
    echo ""
    echo "pi agent log tail:"
    echo '```'
    tail -n 40 "/tmp/ai-repair-attempt-${ATTEMPT}.log"
    echo '```'
    echo ""
    echo "Partial fixes pushed to branch \`${BRANCH}\`."
  } > /tmp/attempt-comment.md
  gh issue comment "$ISSUE" --body-file /tmp/attempt-comment.md || echo "::warning::issue comment failed"
  ```
  (`|| echo ::warning` — must not kill the loop.)
- **Success path** — after `REPAIR_SUCCEEDED=true` (line 457), before `break` (line 458): `gh issue comment "$ISSUE" --body "Validation passed after repair attempt ${ATTEMPT}; opening PR." || true`.
- **Exhaustion** — replace the one-liner at line 469 with a block; comment goes before `exit 1`:
  ```bash
  [ "$REPAIR_SUCCEEDED" == "true" ] || {
    gh issue comment "$ISSUE" --body "AI repair failed after ${PI_MAX_ATTEMPTS} attempts. Branch \`${BRANCH}\` pushed for inspection; next run deletes it and retries fresh. Review the attempt comments above and the pushed branch." || echo "::warning::issue comment failed"
    echo "::error::AI repair failed after ${PI_MAX_ATTEMPTS} attempts; branch pushed for inspection"
    exit 1
  }
  ```

### 5. Both PR steps (`open_pr` lines 264–300, `post_repair_pr` lines 471–508)

- env: add `ISSUE: ${{ steps.tracking_issue.outputs.number }}`.
- Capture the PR URL: wrap `gh pr create` in `PR_URL="$( ... )"`. The heredoc closes inside a command substitution, so the final line `)"` becomes `)" )"` (first `)`+`"` close `$(cat`+body quote; second pair closes the outer `PR_URL="$(gh`).
- Add body line before the heredoc's `EOF`: a blank line, then `Closes #${ISSUE}` (heredoc is unquoted, so it interpolates) — this is the auto-close mechanism; no other config needed since base is the default branch.
- After creation (exact token handling — the step-level `GH_TOKEN` is `UPGRADE_PAT`, which may be a fine-grained PAT with only Contents+Pull-requests RW per the header comment at lines 28–30; that PAT would 403 on issues, so override inline with the default token):
  ```bash
  GH_TOKEN="${{ secrets.GITHUB_TOKEN }}" gh issue comment "$ISSUE" --body "Upgrade PR opened: ${PR_URL} — merging auto-closes this issue." || true
  echo "pr_url=${PR_URL}" >> "$GITHUB_OUTPUT"
  ```

### 6. Final summary step (lines 510–531)

- env additions: `ISSUE: ${{ steps.tracking_issue.outputs.number }}`, `RUN_URL: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}`, `PR_URL: ${{ steps.open_pr.outputs.pr_url || steps.post_repair_pr.outputs.pr_url }}`, and `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` (step has none today; the new comment needs it).
- After the existing summary write, append:
  ```bash
  if [ -n "$ISSUE" ]; then
    gh issue comment "$ISSUE" --body "Outcome: ${OUTCOME} Run: ${RUN_URL}" || true
  fi
  ```
  (No-op runs never create an issue, so the guard holds. Backticks inside `OUTCOME`'s value are safe: the double-quoted string is parsed before expansion, and expansion results are not re-parsed.)

### 7. Spec doc update — `specs/googleads_rs_upgrade_workflow.md`

- Line 30: replace the "No issue/label queue (…the pinned rev in Cargo.lock IS the serialized state, one workflow run at a time)" sentence with: the Cargo.lock pin remains the serialized state, and — post-merge — each detected upgrade now also opens a tracking issue whose comments hold the error trail (runner /tmp logs are ephemeral).
- "Retry contract" section (lines 106–110): add one bullet: issue lifecycle — created at detect when `changed=true` (reused via open-issue title search on retries for the same target version), validation/repair errors commented, PR carries `Closes #N`, issue closed by merge or manually by the human; a failed upgrade leaves the issue open for review.

## Critical files & anchors

- `.github/workflows/googleads-rs-upgrade.yml` — the only behavior file. Anchors (verified; verify again before editing — edits renumber): header comment 37–45; permissions 59–61; detect outputs end 204; migrate starts 206; first validation ends 262; `open_pr` 264–300 (env 267–275, `gh pr create` 286–300); install pi 302–304; ai_repair 306–469 (env ends 321, success break 457–458, fail branch 461–465, exhaustion 469); `post_repair_pr` 471–508; final summary 510–531.
- `specs/googleads_rs_upgrade_workflow.md` — as-built record; line 30 (no-issue claim) and 106–110 (retry contract). **Untracked in git** (`??`) as of 2026-09-01 — committing it is part of this change.

## Git handling

Stage only the two files explicitly — working tree holds unrelated untracked dotfiles (`.bashrc`, `.idea/`, etc.) and `reports/googleads_rs_upgrade_workflow_impl_summary.md` (out of scope; leave untracked):

```bash
git add .github/workflows/googleads-rs-upgrade.yml specs/googleads_rs_upgrade_workflow.md
```

## Verification

1. YAML sanity: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/googleads-rs-upgrade.yml'))"` → OK.
2. End-to-end (after the implementation PR merges to main; pre-merge alternative: `gh workflow run googleads-rs-upgrade.yml --ref <impl-branch>`, accepting that the resulting upgrade PR will also carry the workflow change): `gh workflow run googleads-rs-upgrade.yml` (no `target_rev` → upstream HEAD → real v24→v25 run), then `gh run watch` / `gh run view`. Expected observables:
   - A new open issue exists: `gh issue list --state open --search "\"Upgrade googleads-rs to 25.1.0\" in:title"` → 1 hit; body has run link + was/now table.
   - If validation fails → issue comments contain the "First validation failed" fenced cargo output and one comment per repair attempt; on exhaustion the outcome comment appears and the run is red.
   - If validation passes (expected per the local smoke test: check green with zero source changes) → PR opened; `gh pr view <N> --json body` contains `Closes #<issue-number>`; issue has the PR-link comment.
   - Re-dispatch while that PR is open → run summary "PR … open and awaiting review", no new issue (title-search dedupe).
3. Merge-close semantics are GitHub-native (`Closes #N` in PR body closes the issue on merge into main); confirm post-merge with `gh issue view <N> --json state` → `"closed"`. Not verifiable before a human merges.

## Assumptions & contingencies

- No new secrets/vars: issue APIs use the default token. If `gh issue create` ever 403s (policy change), the run fails fast and visibly — fix is a repo-setting change, and cron retries.
- Orphan-issue edge: a failed v25 upgrade leaves its issue open; if upstream later ships v26, the new run creates a fresh v26 issue and the stale v25 issue stays open for a human to close. Accepted (bot titles make staleness obvious); no auto-close logic.
- Comment size: bodies capped at 300 lines / fenced (`sed -n '1,300p'`), well under GitHub's 65536-char comment limit; full logs always reachable via the run link in each comment.
- If `gh issue list --search` phrase-matching misbehaves (empty result despite an existing open issue), the failure mode is a duplicate issue, not data loss — acceptable; no fallback needed.
- First-run reality per spec: v24→v25 major; workspace compiled green locally on v25, so the likely path is green validation → PR → issue closes on merge; the error-comment path gets exercised only if serial tests or parse-protos fail.