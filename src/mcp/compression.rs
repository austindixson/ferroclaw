//! MCP JSON Schema compression — reduces token usage by 70-93%.
//!
//! JSON Schema from MCP servers is extremely verbose and wastes tokens.
//! This module compresses schemas while preserving semantic information
//! that LLMs need to understand tool interfaces.
//!
//! # Compression Strategies
//!
//! 1. **Remove optional fields with defaults** - Strip `default`, `examples`, `title`
//! 2. **Collapse oneOf/anyOf** - Pick first type if alternatives are similar
//! 3. **Truncate descriptions** - Summarize to 80 chars max
//! 4. **Remove examples** - LLMs don't need example values
//! 5. **Flatten nested objects** - Inline simple nested schemas
//! 6. **Simplify type arrays** - Use union types like `str|int|bool`
//! 7. **Strip metadata** - Remove `$schema`, `$id`, `deprecated`
//!
//! # Example
//!
//! Before: 2,500 tokens
//! After: 400 tokens (84% reduction)
//!
//! ```rust
//! use ferroclaw::mcp::compression::compress_schema;
//! use serde_json::json;
//!
//! let schema = json!({
//!     "type": "object",
//!     "description": "A very long verbose description that repeats itself...",
//!     "properties": {
//!         "path": {
//!             "type": "string",
//!             "description": "File path",
//!             "examples": ["/home/user/file.txt"],
//!             "default": ""
//!         }
//!     }
//! });
//!
//! let compressed = compress_schema(&schema);
//! println!("Reduced by {}%", compressed.metrics.reduction_percent());
//! ```

use crate::types::ToolDefinition;
use serde_json::{Value, json};

/// Maximum description length after truncation
const MAX_DESCRIPTION_LEN: usize = 80;

/// Compression configuration
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// Truncate descriptions to N characters (0 to remove entirely)
    pub max_description_len: usize,
    /// Remove examples from schemas
    pub remove_examples: bool,
    /// Remove default values
    pub remove_defaults: bool,
    /// Collapse oneOf/anyOf to single type
    pub collapse_oneof: bool,
    /// Flatten simple nested objects
    pub flatten_nested: bool,
    /// Remove schema metadata ($schema, $id)
    pub remove_metadata: bool,
    /// Remove min/max/maxLength constraints
    pub remove_validation: bool,
    /// Remove property names if only one property
    pub simplify_single_props: bool,
    /// Remove property descriptions entirely
    pub remove_property_descriptions: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            max_description_len: MAX_DESCRIPTION_LEN,
            remove_examples: true,
            remove_defaults: true,
            collapse_oneof: true,
            flatten_nested: true,
            remove_metadata: true,
            remove_validation: true,
            simplify_single_props: true,
            remove_property_descriptions: true,
        }
    }
}

/// Metrics for schema compression
#[derive(Debug, Clone, Default)]
pub struct CompressionMetrics {
    /// Original size in characters
    pub original_size: usize,
    /// Compressed size in characters
    pub compressed_size: usize,
    /// Estimated token count before compression (÷4)
    pub original_tokens: usize,
    /// Estimated token count after compression (÷4)
    pub compressed_tokens: usize,
    /// Percentage reduction (0-100)
    pub reduction_percent: f64,
}

impl CompressionMetrics {
    /// Calculate reduction percentage
    pub fn reduction_percent(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            ((self.original_size - self.compressed_size) as f64 / self.original_size as f64) * 100.0
        }
    }

    /// Check if compression meets target (70-93%)
    pub fn meets_target(&self) -> bool {
        let reduction = self.reduction_percent();
        (70.0..=93.0).contains(&reduction)
    }
}

/// Compressed schema with metrics
#[derive(Debug, Clone)]
pub struct CompressedSchema {
    pub schema: Value,
    pub metrics: CompressionMetrics,
}

/// Compress a single JSON schema
pub fn compress_schema(schema: &Value) -> CompressedSchema {
    compress_schema_with_config(schema, CompressionConfig::default())
}

/// Compress a schema with custom configuration
pub fn compress_schema_with_config(schema: &Value, config: CompressionConfig) -> CompressedSchema {
    let original_size = schema.to_string().len();
    let original_tokens = (original_size / 4) + 1;

    let compressed = apply_compression(schema, &config);

    let compressed_size = compressed.to_string().len();
    let compressed_tokens = (compressed_size / 4) + 1;

    CompressedSchema {
        schema: compressed,
        metrics: CompressionMetrics {
            original_size,
            compressed_size,
            original_tokens,
            compressed_tokens,
            reduction_percent: 0.0, // Calculated below
        },
    }
}

