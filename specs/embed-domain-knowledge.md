# Spec: Embed Domain Knowledge in Binary

## Overview

Embed `resources/domain_knowledge.md` into the `mcc-gaql-gen` binary at compile time using `include_str!`, ensuring domain knowledge is always available without requiring manual deployment.

## Motivation

### Problem
The `DomainKnowledge::load()` function looks for `~/.config/mcc-gaql/domain_knowledge.md`, but:
1. No code deploys this file from the repo
2. The bundle system only works if someone already has the file
3. Without it, LLM resource selection is non-deterministic (same query succeeded March 31st, failed April 1st)

### Evidence
```
[2026-04-01 12:40:11] domain_knowledge.md not found at "/home/mhuang/.config/mcc-gaql/domain_knowledge.md", using empty sections
[2026-04-01 12:40:16] Phase 1 complete: campaign (5052ms)  <-- WRONG, should be location_view
```

### Solution
Compile `resources/domain_knowledge.md` into the binary as a fallback default, while still allowing user overrides via the config file.

---

## Current State

### File Locations
- **Source:** `resources/domain_knowledge.md` (in repo, not used at runtime)
- **Expected:** `~/.config/mcc-gaql/domain_knowledge.md` (doesn't exist)

### Current Loading Logic (`rag.rs:1467-1493`)
```rust
impl DomainKnowledge {
    fn load() -> Self {
        let path = match config_file_path("domain_knowledge.md") {
            Some(p) if p.exists() => p,
            Some(p) => {
                log::debug!("domain_knowledge.md not found...");
                return Self { sections: HashMap::new() };  // Empty fallback
            }
            None => { ... }
        };
        // Read from path if exists
    }
}
```

### Problem with Current Logic
- Returns empty `HashMap` when file missing
- `section()` returns empty string for all lookups
- LLM prompts receive no domain guidance

---

## Proposed Changes

### 1. Add Default Constant

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Add at module level (near top of file, after imports):

```rust
/// Embedded domain knowledge compiled from resources/domain_knowledge.md.
/// This serves as the default when no user override exists in the config directory.
const DEFAULT_DOMAIN_KNOWLEDGE: &str = include_str!("../../../resources/domain_knowledge.md");
```

**Path explanation:**
- `src/rag.rs` → `../` → `src/` → `../` → `crates/mcc-gaql-gen/` → `../` → `crates/` → `../` → repo root
- Then `resources/domain_knowledge.md`

### 2. Update `DomainKnowledge::load()`

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Replace the current `load()` implementation:

```rust
impl DomainKnowledge {
    /// Load domain knowledge with fallback to embedded defaults.
    /// 
    /// Priority:
    /// 1. User file at ~/.config/mcc-gaql/domain_knowledge.md (if exists)
    /// 2. Embedded default compiled into binary
    fn load() -> Self {
        // Try user override first
        if let Some(path) = mcc_gaql_common::paths::config_file_path("domain_knowledge.md") {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        log::info!("Loaded user domain_knowledge.md from {:?}", path);
                        return Self::parse(&content);
                    }
                    Err(e) => {
                        log::warn!("Failed to read user domain_knowledge.md: {}, using embedded default", e);
                    }
                }
            }
        }
        
        // Fall back to embedded default (always available)
        log::debug!("Using embedded domain_knowledge.md");
        Self::parse(DEFAULT_DOMAIN_KNOWLEDGE)
    }
}
```

### 3. Add Build Script Tracking

**File:** `crates/mcc-gaql-gen/build.rs`

Add rerun-if-changed directive to track the source file:

```rust
fn main() {
    // Existing build.rs content...
    
    // Track domain knowledge file for incremental builds
    println!("cargo:rerun-if-changed=../../resources/domain_knowledge.md");
}
```

This ensures the crate recompiles when `domain_knowledge.md` changes.

### 4. Move Domain Knowledge File (if needed)

Verify the file exists at the correct path relative to `include_str!`:

```
mcc-gaql/
├── crates/
│   └── mcc-gaql-gen/
│       └── src/
│           └── rag.rs          # include_str!("../../../resources/domain_knowledge.md")
└── resources/
    └── domain_knowledge.md     # Source file
```

If the file is currently elsewhere, move it:
```bash
mkdir -p resources
mv <current_location>/domain_knowledge.md resources/
```

---

## Implementation Steps

### Step 1: Verify File Location
```bash
ls -la resources/domain_knowledge.md
```

### Step 2: Update rag.rs

1. Add the `DEFAULT_DOMAIN_KNOWLEDGE` constant after imports
2. Replace the `DomainKnowledge::load()` method

### Step 3: Update build.rs

Add the `cargo:rerun-if-changed` directive.

### Step 4: Build and Verify

```bash
# Build should succeed
cargo build -p mcc-gaql-gen

# Verify embedded content
cargo run -p mcc-gaql-gen -- --name locations_with_highest_revenue_per_conversion --explain 2>&1 | grep -i "domain_knowledge"
```

Expected log output:
```
Using embedded domain_knowledge.md
```

### Step 5: Test User Override

```bash
# Copy to config dir
cp resources/domain_knowledge.md ~/.config/mcc-gaql/domain_knowledge.md

# Run again
cargo run -p mcc-gaql-gen -- --name locations_with_highest_revenue_per_conversion --explain 2>&1 | grep -i "domain_knowledge"
```

Expected log output:
```
Loaded user domain_knowledge.md from "/home/user/.config/mcc-gaql/domain_knowledge.md"
```

---

## Testing Strategy

### Unit Test: Embedded Content Available

```rust
#[test]
fn test_default_domain_knowledge_not_empty() {
    assert!(!DEFAULT_DOMAIN_KNOWLEDGE.is_empty());
    assert!(DEFAULT_DOMAIN_KNOWLEDGE.contains("## Resource Selection Guidance"));
}
```

### Unit Test: Parse Embedded Content

```rust
#[test]
fn test_parse_embedded_domain_knowledge() {
    let dk = DomainKnowledge::parse(DEFAULT_DOMAIN_KNOWLEDGE);
    let guidance = dk.section("Resource Selection Guidance");
    assert!(!guidance.is_empty());
    assert!(guidance.contains("location_view"));
}
```

### Integration Test: Fallback Works

```rust
#[test]
fn test_load_uses_embedded_when_no_user_file() {
    // Ensure no user file exists (or use temp dir)
    let dk = DomainKnowledge::load();
    let guidance = dk.section("Resource Selection Guidance");
    assert!(guidance.contains("location_view"));
}
```

### Manual Test: Query Generation

```bash
# Remove user file to test embedded
rm -f ~/.config/mcc-gaql/domain_knowledge.md

# Run the previously failing query
cargo run -p mcc-gaql-gen -- --name locations_with_highest_revenue_per_conversion --explain

# Should now select location_view (not campaign)
```

---

## Rollback Plan

If issues arise:

1. Revert rag.rs changes
2. Remove build.rs directive
3. Rebuild

The change is low-risk since:
- Only adds a fallback (doesn't remove existing functionality)
- User override still works exactly as before
- No changes to external interfaces

---

## Future Considerations

### Bundle System Update

The bundle system in `bundle.rs` currently copies from config dir. Consider:
- Bundling the embedded version instead
- Or keeping current behavior (bundles user customizations)

No changes required for this spec; current bundle behavior is acceptable.

### Version Tracking

Consider adding a version comment to domain_knowledge.md:
```markdown
<!-- Version: 2026-04-02 -->
# GAQL Domain Knowledge
...
```

This helps users know if their override is outdated.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/mcc-gaql-gen/src/rag.rs` | Add constant, update `load()` |
| `crates/mcc-gaql-gen/build.rs` | Add rerun-if-changed |
| `resources/domain_knowledge.md` | Verify exists (no content change) |

---

## Success Criteria

1. `cargo build -p mcc-gaql-gen` succeeds
2. Running without user config file logs "Using embedded domain_knowledge.md"
3. Running with user config file logs "Loaded user domain_knowledge.md"
4. `locations_with_highest_revenue_per_conversion` query selects `location_view`
5. All existing tests pass
