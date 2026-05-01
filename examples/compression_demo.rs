//! Demonstration of MCP schema compression effectiveness.
//!
//! This example shows real-world compression ratios achieved
//! by the compression module on typical MCP tool schemas.

use ferroclaw::mcp::compression::{SchemaAnalyzer, compress_schema};
use serde_json::json;

fn main() {
    println!("=== MCP Schema Compression Demo ===\n");

    // Example 1: Simple file tool
    demo_simple_tool();

    // Example 2: Complex search tool
    demo_search_tool();

    // Example 3: Tool with oneOf/anyOf
    demo_oneof_tool();

    // Example 4: Real-world verbose schema
    demo_verbose_schema();
}

fn demo_simple_tool() {
    println!("--- Example 1: Simple File Tool ---");

    let schema = json!({
        "type": "object",
        "description": "Read the contents of a file from the filesystem",
        "properties": {
            "path": {
                "type": "string",
                "description": "The absolute path to the file to read",
                "examples": ["/home/user/documents/report.pdf"]
            },
            "encoding": {
                "type": "string",
                "description": "The character encoding to use",
                "default": "utf-8"
            }
        },
        "required": ["path"]
    });

    let compressed = compress_schema(&schema);

    print_metrics(&schema, &compressed.schema, &compressed.metrics);
    println!();
}

fn demo_search_tool() {
    println!("--- Example 2: Complex Search Tool ---");

    let schema = json!({
        "type": "object",
        "description": "Search for files matching a pattern in a directory tree, with support for excluding certain patterns and limiting search depth",
        "properties": {
            "path": {
                "type": "string",
                "description": "The root directory to search in. Must be an absolute path.",
                "examples": ["/home/user/projects"]
            },
            "pattern": {
                "type": "string",
                "description": "The glob pattern to match files against (e.g., '*.rs' for Rust files)"
            },
            "exclude_patterns": {
                "type": "array",
                "description": "Patterns to exclude from search results. Useful for ignoring node_modules, .git, etc.",
                "items": {
                    "type": "string"
                },
                "default": []
            },
            "max_depth": {
                "type": "integer",
                "description": "Maximum directory depth to traverse. 0 means unlimited.",
                "default": 10
            },
            "case_sensitive": {
                "type": "boolean",
                "description": "Whether the search should be case-sensitive",
                "default": false
            }
        },
        "required": ["path", "pattern"]
    });

    let compressed = compress_schema(&schema);

    print_metrics(&schema, &compressed.schema, &compressed.metrics);
    println!();
}

fn demo_oneof_tool() {
    println!("--- Example 3: Tool with oneOf/anyOf ---");

    let schema = json!({
        "type": "object",
        "description": "A tool that accepts multiple types for a parameter",
        "properties": {
            "identifier": {
                "description": "Can be a string ID, integer index, or boolean flag",
                "oneOf": [
                    {"type": "string", "description": "A string identifier"},
                    {"type": "integer", "description": "An integer index"},
                    {"type": "boolean", "description": "A boolean flag"}
                ]
            },
            "config": {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "timeout": {"type": "integer"},
                            "retries": {"type": "integer"}
                        }
                    },
                    {
                        "type": "string",
                        "description": "Path to config file"
                    }
                ]
            }
        }
    });

    let compressed = compress_schema(&schema);

    print_metrics(&schema, &compressed.schema, &compressed.metrics);

    println!("Note: oneOf/anyOf collapsed to union types or first alternatives\n");
}

fn demo_verbose_schema() {
    println!("--- Example 4: Extremely Verbose Schema ---");

    let schema = json!({
        "type": "object",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$id": "https://example.com/schemas/verbose-tool.json",
        "title": "Verbose Tool Title That Adds No Value",
        "description": "This is an excessively verbose schema with long descriptions that repeat information multiple times and contain unnecessary details that don't help the LLM understand how to use the tool effectively. The descriptions are padded with extra words that waste tokens without adding semantic value.",
        "properties": {
            "param1": {
                "type": "string",
                "title": "Parameter 1",
                "description": "This is the first parameter which accepts a string value. The string should be properly formatted and contain valid data. This description is intentionally verbose to demonstrate compression effectiveness.",
                "examples": [
                    "example_value_1",
                    "example_value_2",
                    "example_value_3"
                ],
                "default": "",
                "minLength": 0,
                "maxLength": 1000
            },
            "param2": {
                "type": "integer",
                "title": "Parameter 2",
                "description": "This is the second parameter which accepts an integer value. Integers are whole numbers without decimal points. This parameter controls various aspects of tool behavior.",
                "examples": [1, 2, 3, 4, 5],
                "default": 0,
                "minimum": 0,
                "maximum": 100
            },
            "param3": {
                "type": "boolean",
                "title": "Parameter 3",
                "description": "This is the third parameter which accepts a boolean value. Boolean values can be either true or false. This parameter enables or disables certain features.",
                "default": false
            },
            "param4": {
                "oneOf": [
                    {"type": "string", "description": "String option"},
                    {"type": "integer", "description": "Integer option"},
                    {"type": "boolean", "description": "Boolean option"}
                ],
                "description": "This parameter accepts multiple types",
                "default": null
            }
        },
        "required": ["param1", "param2"]
    });

    let compressed = compress_schema(&schema);

    print_metrics(&schema, &compressed.schema, &compressed.metrics);

    println!("Note: Redundant metadata ($schema, $id, title) removed\n");
}

fn print_metrics(
    original: &serde_json::Value,
    compressed: &serde_json::Value,
    metrics: &ferroclaw::mcp::compression::CompressionMetrics,
) {
    println!(
        "Original size: {} chars (~{} tokens)",
        metrics.original_size, metrics.original_tokens
    );
    println!(
        "Compressed size: {} chars (~{} tokens)",
        metrics.compressed_size, metrics.compressed_tokens
    );
    println!("Reduction: {:.1}%", metrics.reduction_percent());

    if metrics.reduction_percent() >= 70.0 {
        println!("✓ Meets 70-93% target!");
    } else if metrics.reduction_percent() >= 50.0 {
        println!("△ Good reduction, below target");
    } else {
        println!("○ Modest reduction");
    }

    // Analyze opportunities
    let analyzer = SchemaAnalyzer::analyze(original);
    println!("\nAnalysis:");
    println!("  Total fields: {}", analyzer.total_fields);
    println!("  Optional fields (defaults): {}", analyzer.optional_fields);
    println!("  Examples: {}", analyzer.examples_count);
    println!("  oneOf/anyOf: {}", analyzer.oneof_anyof_count);
    println!("  Description chars: {}", analyzer.descriptions_total_len);
    println!(
        "  Estimated reduction potential: {:.1}%",
        analyzer.estimate_reduction()
    );

    println!("\nCompressed schema:");
    println!("{}", serde_json::to_string_pretty(compressed).unwrap());
}