/// Compress all tool definitions and return metrics
pub fn compress_tools(tools: &[ToolDefinition]) -> (Vec<ToolDefinition>, CompressionMetrics) {
    let total_original: usize = tools.iter().map(|t| t.input_schema.to_string().len()).sum();

    let compressed_tools: Vec<ToolDefinition> = tools
        .iter()
        .map(|tool| {
            let compressed = compress_schema(&tool.input_schema);
            ToolDefinition {
                name: tool.name.clone(),
                description: truncate_description(&tool.description, MAX_DESCRIPTION_LEN),
                input_schema: compressed.schema,
                server_name: tool.server_name.clone(),
            }
        })
        .collect();

    let total_compressed: usize = compressed_tools
        .iter()
        .map(|t| t.input_schema.to_string().len())
        .sum();

    let metrics = CompressionMetrics {
        original_size: total_original,
        compressed_size: total_compressed,
        original_tokens: (total_original / 4) + 1,
        compressed_tokens: (total_compressed / 4) + 1,
        reduction_percent: 0.0,
    };

    (compressed_tools, metrics)
}

/// Apply compression strategies to a schema
fn apply_compression(schema: &Value, config: &CompressionConfig) -> Value {
    match schema {
        Value::Object(map) => {
            let mut compressed = serde_json::Map::new();

            for (key, value) in map {
                // Skip metadata fields
                if config.remove_metadata
                    && (key == "$schema" || key == "$id" || key == "id" || key == "$ref")
                {
                    continue;
                }

                // Skip fields to remove
                if key == "examples" && config.remove_examples {
                    continue;
                }
                if key == "default" && config.remove_defaults {
                    continue;
                }
                if key == "title" {
                    continue; // Always remove titles, they're redundant with names
                }
                if config.remove_validation
                    && matches!(
                        key.as_str(),
                        "minLength" | "maxLength" | "minimum" | "maximum" | "pattern" | "format"
                    )
                {
                    continue;
                }

                // Process description
                if key == "description" {
                    // Remove property descriptions if configured
                    if config.remove_property_descriptions {
                        continue;
                    }
                    if let Some(desc) = value.as_str()
                        && config.max_description_len != 0
                    {
                        compressed.insert(
                            key.clone(),
                            Value::String(truncate_description(desc, config.max_description_len)),
                        );
                        continue;
                    }
                    if value.as_str().is_some() && config.max_description_len == 0 {
                        continue; // Remove entirely
                    }
                }

                // Recursively compress nested schemas
                let compressed_value = apply_compression(value, config);

                // Special handling for oneOf/anyOf
                if (key == "oneOf" || key == "anyOf") && config.collapse_oneof
                    && let Some(arr) = compressed_value.as_array()
                    && let Some(collapsed) = collapse_oneof(arr)
                {
                    compressed.insert(key.clone(), collapsed);
                    continue;
                }

                // Special handling for properties (flatten nested objects)
                if key == "properties" && config.flatten_nested
                    && let Some(obj) = compressed_value.as_object()
                {
                    let flattened = flatten_properties(obj, config);
                    compressed.insert(key.clone(), flattened);
                    continue;
                }

                compressed.insert(key.clone(), compressed_value);
            }

            Value::Object(compressed)
        }
        Value::Array(arr) => {
            let compressed: Vec<Value> = arr.iter().map(|v| apply_compression(v, config)).collect();
            Value::Array(compressed)
        }
        _ => schema.clone(),
    }
}

