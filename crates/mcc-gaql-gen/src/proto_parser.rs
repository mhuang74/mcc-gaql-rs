//! Proto file parser for extracting field documentation.
//!
//! This module parses proto files from googleads-rs to extract documentation
//! comments for fields, messages, and enums.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

/// Field behavior annotations (google.api.field_behavior)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldBehavior {
    Immutable,
    OutputOnly,
    Required,
    Optional,
}

/// Parsed documentation for a single proto field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoFieldDoc {
    pub field_name: String,
    pub field_number: u32,
    pub description: String,
    pub field_behavior: Vec<FieldBehavior>,
    pub type_name: String,
    pub is_enum: bool,
    pub enum_type: Option<String>,
}

/// Parsed documentation for a single proto message (resource).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoMessageDoc {
    pub message_name: String,
    pub description: String,
    pub fields: Vec<ProtoFieldDoc>,
}

/// Parsed documentation for a single enum value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumValueDoc {
    pub name: String,
    pub number: i32,
    pub description: String,
}

/// Parsed documentation for a proto enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoEnumDoc {
    pub enum_name: String,
    pub description: String,
    pub values: Vec<EnumValueDoc>,
}

/// Main parser for proto files.
pub struct ProtoParser {
    // Regex patterns for parsing
    message_pattern: Regex,
    field_pattern: Regex,
    enum_pattern: Regex,
    enum_value_pattern: Regex,
    field_behavior_pattern: Regex,
}

/// Find the line index for a given byte position in file content.
/// Returns the index of the line containing the position.
fn find_line_index(content: &str, pos: usize) -> usize {
    // Count newlines before the position to get the line index
    content[..pos.min(content.len())].matches('\n').count()
}

/// Find the line index for a given byte position using a pre-split lines array.
/// This avoids re-collecting lines when we already have them.
fn find_line_index_from_lines(lines: &[&str], pos: usize) -> usize {
    let mut current_pos = 0;

    for (idx, line) in lines.iter().enumerate() {
        // +1 accounts for newline character(s)
        let line_end = current_pos + line.len() + 1;
        if current_pos <= pos && pos < line_end {
            return idx;
        }
        current_pos = line_end;
    }

    lines.len().saturating_sub(1)
}

/// Extract comment lines preceding a given line index.
/// Returns the concatenated comment text.
fn extract_preceding_comment_lines(lines: &[&str], line_idx: usize) -> String {
    let mut comments = Vec::new();

    for i in (0..line_idx).rev() {
        let line = lines[i].trim();

        // Stop at first non-comment line
        if !line.starts_with("//") {
            break;
        }

        // Remove leading // and whitespace
        let comment = line.strip_prefix("//").unwrap_or(line).trim();
        if !comment.is_empty() {
            comments.push(comment.to_string());
        }
    }

    // Reverse to get correct order
    comments.reverse();
    comments.join(" ")
}

impl ProtoParser {
    pub fn new() -> Self {
        Self {
            // Match message definitions: message MessageName {
            message_pattern: Regex::new(r"(?m)^message\s+(\w+)\s*\{").unwrap(),
            // Match field definitions: type name = number;
            // Captures: type, name, number
            field_pattern: Regex::new(
                r#"(?m)^\s*((?:\w+\.)*\w+)\s+(\w+)\s*=\s*(\d+)(?:\s*\[([^\]]*)\])?;"#,
            )
            .unwrap(),
            // Match enum definitions: enum EnumName {
            enum_pattern: Regex::new(r"(?m)^\s*enum\s+(\w+)\s*\{").unwrap(),
            // Match enum values: NAME = number;
            // Optionally preceded by // comment
            enum_value_pattern: Regex::new(r"(?m)^\s*(\w+)\s*=\s*(-?\d+)\s*;").unwrap(),
            // Match field behavior: (google.api.field_behavior) = BEHAVIOR
            field_behavior_pattern: Regex::new(
                r#"(?m)\(google\.api\.field_behavior\)\s*=\s*(\w+)"#,
            )
            .unwrap(),
        }
    }

