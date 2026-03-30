# Extract Domain Knowledge into `resources/domain_knowledge.md`

## Context

The LLM prompts in `rag.rs` contain ~150 lines of domain knowledge (resource routing rules, metric terminology, conversion rules, date handling) hardcoded as inline Rust string literals. This knowledge is duplicated between the two Phase 3 prompt variants (with-cookbook and without-cookbook). Extracting it to a Markdown file makes it easier to maintain, review, and customize without recompiling. Additionally, we need to add new best practices around WHERE/SELECT field alignment, ratio metric decomposition, and metric variant completeness.

## Files to Modify

1. **`resources/domain_knowledge.md`** — NEW file with all domain knowledge
2. **`crates/mcc-gaql-gen/src/rag.rs`** — Add `DomainKnowledge` struct, load it in `MultiStepRAGAgent::init()`, inject sections into Phase 1 and Phase 3 prompts
3. **`crates/mcc-gaql-gen/src/bundle.rs`** — Add `domain_knowledge.md` to bundle create/install
4. **`crates/mcc-gaql-common/src/paths.rs`** — Add `domain_knowledge_path()` helper (optional, for consistency)

## Step 1: Create `resources/domain_knowledge.md`

Structured with `## Section` headers so sections can be parsed and injected into different prompts:

- **`## Resource Selection Guidance`** — Extract from Phase 1 prompt (rag.rs:1984-2000). Resource routing rules for asset extensions, Smart campaigns, location_view, impression share, etc.
- **`## Metric Terminology`** — Extract from Phase 3 prompt (rag.rs:2789-2821 / 2928-2960). Volume/Financial/Ratio metric categories, engagement disambiguation, default performance fields.
- **`## Numeric and Monetary Conversion`** — Extract K/M/B suffix rules and micros conversion (rag.rs:2823-2833 / 2962-2972).
- **`## Monetary Value Extraction`** — Extract threshold validation rules (rag.rs:2885-2892).
- **`## Date Range Handling`** — Extract DURING vs BETWEEN rules and literal mappings (rag.rs:2834-2865 / 2973-2999). NOTE: Computed date *examples* using runtime variables (`{today}`, `{this_year_start}`) stay inline in rag.rs.
- **`## Query Best Practices`** — NEW content:
  - WHERE fields should also appear in SELECT for transparency/verifiability
  - Ratio metrics must include component metrics (CTR→clicks+impressions, CPC→cost+clicks, ROAS→revenue+cost, CPA→cost+conversions, impression share→impressions)
  - Include all metric variants when category has multiple (e.g., impression share: both regular and top_impression sets)

## Step 2: Add `DomainKnowledge` struct to `rag.rs`

```rust
struct DomainKnowledge {
    sections: HashMap<String, String>,
}
```

- `DomainKnowledge::load()` — reads from `config_file_path("domain_knowledge.md")`, falls back to empty if missing
- `DomainKnowledge::parse(content)` — splits on `## ` headers into named sections
- `DomainKnowledge::section(name) -> &str` — returns section content or `""`

Add `domain_knowledge: DomainKnowledge` field to `MultiStepRAGAgent`, load in `init()`.

## Step 3: Modify Phase 1 prompt (~line 1971)

Replace inline "Resource selection guidance:" block (lines 1984-2000) with:
```rust
let resource_guidance = self.domain_knowledge.section("Resource Selection Guidance");
// Inject as {resource_guidance} in the format! string
```

Also inject the new Query Best Practices section here.

## Step 4: Modify Phase 3 prompts (~lines 2761 and 2901)

Both with-cookbook and without-cookbook variants get the same treatment. Replace inline domain blocks with injected sections:

| Inline block | Replaced by |
|---|---|
| Metric Terminology (lines 2789-2821) | `self.domain_knowledge.section("Metric Terminology")` |
| Numeric/Monetary (lines 2823-2833) | `self.domain_knowledge.section("Numeric and Monetary Conversion")` |
| Monetary Extraction (lines 2885-2892) | `self.domain_knowledge.section("Monetary Value Extraction")` |
| Date Range Rules (lines 2834-2865) | `self.domain_knowledge.section("Date Range Handling")` |
| NEW: Query Best Practices | `self.domain_knowledge.section("Query Best Practices")` |

Keep inline:
- JSON response format instructions
- Computed date examples with runtime variables (`{today}`, `{this_year_start}`, etc.)
- IN/NOT IN operator rules (structural, not domain knowledge)
- Mandatory date filter rule
- MCC/identity field inclusion rules (these reference `{resource}` placeholder — keep inline)
- LIMIT/ORDER BY rules

## Step 5: Add to bundle system (`bundle.rs`)

- In `create_bundle()`: copy `domain_knowledge.md` into the tar.gz bundle
- In `install_bundle()`: copy from bundle to `config_dir.join("domain_knowledge.md")`
- Both with graceful handling if file is missing (warn, skip)

## Step 6: Add path helper (`paths.rs`)

Add `domain_knowledge_path() -> Result<PathBuf>` for consistency with existing helpers.

## Verification

1. `cargo check --workspace` — ensure it compiles
2. `cargo test -p mcc-gaql-gen --lib -- --test-threads=1` — run existing tests
3. Manual test: copy `resources/domain_knowledge.md` to `~/Library/Application Support/mcc-gaql/domain_knowledge.md`, run `mcc-gaql-gen generate` with `--verbose` to verify domain knowledge sections appear in LLM prompts
4. Compare generated GAQL for queries involving ratio metrics (e.g., "campaigns with CTR > 5%") to verify component metrics are now included

## Design Notes

- **Graceful fallback**: If file is missing, prompts work without domain hints — acceptable for transition
- **No duplication**: Extracting shared content from two near-identical Phase 3 prompts reduces maintenance burden
- **Bundle compatibility**: Old CLIs ignore unknown files; new CLIs handle missing files gracefully
