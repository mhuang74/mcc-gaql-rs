# Switch code-review.yml to dceoy/opencode-action with Ollama Cloud

## Context

The workflow `.github/workflows/code-review.yml` currently uses `jonit-dev/openrouter-github-action@v1.0.0` with OpenRouter and model `z-ai/glm-4.7`. The request is to switch to `dceoy/opencode-action` (v0.7.1, latest release) using `ollama-cloud` as the provider and `glm-5.2` as the model. Ollama Cloud is not a built-in OpenCode provider, so a custom provider config (`opencode.json`) is required at the repo root. The bundled `/review-pr` flow disables project config and therefore cannot use custom providers; instead, a fixed review prompt drives a normal OpenCode run that posts a PR comment.

## Approach

### 1. Create `opencode.json` at repo root

New file `opencode.json` defining the `ollama-cloud` custom provider using `@ai-sdk/openai-compatible` (Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint). The Ollama Cloud native API is at `https://ollama.com/api`; the OpenAI-compatible endpoint follows the same pattern as local Ollama (`http://localhost:11434/v1`), giving `https://ollama.com/v1` as the cloud OpenAI-compatible base URL.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "ollama-cloud": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Ollama Cloud",
      "options": {
        "baseURL": "https://ollama.com/v1",
        "apiKey": "{env:OLLAMA_API_KEY}"
      },
      "models": {
        "glm-5.2:cloud": {
          "name": "glm-5.2:cloud"
        }
      }
    }
  }
}
```

- Provider ID `ollama-cloud` → action `model` input prefix.
- Model ID `glm-5.2:cloud` — the `:cloud` tag is Ollama's convention for routing to the cloud backend (confirmed: `glm-5.2:cloud` is a published tag on `ollama.com/library/glm-5.2`).
- `apiKey` references `{env:OLLAMA_API_KEY}` — OpenCode replaces unset env refs with empty string, so the GitHub Actions secret must be configured.
- No equivalent built-in provider exists for Ollama Cloud; this custom config is required.

### 2. Replace `.github/workflows/code-review.yml` entirely

Full replacement of the workflow file. Key changes from the current workflow:

- **Action**: `dceoy/opencode-action@e9f543aabedff8b75c24daeb46c8089ccf86d68f` (v0.7.1, pinned to SHA).
- **Trigger**: Same `pull_request` types as current (`opened, synchronize, reopened, ready_for_review`).
- **Model**: `ollama-cloud/glm-5.2:cloud` (provider/model format required by the action).
- **Prompt**: Fixed `prompt` input with review instructions (replaces the old `custom_prompt`). Since `pull_request` events have no triggering comment, a fixed prompt is required.
- **Auth**: `use-github-token: true` — uses `GITHUB_TOKEN` directly, skips OIDC exchange, no `id-token: write` needed. Comments appear as `github-actions[bot]`.
- **Secret**: `OLLAMA_API_KEY` replaces `OPEN_ROUTER_KEY`. The user must set `OLLAMA_API_KEY` in repo secrets (Settings → Secrets and variables → Actions). `OPEN_ROUTER_KEY` is no longer referenced by this workflow.
- **Label filter**: Dropped. The old `review_label: 'ai-review'` (jonit-dev-specific input) is not a feature of opencode-action. All PRs matching the trigger types will be reviewed.
- **Concurrency**: Preserved unchanged.
- **Checkout**: Updated to `actions/checkout@v4` with `persist-credentials: false` (security best practice from opencode-action README; agent uses `GITHUB_TOKEN` env for API access).

New file content:

```yaml
name: AI Code Review
on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]

permissions:
  contents: read
  pull-requests: write

concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false

      - name: AI PR Review
        uses: dceoy/opencode-action@e9f543aabedff8b75c24daeb46c8089ccf86d68f  # v0.7.1
        env:
          OLLAMA_API_KEY: ${{ secrets.OLLAMA_API_KEY }}
          GITHUB_TOKEN: ${{ github.token }}
        with:
          model: ollama-cloud/glm-5.2:cloud
          use-github-token: true
          prompt: |
            You are an experienced software developer in a variety of programming languages and methodologies. You create efficient, scalable, and fault-tolerant solutions.
            Review the pull request changes and reply on how to improve the code.
            Think step-by-step.
            Give code examples of specific changes.
            Limit suggestions to 3 high quality examples.
```

## Critical files & anchors

- `opencode.json` (new, repo root) — custom `ollama-cloud` provider definition; required because Ollama Cloud is not built into OpenCode.
- `.github/workflows/code-review.yml` (full replacement) — switches action, model, secret, auth mode.
- `dceoy/opencode-action` `action.yml` (external) — confirms `model`, `prompt`, `use-github-token` inputs and their behavior.
- `dceoy/opencode-action` `docs/custom-providers.md` (external) — confirms `@ai-sdk/openai-compatible` config shape and `{env:VAR}` apiKey convention.

## Verification

1. **YAML validity**: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/code-review.yml'))"` — confirms no syntax errors.
2. **JSON validity**: `python3 -c "import json; json.load(open('opencode.json'))"` — confirms `opencode.json` parses.
3. **Secret prerequisite**: `OLLAMA_API_KEY` must exist in repo Actions secrets. Without it, OpenCode sends an empty API key and the Ollama Cloud API rejects the request.
4. **End-to-end**: Open a PR (or push to an existing PR branch) and confirm the workflow run:
   - Job starts on `pull_request` trigger.
   - OpenCode installs, loads the `ollama-cloud` provider from `opencode.json`, and runs the review prompt with `glm-5.2:cloud`.
   - A comment is posted on the PR with review suggestions (as `github-actions[bot]`).
   - If the run fails with an auth/401 error, the `OLLAMA_API_KEY` secret is missing or invalid.
   - If the run fails with a model-not-found error, verify `glm-5.2:cloud` is accessible to the Ollama Cloud account.

## Assumptions & contingencies

- **Ollama Cloud OpenAI-compatible base URL is `https://ollama.com/v1`** — inferred from the pattern: local Ollama serves native at `http://localhost:11434/api` and OpenAI-compatible at `http://localhost:11434/v1`; cloud native is documented at `https://ollama.com/api`. If the provider returns 404, try `https://ollama.com/api/v1` as the `baseURL` in `opencode.json` instead.
- **Label filter dropped** — the old `review_label: 'ai-review'` was a jonit-dev-specific feature with no opencode-action equivalent. All PRs matching the trigger types will now be reviewed. To restore label filtering, add `if: contains(github.event.pull_request.labels.*.name, 'ai-review')` to the `review` job.
- **`use-github-token: true`** chosen over the default OIDC flow for simpler setup (no `id-token: write` needed). Reviews appear as `github-actions[bot]` instead of `opencode-agent[bot]`. To use the OpenCode App identity instead, remove `use-github-token`, add `id-token: write` to permissions, and keep `GITHUB_TOKEN` in env.