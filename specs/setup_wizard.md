# Setup Wizard Specification

**Status**: In Progress (Improvements Planned)
**Created**: 2025-10-28
**Updated**: 2025-10-28

## Overview

The setup wizard (`--setup` flag) provides an interactive configuration experience for first-time users of mcc-gaql. It guides users through creating a configuration profile without requiring manual TOML editing.

## Purpose

- Simplify initial setup for new users
- Generate valid configuration profiles automatically
- Reduce errors from manual config file editing
- Provide sensible defaults for optional settings

## User Flow

When users run `mcc-gaql --setup`, the wizard:

1. Determines profile name automatically
2. Prompts for required configuration fields
3. Prompts for optional configuration fields
4. Generates token cache filename automatically
5. Creates/updates `~/.config/mcc-gaql/config.toml`
6. Provides next steps guidance

## Configuration Fields

### Current Implementation

The wizard prompts for the following fields:

#### 1. Profile Name (Auto-generated)
- **Type**: String
- **Prompt**: None (auto-generated)
- **Validation**: Ensures uniqueness by appending `_N` suffix
- **Logic**:
  - Tries "myprofile" first
  - If exists, tries "myprofile_2", "myprofile_3", etc.
  - Maximum 100 attempts
- **Example**: `myprofile`, `myprofile_2`

#### 2. MCC Customer ID (Required)
- **Type**: String
- **Prompt**: "Enter your MCC Account ID (digits only, no dashes)"
- **Validation**: Must contain only digits
- **Error**: "Invalid input. Please enter digits only (no dashes or other characters)"
- **Example**: `1234567890`
- **Notes**: The ID should be the Google Ads MCC (Manager) account ID

#### 3. Token Cache Filename (Auto-generated)
- **Type**: Optional String
- **Prompt**: Shows default, allows Enter to accept or custom input
- **Default**: `tokencache_{profile}_{date}.json`
- **Example**: `tokencache_myprofile_20251028.json`
- **Location**: Same directory as config file
- **Current Issue**: ⚠️ May be unused if user field is set (runtime auto-generates from email)

#### 4. Customer IDs Filename (Optional)
- **Type**: Optional String
- **Prompt**: "Enter customer IDs filename (or press Enter for default)"
- **Default**: `customerids.txt`
- **Validation**: None (accepts any string)
- **Purpose**: File containing list of customer account IDs to query

#### 5. Queries Filename (Optional)
- **Type**: Optional String
- **Prompt**: "Enter queries filename (or press Enter for default)"
- **Default**: `query_cookbook.toml`
- **Validation**: None (accepts any string)
- **Purpose**: TOML file containing saved GAQL queries

### Missing Fields

#### 🚨 CRITICAL: User Email (Not Prompted)
- **Current Behavior**: Always set to `None`
- **Impact**:
  - Profiles created by wizard cannot authenticate properly
  - Users must manually edit config file to add user field
  - Breaks the "easy setup" promise
- **Required For**: OAuth2 authentication flow
- **Planned Fix**: Add user email prompt with validation

## Output Format

The wizard generates a TOML configuration file at `~/.config/mcc-gaql/config.toml` with the following structure:

```toml
[myprofile]
mcc_customerid = "1234567890"
token_cache_filename = "tokencache_myprofile_20251028.json"
customerids_filename = "customerids.txt"
queries_filename = "query_cookbook.toml"
```

### Optional Field Handling

Fields that are `None` are omitted from the TOML file:
- If user doesn't provide customerids_filename → field not written
- If user doesn't provide queries_filename → field not written
- If user doesn't provide token_cache_filename → field not written

## Implementation Details

### File Location
- **Config File**: `~/.config/mcc-gaql/config.toml`
- **Platform-specific**: Uses `dirs::config_dir()` for cross-platform support
- **Directory Creation**: Automatically creates `~/.config/mcc-gaql/` if missing

### Profile Management

The wizard appends new profiles to existing config files:
1. Reads existing config.toml if present
2. Parses as TOML document
3. Adds new profile as table entry
4. Writes back with pretty formatting

### Serialization Method

Currently uses **manual TOML construction**:
```rust
let mut profile_table = Map::new();
profile_table.insert("mcc_customerid".to_string(), Value::String(...));
// ... manual field insertion
```

⚠️ **Technical Debt**: Should use serde automatic serialization instead

## Validation Rules

### MCC Customer ID
- Must contain only digits (0-9)
- No dashes or other characters allowed
- Example valid: `1234567890`
- Example invalid: `123-456-7890`

### Profile Name
- Must be unique within config file
- Auto-generated to avoid conflicts
- Limited to 100 generation attempts

### Filenames
- Currently no validation
- Any string accepted
- No check for file existence
- No path validation

## User Experience Flow

