# Polars JsonWriter vs serde_json Investigation

**Date**: 2025-10-22
**Author**: Investigation into using Polars built-in JSON writer

## Executive Summary

**Recommendation: Do NOT upgrade to use Polars JsonWriter at this time.**

The current custom implementation using `serde_json` should be maintained. While Polars JsonWriter would simplify the code, the upgrade costs (compilation bugs in 0.42, breaking API changes in 0.51+) significantly outweigh the benefits.

---

## Current State

### Polars Version
- **Current version**: 0.42.0
- **Features enabled**: `["lazy", "serde-lazy"]`

### Current JSON Implementation
- **Location**: src/main.rs:493-575
- **Functions**: `write_json()` and `write_json_to_stdout()`
- **Implementation**: Custom implementation using `serde_json`
- **Approach**:
  - Iterates through DataFrame rows and columns
  - Converts each value to appropriate JSON type (integer, float, null, string)
  - Builds JSON array of objects manually
  - ~80 lines per function

---

## Investigation Findings

### Version 0.42.0 (Current)

**Status**: JSON feature is BROKEN

**Issues**:
- The `json` feature exists but has a critical compilation bug when used with `lazy` feature
- Compilation error: `error[E0412]: cannot find type 'CloudOptions' in this scope`
- Occurs in: `polars-plan-0.42.0/src/plans/functions/count.rs:189:28`
- Related GitHub issue: [#18416](https://github.com/pola-rs/polars/issues/18416)

**Workaround**:
- Adding `parquet` feature resolves the compilation error
- However, this adds unnecessary dependencies for cloud storage features

**Conclusion**: Not viable for production use

### Version 0.51.0 (Latest Stable)

**Status**: JSON feature works, but requires extensive refactoring

**What Works**:
- The `json` feature compiles successfully
- JsonWriter is available and functional
- Provides two output formats:
  - `JsonFormat::Json` - Standard JSON array of objects
  - `JsonFormat::JsonLines` - Newline-delimited JSON (NDJSON)

**Breaking API Changes Required**:

1. **Series::new() signature change**
   - Old (0.42): `Series::new(&str, values)`
   - New (0.51): `Series::new(PlSmallStr, values)`
   - Fix: Add `.into()` to all string literals: `Series::new("name".into(), values)`
   - Affected locations: src/googleads.rs:270, 276, 280

2. **DataFrame::new() signature change**
   - Old (0.42): `DataFrame::new(Vec<Series>)`
   - New (0.51): `DataFrame::new(Vec<Column>)`
   - Requires converting Series to Column types
   - Affected locations: src/googleads.rs:285

**Code Impact**:
- Multiple changes across src/googleads.rs
- Potential changes in other files using Polars API
- Comprehensive testing required after upgrade

**Benefits if Upgraded**:
- Simpler JSON output code (~80 lines → ~6 lines per function)
- Example simplified code:
  ```rust
  fn write_json_to_stdout(df: &mut DataFrame) -> Result<()> {
      let mut buf = Vec::new();
      JsonWriter::new(&mut buf)
          .with_json_format(JsonFormat::Json)
          .finish(df)?;
      print!("{}", String::from_utf8(buf)?);
      Ok(())
  }
  ```

---

## Current Implementation Analysis

### Strengths
1. **Reliable**: Works correctly with current Polars 0.42.0
2. **Type-safe**: Proper handling of different data types (integers, floats, nulls, strings)
3. **No extra dependencies**: Uses only `serde_json` which is already a dependency
4. **Readable**: Logic is clear and easy to understand
5. **Maintainable**: Self-contained implementation

### Code Quality
The current implementation (src/main.rs:493-575):
- Properly converts DataFrame values to appropriate JSON types
- Handles edge cases (null values, string quoting)
- Distinguishes between integers and floats
- Creates clean JSON output compatible with LLM tools

---

## Recommendations

### Short Term (Current)
**Keep the existing `serde_json` implementation**

Reasons:
1. Version 0.42.0 JSON feature is broken
2. Version 0.51.0 requires too much refactoring for minimal benefit
3. Current implementation is solid and working well
4. No urgent need for code simplification

### Medium Term (6-12 months)
**Re-evaluate when Polars 1.0 stable is released**

Actions:
1. Monitor Polars release notes for API stabilization
2. Wait for Polars 1.0 with more stable, mature APIs
3. Consider upgrade during a major refactoring effort
4. Ensure comprehensive test coverage before attempting upgrade

### Long Term
**Consider upgrade to latest Polars during major codebase refactoring**

Conditions for upgrade:
1. Polars API has stabilized (1.0+ releases)
2. Planning broader codebase refactoring anyway
3. Can dedicate time for comprehensive testing
4. Benefits extend beyond just JSON writing (e.g., performance, new features)

---

## Cost-Benefit Analysis

### Benefits of Upgrading
- Cleaner code (~150 lines reduced across both JSON functions)
- Native Polars integration
- Potential performance improvements (though likely negligible for typical use)

### Costs of Upgrading
- Breaking changes across multiple files
- Risk of introducing bugs during migration
- Comprehensive testing required
- Time investment for refactoring
- Potential for new issues with Polars API changes

### Verdict
**Costs significantly outweigh benefits.** The code simplification is nice but not essential, while the upgrade risks and effort are substantial.

---

## Technical Details

### Polars JsonWriter API (v0.51+)
```rust
use polars::prelude::*;

// Write to stdout
let mut buf = Vec::new();
JsonWriter::new(&mut buf)
    .with_json_format(JsonFormat::Json)
    .finish(&mut df)?;
print!("{}", String::from_utf8(buf)?);

// Write to file
let f = File::create(outfile)?;
JsonWriter::new(f)
    .with_json_format(JsonFormat::Json)
    .finish(&mut df)?;
```

### Required Cargo.toml Change
```toml
# Current
polars = { version = "0.42", features = ["lazy", "serde-lazy"] }

# For JsonWriter (0.51+)
polars = { version = "0.51", features = ["lazy", "serde-lazy", "json"] }
```

---

## References

- [Polars GitHub Issue #18416](https://github.com/pola-rs/polars/issues/18416) - JSON + lazy feature compilation bug
- [Polars GitHub Issue #19643](https://github.com/pola-rs/polars/issues/19643) - PlSmallStr conversion changes
- [Polars JsonWriter Documentation](https://docs.rs/polars/latest/polars/prelude/struct.JsonWriter.html)
- Current implementation: src/main.rs:493-575

---

## Conclusion

The investigation confirms that while Polars JsonWriter is a cleaner solution in theory, the practical barriers make it unsuitable for adoption at this time. The current `serde_json` implementation is well-written, reliable, and should be maintained until Polars APIs stabilize and a natural refactoring opportunity arises.