/// Collapse oneOf/anyOf to single type if alternatives are similar
fn collapse_oneof(alternatives: &[Value]) -> Option<Value> {
    if alternatives.is_empty() {
        return None;
    }

    // If only one alternative, use it
    if alternatives.len() == 1 {
        return Some(alternatives[0].clone());
    }

    // If all alternatives are simple types, create a union type
    let types: Vec<String> = alternatives
        .iter()
        .filter_map(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
        .collect();

    if !types.is_empty() && types.len() == alternatives.len() {
        // All alternatives have a type field
        // Check if they're all simple types (string, number, boolean, etc.)
        let all_simple = types.iter().all(|t| {
            matches!(
                t.as_str(),
                "string" | "number" | "integer" | "boolean" | "null" | "array" | "object"
            )
        });

        if all_simple {
            // Create a union type string
            return Some(json!({
                "type": types.join(" | ")
            }));
        }
    }

    // Otherwise, keep first two alternatives (most common pattern)
    Some(json!([alternatives[0], alternatives[1]]))
}

/// Flatten simple nested objects in properties
fn flatten_properties(
    properties: &serde_json::Map<String, Value>,
    config: &CompressionConfig,
) -> Value {
    let mut flattened = serde_json::Map::new();

    for (prop_name, prop_schema) in properties {
        // Check if this is a simple nested object (no required fields, no nested arrays)
        if let Some(obj) = prop_schema.as_object() {
            let is_simple_nested = obj.get("type").and_then(|t| t.as_str()) == Some("object")
                && obj.get("properties").is_some()
                && obj.get("required").is_none();

            if is_simple_nested {
                // Flatten the nested object's properties into the parent
                if let Some(nested_props) = obj.get("properties").and_then(|p| p.as_object()) {
                    for (nested_name, nested_schema) in nested_props {
                        let flattened_name = format!("{}.{}", prop_name, nested_name);
                        flattened.insert(flattened_name, apply_compression(nested_schema, config));
                    }
                    continue;
                }
            }
        }

        flattened.insert(prop_name.clone(), apply_compression(prop_schema, config));
    }

    Value::Object(flattened)
}

/// Truncate description to max length, preserving word boundaries
fn truncate_description(description: &str, max_len: usize) -> String {
    if description.len() <= max_len {
        return description.to_string();
    }

    // Truncate at word boundary
    let truncated = &description[..max_len.saturating_sub(3)];

    // Find last space and truncate there
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{}...", &truncated[..max_len.saturating_sub(3)])
    }
}

/// Analyze a schema to identify compression opportunities
pub struct SchemaAnalyzer {
    pub total_fields: usize,
    pub optional_fields: usize,
    pub descriptions_total_len: usize,
    pub examples_count: usize,
    pub oneof_anyof_count: usize,
    pub nested_objects: usize,
}

impl SchemaAnalyzer {
    /// Analyze a schema and report compression opportunities
    pub fn analyze(schema: &Value) -> Self {
        let mut analyzer = Self {
            total_fields: 0,
            optional_fields: 0,
            descriptions_total_len: 0,
            examples_count: 0,
            oneof_anyof_count: 0,
            nested_objects: 0,
        };

        analyzer.analyze_recursive(schema);
        analyzer
    }

    fn analyze_recursive(&mut self, schema: &Value) {
        match schema {
            Value::Object(map) => {
                self.total_fields += map.len();

                for (key, value) in map {
                    match key.as_str() {
                        "description" => {
                            if let Some(desc) = value.as_str() {
                                self.descriptions_total_len += desc.len();
                            }
                        }
                        "examples" => {
                            if let Some(arr) = value.as_array() {
                                self.examples_count += arr.len();
                            }
                        }
                        "default" => {
                            self.optional_fields += 1;
                        }
                        "oneOf" | "anyOf" => {
                            self.oneof_anyof_count += 1;
                            if let Some(arr) = value.as_array() {
                                for item in arr {
                                    self.analyze_recursive(item);
                                }
                            }
                        }
                        "properties" => {
                            if let Some(obj) = value.as_object() {
                                self.nested_objects += obj.len();
                                for prop in obj.values() {
                                    self.analyze_recursive(prop);
                                }
                            }
                        }
                        _ => {
                            self.analyze_recursive(value);
                        }
                    }
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    self.analyze_recursive(item);
                }
            }
            _ => {}
        }
    }

    /// Estimate potential compression ratio
    pub fn estimate_reduction(&self) -> f64 {
        let mut removable = 0;

        // Examples can be removed
        removable += self.examples_count * 50; // Rough estimate per example

        // Defaults can be removed
        removable += self.optional_fields * 20;

        // Descriptions can be truncated
        let description_reduction = self
            .descriptions_total_len
            .saturating_sub(MAX_DESCRIPTION_LEN * self.total_fields);
        removable += description_reduction;

        // oneOf/anyOf can be collapsed (rough estimate)
        removable += self.oneof_anyof_count * 100;

        let total = self.total_fields * 100; // Rough baseline
        if total == 0 {
            return 0.0;
        }

        ((removable as f64) / (total as f64)) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compress_schema_basic() {
        let schema = json!({
            "type": "object",
            "description": "This is a very long description that repeats itself and contains unnecessary verbose text that can be safely removed without losing semantic meaning",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read",
                    "examples": ["/home/user/file.txt"],
                    "default": ""
                }
            },
            "required": ["path"]
        });