```
$ mcc-gaql --setup

Welcome to mcc-gaql setup!

This wizard will help you configure mcc-gaql for first-time use.

Enter your MCC Account ID (digits only, no dashes): 1234567890
✓ MCC Customer ID set

Token cache filename [tokencache_myprofile_20251028.json]: ⏎
✓ Using token cache filename: tokencache_myprofile_20251028.json

Enter customer IDs filename (or press Enter for default) [customerids.txt]: ⏎
✓ Using customer IDs filename: customerids.txt

Enter queries filename (or press Enter for default) [query_cookbook.toml]: ⏎
✓ Using queries filename: query_cookbook.toml

Configuration saved to: /Users/username/.config/mcc-gaql/config.toml
Profile name: myprofile

Next steps:
1. Run 'mcc-gaql --list-child-accounts' to see your linked accounts
2. Create 'customerids.txt' with account IDs (one per line)
3. Run queries with 'mcc-gaql -q "SELECT ..."'

For more information, visit: https://github.com/your-repo/mcc-gaql
```

## Known Issues

### Issue 1: Missing User Email Field
**Severity**: HIGH
**Impact**: Profiles cannot authenticate without manual editing

**Current Behavior**:
```rust
let config = MyConfig {
    mcc_customerid,
    user: None,  // ❌ Always None
    // ...
};
```

**Required Behavior**:
```rust
let config = MyConfig {
    mcc_customerid,
    user: Some(user_email),  // ✅ Prompted from user
    // ...
};
```

### Issue 2: Token Cache Filename Inconsistency
**Severity**: MEDIUM
**Impact**: Generated filename may never be used

**Problem**:
- Wizard generates: `tokencache_{profile}_{date}.json`
- Runtime generates: `tokencache_{sanitized_email}.json` (when user field is set)
- If user field is added later, wizard's token_cache_filename is ignored

**Solution**: Remove token_cache_filename generation from wizard

### Issue 3: No File Path Validation
**Severity**: LOW
**Impact**: Users may specify invalid or inaccessible paths

**Problem**:
- No check if customerids_filename exists or is accessible
- No check if queries_filename exists or is accessible
- Could lead to confusing runtime errors

**Solution**: Add validation or offer to create files

### Issue 4: Manual TOML Construction
**Severity**: LOW
**Impact**: Maintenance burden and potential for divergence

**Problem**:
- Wizard doesn't use serde automatic serialization
- Manual field-by-field copying is error-prone
- If MyConfig fields are added, wizard must be manually updated

**Solution**: Use `toml::to_string_pretty(&config)` for serialization

## Planned Improvements

### High Priority

#### Add User Email Prompt
```rust
let user_email: String = Input::new()
    .with_prompt("Enter your email for OAuth2 authentication")
    .validate_with(|input: &String| -> Result<(), &str> {
        if input.trim().is_empty() {
            return Err("Email is required for authentication");
        }
        if !input.contains('@') {
            return Err("Invalid email format");
        }
        Ok(())
    })
    .interact_text()?;
```

#### Remove Token Cache Auto-generation
- Let runtime generate token cache filename from user email
- Simpler and more consistent
- Aligns with current best practice (see oauth_token_caching_without_config_file.md)

### Medium Priority

#### Switch to Serde Serialization
Replace manual TOML construction with:
```rust
let toml_string = toml::to_string_pretty(&config)?;
```

#### Add File Path Validation
- Check if customerids_filename exists → offer to create if missing
- Check if queries_filename exists → offer to use default cookbook
- Provide helpful error messages with full paths

### Low Priority

#### Better UX Messaging
- Explain what each field is used for
- Provide examples in prompts
- Link to documentation for more details
- Show full paths for generated files

#### Integration Tests
- Test wizard-generated configs can be loaded
- Test profile uniqueness logic
- Test TOML round-trip serialization

## Testing Strategy

### Manual Testing
1. Run wizard with no existing config → creates myprofile
2. Run wizard again → creates myprofile_2
3. Verify config.toml is valid TOML
4. Load config with figment → verify all fields present
5. Test authentication flow with generated profile

### Automated Testing
1. Unit test profile name generation logic
2. Unit test MCC customer ID validation
3. Integration test: wizard → config file → load → verify
4. Test handling of existing config files
5. Test optional field omission in TOML

## Related Specifications
- `no_default_profile.md` - Config-free mode and profile selection
- `oauth_token_caching_without_config_file.md` - Token cache behavior
- `configuration_system.md` - Full config field documentation (to be created)

## Dependencies

### Crates
- `dialoguer` - Interactive prompts and validation
- `toml` - TOML serialization/deserialization
- `serde` - Serialization framework
- `dirs` - Cross-platform config directory
- `anyhow` - Error handling

### Code
- `src/setup.rs` - Wizard implementation
- `src/config.rs` - Config structures and loading
- `src/args.rs` - CLI argument parsing

## Future Considerations

### Potential Enhancements
- Add `--setup-profile <name>` to specify profile name
- Add `--setup-non-interactive` with environment variables
- Support updating existing profiles (not just creating new ones)
- Add wizard step to test authentication immediately
- Generate example customerids.txt and query_cookbook.toml files
- Support multiple config file locations (project-local configs)

### Migration Path
For existing users with profiles missing the user field:
1. Detect missing user field on first run
2. Prompt user to run `--setup` or manually edit config
3. Provide helpful error message with exact fix needed

## Conclusion

The setup wizard is a critical onboarding tool but currently has gaps that prevent it from creating fully functional profiles. The highest priority fix is adding the user email prompt to enable complete OAuth2 authentication flow.