    /// Parse a proto file and extract message documentation.
    pub fn parse_proto_file(&self, content: &str) -> Vec<ProtoMessageDoc> {
        let mut messages = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Find all message definitions
        for caps in self.message_pattern.captures_iter(content) {
            let message_name = caps.get(1).unwrap().as_str().to_string();

            // Find message start position
            let msg_start = caps.get(0).unwrap().start();

            // Extract message-level comment (lines before the message)
            let description = self.extract_preceding_comment(&lines, msg_start);

            // Extract fields within the message
            let fields = self.extract_message_fields(content, msg_start, &message_name);

            messages.push(ProtoMessageDoc {
                message_name,
                description,
                fields,
            });
        }

        messages
    }

    /// Parse an enum proto file and extract enum documentation.
    pub fn parse_enum_file(&self, content: &str) -> Vec<ProtoEnumDoc> {
        let mut enums = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Find all nested enum definitions (message EnumMessage { enum EnumName { ... } })
        for caps in self.enum_pattern.captures_iter(content) {
            let enum_name = caps.get(1).unwrap().as_str().to_string();
            let enum_start = caps.get(0).unwrap().start();

            // Get the enclosing message name
            let container_name = self
                .find_container_message(content, enum_start)
                .unwrap_or_else(|| format!("{}Enum", enum_name));

            // Extract enum-level comment
            let description = self.extract_preceding_comment(&lines, enum_start);

            // Extract enum values
            let values = self.extract_enum_values(content, enum_start);

            enums.push(ProtoEnumDoc {
                enum_name: format!("{}Enum.{}", container_name, enum_name),
                description,
                values,
            });
        }

        enums
    }

    /// Extract the name of the containing message for an enum.
    fn find_container_message(&self, content: &str, pos: usize) -> Option<String> {
        // Look backwards from pos to find the message definition
        let before = &content[..pos];

        // Find the last message before this enum
        for caps in self.message_pattern.captures_iter(before) {
            return Some(caps.get(1).unwrap().as_str().to_string());
        }

        None
    }

    /// Extract comment lines preceding a definition.
    fn extract_preceding_comment(&self, lines: &[&str], pos: usize) -> String {
        let line_idx = find_line_index_from_lines(lines, pos);
        extract_preceding_comment_lines(lines, line_idx)
    }