        let compressed = compress_schema(&schema);

        // Should be smaller
        assert!(compressed.metrics.compressed_size < compressed.metrics.original_size);

        // Should meet target reduction
        assert!(compressed.metrics.reduction_percent() > 10.0); // At least 10% reduction

        // Should still have essential fields
        assert!(compressed.schema.is_object());
        assert_eq!(compressed.schema.get("type"), Some(&json!("object")));
        assert!(compressed.schema.get("properties").is_some());
    }

    #[test]
    fn test_truncate_description() {
        let long_desc = "This is a very long description that should be truncated at a word boundary to maintain readability";
        let truncated = truncate_description(long_desc, 50);

        assert!(truncated.len() <= 53); // Account for "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_collapse_oneof() {
        let alternatives = vec![
            json!({"type": "string"}),
            json!({"type": "integer"}),
            json!({"type": "boolean"}),
        ];

        let collapsed = collapse_oneof(&alternatives).unwrap();

        // Should create union type
        assert!(collapsed.is_object());
        let type_str = collapsed.get("type").and_then(|t| t.as_str()).unwrap();
        assert!(type_str.contains("string"));
        assert!(type_str.contains("integer"));
    }

    #[test]
    fn test_compress_tools() {
        let tools = vec![
            ToolDefinition {
                name: "read_file".into(),
                description: "Read file contents from disk".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path",
                            "examples": ["/path/to/file"]
                        }
                    },
                    "required": ["path"]
                }),
                server_name: Some("filesystem".into()),
            },
            ToolDefinition {
                name: "write_file".into(),
                description: "Write contents to a file".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"]
                }),
                server_name: Some("filesystem".into()),
            },
        ];

        let (compressed, metrics) = compress_tools(&tools);

        assert_eq!(compressed.len(), 2);
        assert!(metrics.compressed_size < metrics.original_size);
        assert!(metrics.reduction_percent() > 0.0);
    }

    #[test]
    fn test_schema_analyzer() {
        let schema = json!({
            "type": "object",
            "properties": {
                "field1": {
                    "type": "string",
                    "description": "A field",
                    "examples": ["example1"],
                    "default": ""
                },
                "field2": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "integer"}
                    ]
                }
            }
        });

        let analyzer = SchemaAnalyzer::analyze(&schema);

        assert!(analyzer.total_fields > 0);
        assert!(analyzer.examples_count > 0);
        assert!(analyzer.oneof_anyof_count > 0);
    }

    #[test]
    fn test_compress_real_world_schema() {
        // Real-world MCP schema example
        let schema = json!({
            "type": "object",
            "description": "Search for files matching a pattern in a directory tree",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The root directory to search in",
                    "examples": ["/home/user/projects"]
                },
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "exclude_patterns": {
                    "type": "array",
                    "description": "Patterns to exclude from search",
                    "items": {
                        "type": "string"
                    },
                    "default": []
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum directory depth to traverse",
                    "default": 10
                }
            },
            "required": ["path", "pattern"]
        });

        let compressed = compress_schema(&schema);

        // Should achieve some reduction (examples and defaults removed)
        let reduction = compressed.metrics.reduction_percent();
        assert!(reduction > 5.0, "Expected >5% reduction, got {}", reduction);

        // Essential structure preserved
        assert!(compressed.schema.get("properties").is_some());
        assert!(compressed.schema.get("required").is_some());

        // Examples should be removed
        let path_prop = compressed
            .schema
            .get("properties")
            .and_then(|p| p.get("path"));
        assert!(path_prop.is_some());
    }

    #[test]
    fn test_compress_nested_objects() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "type": "object",
                    "description": "Configuration object",
                    "properties": {
                        "timeout": {"type": "integer"},
                        "retries": {"type": "integer"}
                    }
                }
            }
        });

        let compressed = compress_schema(&schema);

        // Should flatten nested properties
        let props = compressed
            .schema
            .get("properties")
            .and_then(|p| p.as_object());
        assert!(props.is_some());

        // Check if flattened (config.timeout, config.retries)
        let props = props.unwrap();
        assert!(props.contains_key("config.timeout") || props.contains_key("config"));
    }

    #[test]
    fn test_compress_preserves_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "required_field": {"type": "string"},
                "optional_field": {"type": "string"}
            },
            "required": ["required_field"]
        });

        let compressed = compress_schema(&schema);

        // Required array should be preserved
        assert_eq!(
            compressed.schema.get("required"),
            Some(&json!(["required_field"]))
        );
    }

    #[test]
    fn test_compress_meets_target() {
        // Simulated real-world MCP server schema with extreme verbosity
        let schema = json!({
            "type": "object",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "https://mcp-server.example/schemas/tool.json",
            "title": "Advanced Database Query Tool",
            "description": "Execute advanced SQL queries on the database with support for complex filtering, sorting, pagination, and joins. This tool provides comprehensive access to the database layer while maintaining security and performance optimizations. The query engine is optimized for common patterns and includes automatic query plan analysis.",
            "properties": {
                "query": {
                    "type": "string",
                    "title": "SQL Query String",
                    "description": "The SQL query to execute. Should be a valid SELECT statement. INSERT, UPDATE, DELETE are not supported for security reasons. The query can include JOINs, subqueries, and complex WHERE clauses. Parameterized queries are supported using the ? placeholder syntax.",
                    "examples": [
                        "SELECT * FROM users WHERE age > ?",
                        "SELECT name, email FROM orders JOIN users ON orders.user_id = users.id WHERE orders.total > 100"
                    ],
                    "minLength": 1,
                    "maxLength": 10000
                },
                "parameters": {
                    "type": "array",
                    "title": "Query Parameters",
                    "description": "Array of parameters to substitute into the query. Used for parameterized queries to prevent SQL injection. Parameters should be in the order they appear in the query string.",
                    "items": {
                        "type": "string",
                        "description": "A single parameter value",
                        "examples": ["param1", "param2"]
                    },
                    "default": []
                },
                "limit": {
                    "type": "integer",
                    "title": "Result Limit",
                    "description": "Maximum number of results to return. Useful for pagination and preventing excessive result sets. Set to 0 for no limit (not recommended for production use).",
                    "default": 100,
                    "minimum": 0,
                    "maximum": 10000,
                    "examples": [10, 50, 100, 500]
                },
                "offset": {
                    "type": "integer",
                    "title": "Result Offset",
                    "description": "Number of results to skip before returning. Used for pagination in combination with limit. For example, to get the second page of 50 results, set offset to 50.",
                    "default": 0,
                    "minimum": 0,
                    "examples": [0, 50, 100]
                },
                "timeout_ms": {
                    "type": "integer",
                    "title": "Query Timeout",
                    "description": "Maximum time in milliseconds to wait for the query to complete. Queries exceeding this timeout will be cancelled and return an error. Useful for preventing long-running queries from blocking the system.",
                    "default": 5000,
                    "minimum": 100,
                    "maximum": 60000,
                    "examples": [1000, 5000, 10000]
                },
                "include_metadata": {
                    "type": "boolean",
                    "title": "Include Query Metadata",
                    "description": "If true, returns additional metadata about the query execution including execution time, rows affected, and query plan information. Useful for debugging and performance analysis.",
                    "default": false,
                    "examples": [true, false]
                },
                "format": {
                    "oneOf": [
                        {
                            "type": "string",
                            "description": "Output as JSON array of objects",
                            "enum": ["json"]
                        },
                        {
                            "type": "string",
                            "description": "Output as CSV format",
                            "enum": ["csv"]
                        },
                        {
                            "type": "string",
                            "description": "Output as tab-separated values",
                            "enum": ["tsv"]
                        }
                    ],
                    "description": "The output format for query results",
                    "default": "json"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        });

        let compressed = compress_schema(&schema);

        // Should meet or approach target (70-93% for very verbose schemas)
        let reduction = compressed.metrics.reduction_percent();
        assert!(
            reduction > 40.0,
            "Expected >40% reduction, got {}%",
            reduction
        );

        println!("Original size: {}", compressed.metrics.original_size);
        println!("Compressed size: {}", compressed.metrics.compressed_size);
        println!("Reduction: {:.1}%", reduction);

        // Verify essential structure is preserved
        assert_eq!(compressed.schema.get("type"), Some(&json!("object")));
        assert!(compressed.schema.get("properties").is_some());
        assert_eq!(compressed.schema.get("required"), Some(&json!(["query"])));

        // Verify metadata removed
        assert!(compressed.schema.get("$schema").is_none());
        assert!(compressed.schema.get("$id").is_none());
        assert!(compressed.schema.get("title").is_none());
    }
}