    /// Remove nested message definitions from content.
    /// Returns content with nested message bodies replaced by whitespace,
    /// preserving byte positions for parent-level field extraction.
    fn remove_nested_messages(&self, content: &str) -> String {
        let mut chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        let mut brace_depth = 0;

        while i < chars.len() {
            match chars[i] {
                '{' => {
                    brace_depth += 1;
                    i += 1;
                }
                '}' => {
                    brace_depth -= 1;
                    i += 1;
                }
                _ => {
                    // Look for "message" keyword at depth > 0 (nested message)
                    if brace_depth > 0
                        && chars.get(i..i + 7) == Some(&['m', 'e', 's', 's', 'a', 'g', 'e'])
                    {
                        // Check if it's a word boundary (start of "message" keyword)
                        let is_word_start =
                            i == 0 || chars.get(i - 1).map_or(true, |c| !c.is_alphanumeric());

                        if is_word_start {
                            // Find the opening brace after message name
                            let mut j = i + 7;
                            // Skip whitespace and get message name
                            while j < chars.len() && chars[j].is_whitespace() {
                                j += 1;
                            }
                            // Skip message name
                            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_')
                            {
                                j += 1;
                            }
                            // Skip whitespace
                            while j < chars.len() && chars[j].is_whitespace() {
                                j += 1;
                            }

                            // Check if we found an opening brace
                            if j < chars.len() && chars[j] == '{' {
                                // Found a nested message, find its matching closing brace
                                let nested_start = j;
                                let mut nested_brace_depth = 0;
                                let mut k = j;

                                while k < chars.len() {
                                    match chars[k] {
                                        '{' => nested_brace_depth += 1,
                                        '}' => {
                                            nested_brace_depth -= 1;
                                            if nested_brace_depth == 0 {
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                    k += 1;
                                }

                                // Replace content from opening brace to closing brace with whitespace
                                // This preserves byte positions for parent fields
                                for idx in nested_start..=k.min(chars.len() - 1) {
                                    if chars[idx] != '\n' {
                                        chars[idx] = ' ';
                                    }
                                }

                                i = k + 1;
                                continue;
                            }
                        }
                    }
                    i += 1;
                }
            }
        }

        chars.into_iter().collect()
    }

    /// Extract fields within a message block.
    fn extract_message_fields(
        &self,
        content: &str,
        msg_start: usize,
        _message_name: &str,
    ) -> Vec<ProtoFieldDoc> {
        // Find message end (matching brace)
        let mut brace_count = 0;
        let mut msg_end = msg_start;

        let after_start = &content[msg_start..];
        for (i, c) in after_start.char_indices() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        msg_end = msg_start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        let message_block = &content[msg_start..msg_end];

        // Remove nested message definitions to avoid extracting their fields
        // This preserves byte positions so field comment extraction still works
        let filtered_block = self.remove_nested_messages(message_block);

        let mut fields = Vec::new();

        for caps in self.field_pattern.captures_iter(&filtered_block) {
            let type_name = caps.get(1).unwrap().as_str().to_string();
            let field_name = caps.get(2).unwrap().as_str().to_string();
            let field_number: u32 = caps.get(3).unwrap().as_str().parse().unwrap_or(0);
            let field_opts = caps.get(4).map(|m| m.as_str()).unwrap_or("");

            // Extract field behavior
            let field_behavior = self.extract_field_behavior(field_opts);

            // Determine if this is an enum type
            let is_enum = type_name.contains("Enum") || type_name.contains("Status");
            let enum_type = if is_enum {
                Some(type_name.clone())
            } else {
                None
            };

            // Get field comment
            let field_pos = msg_start + caps.get(0).unwrap().start();
            let description = self.extract_field_comment(content, field_pos);

            fields.push(ProtoFieldDoc {
                field_name,
                field_number,
                description,
                field_behavior,
                type_name,
                is_enum,
                enum_type,
            });
        }

        fields
    }

    /// Extract field behavior annotations.
    fn extract_field_behavior(&self, field_opts: &str) -> Vec<FieldBehavior> {
        let mut behaviors = Vec::new();

        for caps in self.field_behavior_pattern.captures_iter(field_opts) {
            let behavior = caps.get(1).unwrap().as_str();
            match behavior {
                "IMMUTABLE" => behaviors.push(FieldBehavior::Immutable),
                "OUTPUT_ONLY" => behaviors.push(FieldBehavior::OutputOnly),
                "REQUIRED" => behaviors.push(FieldBehavior::Required),
                "OPTIONAL" => behaviors.push(FieldBehavior::Optional),
                _ => {}
            }
        }

        behaviors
    }

    /// Extract comment for a specific field.
    fn extract_field_comment(&self, content: &str, field_pos: usize) -> String {
        let line_idx = find_line_index(content, field_pos);
        let lines: Vec<&str> = content.lines().collect();
        extract_preceding_comment_lines(&lines, line_idx)
    }

    /// Extract enum values from an enum block.
    fn extract_enum_values(&self, content: &str, enum_start: usize) -> Vec<EnumValueDoc> {
        // Find enum block end
        let mut brace_count = 0;
        let mut enum_end = enum_start;

        let after_start = &content[enum_start..];
        for (i, c) in after_start.char_indices() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        enum_end = enum_start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        let enum_block = &content[enum_start..enum_end];
        let block_offset = enum_start;

        let mut values = Vec::new();

        for caps in self.enum_value_pattern.captures_iter(enum_block) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let number: i32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);

            // Get comment for this value
            let val_start = block_offset + caps.get(0).unwrap().start();
            let description = self.extract_enum_value_comment(content, val_start);

            values.push(EnumValueDoc {
                name,
                number,
                description,
            });
        }

        values
    }

    /// Extract comment for an enum value.
    fn extract_enum_value_comment(&self, content: &str, val_pos: usize) -> String {
        let line_idx = find_line_index(content, val_pos);
        let lines: Vec<&str> = content.lines().collect();
        extract_preceding_comment_lines(&lines, line_idx)
    }
}

impl Default for ProtoParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse all proto files in a directory and return combined documentation.
pub fn parse_all_protos(
    proto_dir: &Path,
) -> Result<(
    HashMap<String, ProtoMessageDoc>,
    HashMap<String, ProtoEnumDoc>,
)> {
    let parser = ProtoParser::new();
    let mut messages = HashMap::new();
    let mut enums = HashMap::new();

    let resources_dir = proto_dir.join("resources");
    let enums_dir = proto_dir.join("enums");

    // Parse resource proto files
    if resources_dir.exists() {
        for entry in WalkDir::new(&resources_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "proto") {
                let content = std::fs::read_to_string(path)?;
                let parsed = parser.parse_proto_file(&content);

                for msg in parsed {
                    messages.insert(msg.message_name.clone(), msg);
                }
            }
        }
    }

    // Parse enum proto files
    if enums_dir.exists() {
        for entry in WalkDir::new(&enums_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "proto") {
                let content = std::fs::read_to_string(path)?;
                let parsed = parser.parse_enum_file(&content);

                for enum_doc in parsed {
                    enums.insert(enum_doc.enum_name.clone(), enum_doc);
                }
            }
        }
    }

    Ok((messages, enums))
}

/// Convert proto field to GAQL field name.
/// E.g., Campaign.name -> campaign.name
pub fn proto_to_gaql_field(resource: &str, field: &str) -> String {
    let resource_snake = to_snake_case(resource);
    format!("{}.{}", resource_snake, field)
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    // Estimate capacity: original length plus ~20% for underscores
    let mut result = String::with_capacity(s.len() + s.len() / 5);

    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PROTO: &str = r#"
syntax = "proto3";

package google.ads.googleads.v23.resources;

// A campaign resource.
message Campaign {
  // The name of the campaign.
  string name = 1;

  // Output only. The status of the campaign.
  // When a new campaign is added, the default value is ENABLED.
  google.ads.googleads.v23.enums.CampaignStatusEnum.CampaignStatus status = 2 [
    (google.api.field_behavior) = OUTPUT_ONLY
  ];
}
"#;

    #[test]
    fn test_parse_proto_message() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(SAMPLE_PROTO);

        assert_eq!(messages.len(), 1);
        let campaign = &messages[0];
        assert_eq!(campaign.message_name, "Campaign");
        assert!(campaign.description.contains("campaign"));

        // Check fields
        assert_eq!(campaign.fields.len(), 2);

        let name_field = &campaign.fields[0];
        assert_eq!(name_field.field_name, "name");
        assert_eq!(name_field.field_number, 1);
        assert!(name_field.description.contains("name"));

        let status_field = &campaign.fields[1];
        assert_eq!(status_field.field_name, "status");
        assert_eq!(status_field.field_number, 2);
        assert!(status_field.description.contains("status"));
        assert!(status_field.description.contains("ENABLED"));
        assert!(
            status_field
                .field_behavior
                .contains(&FieldBehavior::OutputOnly)
        );
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Campaign"), "campaign");
        assert_eq!(to_snake_case("AdGroup"), "ad_group");
        assert_eq!(to_snake_case("CampaignBudget"), "campaign_budget");
    }

    #[test]
    fn test_proto_to_gaql_field() {
        assert_eq!(proto_to_gaql_field("Campaign", "name"), "campaign.name");
        assert_eq!(
            proto_to_gaql_field("AdGroup", "campaign"),
            "ad_group.campaign"
        );
    }

    const NESTED_PROTO: &str = r#"
message AccessibleBiddingStrategy {
  // Message describing TargetRoas.
  message TargetRoas {
    // Output only. The target ROAS.
    double target_roas = 1;
  }

  // Message describing TargetCpa.
  message TargetCpa {
    // Output only. The target CPA.
    int64 target_cpa_micros = 1;
  }

  // The resource name.
  string resource_name = 1;

  // Output only. The ID.
  int64 id = 2;

  // The target ROAS bidding strategy.
  TargetRoas target_roas = 11;

  // The target CPA bidding strategy.
  TargetCpa target_cpa = 12;
}
"#;

    #[test]
    fn test_nested_message_extraction() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(NESTED_PROTO);

        assert_eq!(messages.len(), 1);
        let strategy = &messages[0];
        assert_eq!(strategy.message_name, "AccessibleBiddingStrategy");

        // Should only extract parent-level fields, NOT fields from nested messages
        // Parent fields: resource_name (1), id (2), target_roas (11), target_cpa (12)
        // Nested fields that should NOT be extracted:
        //   - TargetRoas.target_roas (1)
        //   - TargetCpa.target_cpa_micros (1)
        assert_eq!(
            strategy.fields.len(),
            4,
            "Should extract exactly 4 parent-level fields, not nested fields"
        );

        // Check that we have the correct parent fields
        let field_names: Vec<&str> = strategy
            .fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();
        assert!(field_names.contains(&"resource_name"));
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"target_roas"));
        assert!(field_names.contains(&"target_cpa"));

        // Verify field numbers are correct
        let target_roas_field = strategy
            .fields
            .iter()
            .find(|f| f.field_name == "target_roas")
            .unwrap();
        assert_eq!(
            target_roas_field.field_number, 11,
            "target_roas should have field number 11 (parent), not 1 (nested)"
        );
        assert_eq!(
            target_roas_field.type_name, "TargetRoas",
            "target_roas should be message type, not double"
        );

        // Verify we didn't extract the nested field with the same name
        let target_roas_count = strategy
            .fields
            .iter()
            .filter(|f| f.field_name == "target_roas")
            .count();
        assert_eq!(
            target_roas_count, 1,
            "Should only have one target_roas field (the parent one)"
        );
    }

    const MULTIPLE_NESTED_PROTO: &str = r#"
message Campaign {
  message NestedBudget {
    int64 amount_micros = 1;
    string delivery_method = 2;
  }

  message NestedSetting {
    bool optimize = 1;
    string target = 2;
  }

  // Campaign fields
  string resource_name = 1;
  int64 id = 2;
  string name = 3;

  // Fields using nested types
  NestedBudget budget = 10;
  NestedSetting setting = 11;
}
"#;

    #[test]
    fn test_multiple_nested_messages() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(MULTIPLE_NESTED_PROTO);

        assert_eq!(messages.len(), 1);
        let campaign = &messages[0];
        assert_eq!(campaign.message_name, "Campaign");

        // Should extract exactly 5 parent fields (3 primitive + 2 nested types)
        assert_eq!(
            campaign.fields.len(),
            5,
            "Should extract exactly 5 parent fields, not fields from nested messages"
        );

        // Verify field names
        let field_names: Vec<&str> = campaign
            .fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();
        assert!(field_names.contains(&"resource_name"));
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"budget"));
        assert!(field_names.contains(&"setting"));

        // Verify nested message fields are NOT present
        assert!(
            !field_names.contains(&"amount_micros"),
            "NestedBudget.amount_micros should not be extracted"
        );
        assert!(
            !field_names.contains(&"delivery_method"),
            "NestedBudget.delivery_method should not be extracted"
        );
        assert!(
            !field_names.contains(&"optimize"),
            "NestedSetting.optimize should not be extracted"
        );
        assert!(
            !field_names.contains(&"target"),
            "NestedSetting.target should not be extracted"
        );
    }

    const DEEP_NESTED_PROTO: &str = r#"
message Outer {
  message Middle {
    message Inner {
      string deep_field = 1;
    }
    string middle_field = 1;
    Inner inner = 2;
  }
  string outer_field = 1;
  Middle middle = 2;
}
"#;

    #[test]
    fn test_deeply_nested_messages() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(DEEP_NESTED_PROTO);

        assert_eq!(messages.len(), 1);
        let outer = &messages[0];
        assert_eq!(outer.message_name, "Outer");

        // Should only extract 2 parent fields (outer_field and middle)
        assert_eq!(
            outer.fields.len(),
            2,
            "Should extract exactly 2 parent fields, ignoring all nested fields"
        );

        let field_names: Vec<&str> = outer.fields.iter().map(|f| f.field_name.as_str()).collect();
        assert!(field_names.contains(&"outer_field"));
        assert!(field_names.contains(&"middle"));

        // Ensure NO nested fields at any depth are extracted
        assert!(
            !field_names.contains(&"middle_field"),
            "Middle.middle_field should not be extracted"
        );
        assert!(
            !field_names.contains(&"deep_field"),
            "Inner.deep_field should not be extracted"
        );
        assert!(
            !field_names.contains(&"inner"),
            "Middle.inner field should not be extracted"
        );
    }

    const ACCESSIBLE_BIDDING_STRATEGY_PROTO: &str = r#"
message AccessibleBiddingStrategy {
  // Message describing a maximize conversions bid strategy.
  // Maximize conversions is an automated bidding strategy which attempts
  // to get the most conversions for the campaign.
  message MaximizeConversions {
    // Output only. The target CPA.
    optional int64 target_cpa_micros = 1;

    // Output only. The target cpa opt out field.
    optional bool cpa_opt_out = 2;
  }

  // Message describing a TargetCpa bidding strategy.
  message TargetCpa {
    // Output only. Average CPA target.
    int64 target_cpa_micros = 1;
  }

  // Message describing a TargetImpressionShare bid strategy.
  message TargetImpressionShare {
    // Output only. The targeted location on the search results page.
    int64 location = 1;

    // The chosen fraction of ads to be shown in the targeted location.
    int64 location_fraction_micros = 2;

    // Output only. Maximum bid limit.
    int64 cpc_bid_ceiling_micros = 3;
  }

  // Message describing a TargetRoas bid strategy.
  message TargetRoas {
    // Output only. The target return on ad spend (ROAS) option.
    double target_roas = 1;

    // Output only. The chosen revenue per unit of spend.
    double target_roas_value = 2;
  }

  // Message describing a TargetSpend bid strategy.
  message TargetSpend {
    // Output only. The spend target.
    int64 target_spend_micros = 1;

    // Output only. Maximum bid limit.
    int64 cpc_bid_ceiling_micros = 2;
  }

  // Output only. The resource name.
  string resource_name = 1;

  // Output only. The ID.
  int64 id = 2;

  // Output only. The name.
  string name = 3;

  // Output only. The type.
  int64 type = 4;

  // Output only. Maximize conversions strategy metadata.
  MaximizeConversions maximize_conversions = 5;

  // Output only. Target CPA strategy metadata.
  TargetCpa target_cpa = 6;

  // Output only. Target impression share strategy metadata.
  TargetImpressionShare target_impression_share = 7;

  // Output only. Target ROAS strategy metadata.
  TargetRoas target_roas = 8;

  // Output only. Target spend strategy metadata.
  TargetSpend target_spend = 9;
}
"#;

    #[test]
    fn test_accessible_bidding_strategy_no_duplicates() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(ACCESSIBLE_BIDDING_STRATEGY_PROTO);

        assert_eq!(messages.len(), 1);
        let strategy = &messages[0];
        assert_eq!(strategy.message_name, "AccessibleBiddingStrategy");

        // Should extract exactly 9 parent fields
        assert_eq!(
            strategy.fields.len(),
            9,
            "Should extract exactly 9 parent fields, not fields from nested bidding strategy types"
        );

        // Build a map of field_name -> (field_number, type_name)
        let field_map: std::collections::HashMap<&str, (u32, &str)> = strategy
            .fields
            .iter()
            .map(|f| {
                (
                    f.field_name.as_str(),
                    (f.field_number, f.type_name.as_str()),
                )
            })
            .collect();

        // Verify each parent field exists with correct field number
        assert_eq!(field_map.get("resource_name"), Some(&(1, "string")));
        assert_eq!(field_map.get("id"), Some(&(2, "int64")));
        assert_eq!(field_map.get("name"), Some(&(3, "string")));
        assert_eq!(field_map.get("type"), Some(&(4, "int64")));
        assert_eq!(
            field_map.get("maximize_conversions"),
            Some(&(5, "MaximizeConversions"))
        );
        assert_eq!(field_map.get("target_cpa"), Some(&(6, "TargetCpa")));
        assert_eq!(
            field_map.get("target_impression_share"),
            Some(&(7, "TargetImpressionShare"))
        );
        assert_eq!(field_map.get("target_roas"), Some(&(8, "TargetRoas")));
        assert_eq!(field_map.get("target_spend"), Some(&(9, "TargetSpend")));

        // Verify NO nested fields are present
        assert!(
            !field_map.contains_key("target_cpa_micros"),
            "target_cpa_micros from nested messages should NOT be extracted"
        );
        assert!(
            !field_map.contains_key("target_roas_value"),
            "target_roas_value from nested messages should NOT be extracted"
        );
        assert!(
            !field_map.contains_key("cpa_opt_out"),
            "cpa_opt_out from nested messages should NOT be extracted"
        );
        assert!(
            !field_map.contains_key("location_fraction_micros"),
            "location_fraction_micros from nested messages should NOT be extracted"
        );

        // Most importantly: verify NO duplicate field names
        let mut field_names: Vec<&str> = strategy
            .fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();
        field_names.sort();
        let unique_names: std::collections::HashSet<&str> = field_names.iter().cloned().collect();
        assert_eq!(
            field_names.len(),
            unique_names.len(),
            "Found duplicate field names in extracted fields"
        );

        // Verify target_roas field is the parent one (field 8, type TargetRoas),
        // NOT the nested one (field 1, type double)
        let target_roas_fields: Vec<_> = strategy
            .fields
            .iter()
            .filter(|f| f.field_name == "target_roas")
            .collect();
        assert_eq!(
            target_roas_fields.len(),
            1,
            "Should have exactly one target_roas field"
        );
        let target_roas = target_roas_fields[0];
        assert_eq!(
            target_roas.field_number, 8,
            "target_roas should be field 8 (parent)"
        );
        assert_eq!(
            target_roas.type_name, "TargetRoas",
            "target_roas should be message type"
        );
    }

    const NESTED_WITH_ONEOF_PROTO: &str = r#"
message Campaign {
  message NetworkSettings {
    bool target_google_search = 1;
    bool target_search_network = 2;
  }

  message HotelSettingInfo {
    int64 hotel_center_id = 1;
  }

  string resource_name = 1;
  int64 id = 2;

  oneof campaign_bidding_strategy {
    int64 manual_cpc = 10;
    int64 manual_cpm = 11;
  }

  NetworkSettings network_settings = 20;
  HotelSettingInfo hotel_setting = 21;
}
"#;

    #[test]
    fn test_nested_messages_with_oneof() {
        let parser = ProtoParser::new();
        let messages = parser.parse_proto_file(NESTED_WITH_ONEOF_PROTO);

        assert_eq!(messages.len(), 1);
        let campaign = &messages[0];

        // Should extract: resource_name, id, manual_cpc, manual_cpm, network_settings, hotel_setting = 6 fields
        // Should NOT extract: target_google_search, target_search_network, hotel_center_id
        assert_eq!(
            campaign.fields.len(),
            6,
            "Should extract exactly 6 parent fields"
        );

        let field_names: Vec<&str> = campaign
            .fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();

        // Verify parent fields
        assert!(field_names.contains(&"resource_name"));
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"manual_cpc"));
        assert!(field_names.contains(&"manual_cpm"));
        assert!(field_names.contains(&"network_settings"));
        assert!(field_names.contains(&"hotel_setting"));

        // Verify nested fields are NOT present
        assert!(!field_names.contains(&"target_google_search"));
        assert!(!field_names.contains(&"target_search_network"));
        assert!(!field_names.contains(&"hotel_center_id"));
    }
}
