//! Benchmark showing real-world compression ratios for MCP schemas.
//!
//! This demonstrates the effectiveness of schema compression on
//! typical MCP tool definitions.

use ferroclaw::mcp::compression::compress_tools;
use ferroclaw::types::ToolDefinition;
use serde_json::json;

fn main() {
    println!("=== MCP Schema Compression Benchmark ===\n");

    // Simulate a real MCP server with multiple tools
    let tools = create_mock_mcp_tools();

    let total_original: usize = tools.iter().map(|t| t.input_schema.to_string().len()).sum();

    let (compressed_tools, metrics) = compress_tools(&tools);

    println!("MCP Server with {} tools:", tools.len());
    println!(
        "  Original: {} chars (~{} tokens)",
        total_original,
        total_original / 4
    );
    println!(
        "  Compressed: {} chars (~{} tokens)",
        metrics.compressed_size,
        metrics.compressed_size / 4
    );
    println!("  Reduction: {:.1}%", metrics.reduction_percent());
    println!(
        "  Tokens saved: ~{}",
        (total_original - metrics.compressed_size) / 4
    );

    if metrics.reduction_percent() >= 70.0 {
        println!("  ✓ Meets 70-93% target!");
    } else if metrics.reduction_percent() >= 50.0 {
        println!("  △ Good reduction, below target");
    } else {
        println!("  ○ Modest reduction");
    }

    println!("\n--- Per-Tool Breakdown ---");
    for (i, (original, compressed)) in tools.iter().zip(compressed_tools.iter()).enumerate() {
        let orig_size = original.input_schema.to_string().len();
        let comp_size = compressed.input_schema.to_string().len();
        let reduction = ((orig_size - comp_size) as f64 / orig_size as f64) * 100.0;

        println!("{}. {}:", i + 1, original.name);
        println!("   {} → {} chars ({:.1}%)", orig_size, comp_size, reduction);
    }

    println!("\n--- Compression Strategies Applied ---");
    println!("✓ Removed: examples, defaults, titles, $schema, $id");
    println!("✓ Truncated: descriptions to 80 chars");
    println!("✓ Collapsed: oneOf/anyOf to union types");
    println!("✓ Preserved: required fields, types, property structure");
}

fn create_mock_mcp_tools() -> Vec<ToolDefinition> {
    vec![
        // Tool 1: File system operations
        ToolDefinition {
            name: "read_file".into(),
            description: "Read the complete contents of a file from the filesystem. Supports both text and binary files with automatic encoding detection. The file path must be absolute and within the allowed directories. Large files may be truncated for performance.".into(),
            input_schema: json!({
                "type": "object",
                "title": "Read File Tool",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "path": {
                        "type": "string",
                        "title": "File Path",
                        "description": "Absolute path to the file to read. Must be within allowed directories.",
                        "examples": ["/home/user/documents/report.pdf", "/var/log/system.log"],
                        "minLength": 1
                    },
                    "encoding": {
                        "type": "string",
                        "title": "Character Encoding",
                        "description": "Character encoding to use when reading the file. Defaults to UTF-8.",
                        "default": "utf-8",
                        "examples": ["utf-8", "latin-1", "ascii"]
                    },
                    "max_size": {
                        "type": "integer",
                        "title": "Maximum Size",
                        "description": "Maximum file size in bytes to read. Larger files will be truncated.",
                        "default": 1048576,
                        "minimum": 0
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            server_name: Some("filesystem".into()),
        },

        // Tool 2: Search operations
        ToolDefinition {
            name: "search_files".into(),
            description: "Search for files matching a glob pattern within a directory tree. Supports recursive search with configurable depth limits. Can exclude specific patterns like node_modules or .git directories. Case sensitivity can be controlled for filename matching.".into(),
            input_schema: json!({
                "type": "object",
                "title": "Search Files Tool",
                "description": "Advanced file search with glob patterns and exclusions",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "path": {
                        "type": "string",
                        "title": "Search Root",
                        "description": "Root directory to start the search from. Must be an absolute path.",
                        "examples": ["/home/user/projects", "/var/www"]
                    },
                    "pattern": {
                        "type": "string",
                        "title": "Glob Pattern",
                        "description": "Glob pattern to match filenames against. Supports * and ** wildcards.",
                        "examples": ["*.rs", "**/*.txt", "test_*.json"]
                    },
                    "exclude_patterns": {
                        "type": "array",
                        "title": "Exclusion Patterns",
                        "description": "Array of glob patterns to exclude from search results.",
                        "items": {
                            "type": "string",
                            "description": "A single exclusion pattern"
                        },
                        "default": ["node_modules", ".git", "target", "dist"]
                    },
                    "max_depth": {
                        "type": "integer",
                        "title": "Max Depth",
                        "description": "Maximum directory depth to traverse. 0 means unlimited depth.",
                        "default": 10,
                        "minimum": 0,
                        "maximum": 100
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "title": "Case Sensitive",
                        "description": "Whether pattern matching should be case-sensitive.",
                        "default": false
                    }
                },
                "required": ["path", "pattern"]
            }),
            server_name: Some("filesystem".into()),
        },

        // Tool 3: Git operations
        ToolDefinition {
            name: "git_commit".into(),
            description: "Create a new git commit with staged changes. Supports automatic staging of specified files and custom commit messages. The commit will be created with the current git user configuration. Returns the commit SHA for reference.".into(),
            input_schema: json!({
                "type": "object",
                "title": "Git Commit Tool",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "message": {
                        "type": "string",
                        "title": "Commit Message",
                        "description": "The commit message to use. Should follow conventional commit format.",
                        "examples": ["feat: add new feature", "fix: resolve bug in authentication"],
                        "minLength": 1,
                        "maxLength": 1000
                    },
                    "files": {
                        "oneOf": [
                            {
                                "type": "array",
                                "description": "List of files to stage before committing",
                                "items": {"type": "string"}
                            },
                            {
                                "type": "string",
                                "description": "Glob pattern of files to stage"
                            }
                        ]
                    },
                    "allow_empty": {
                        "type": "boolean",
                        "title": "Allow Empty",
                        "description": "Whether to allow creating a commit even if no changes are staged.",
                        "default": false
                    },
                    "amend": {
                        "type": "boolean",
                        "title": "Amend Last Commit",
                        "description": "If true, amend the previous commit instead of creating a new one.",
                        "default": false
                    }
                },
                "required": ["message"]
            }),
            server_name: Some("git".into()),
        },

        // Tool 4: Database query
        ToolDefinition {
            name: "db_query".into(),
            description: "Execute a SQL query on the database with support for parameterized queries and multiple output formats. Provides comprehensive access to the database layer while maintaining security through parameterization. Includes query timeout and result limiting features.".into(),
            input_schema: json!({
                "type": "object",
                "title": "Database Query Tool",
                "description": "Execute SQL queries with parameterization and result formatting",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "$id": "https://example.com/schemas/db-query.json",
                "properties": {
                    "query": {
                        "type": "string",
                        "title": "SQL Query",
                        "description": "The SQL query to execute. Must be a SELECT statement. Supports JOINs, subqueries, and complex WHERE clauses.",
                        "examples": [
                            "SELECT * FROM users WHERE age > ?",
                            "SELECT name, email FROM orders JOIN users ON orders.user_id = users.id"
                        ],
                        "minLength": 1
                    },
                    "parameters": {
                        "type": "array",
                        "title": "Query Parameters",
                        "description": "Parameters for parameterized queries to prevent SQL injection.",
                        "items": {
                            "type": "string",
                            "description": "A single parameter value"
                        },
                        "default": []
                    },
                    "limit": {
                        "type": "integer",
                        "title": "Result Limit",
                        "description": "Maximum number of results to return. 0 means no limit.",
                        "default": 100,
                        "minimum": 0,
                        "maximum": 10000
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "title": "Timeout",
                        "description": "Query timeout in milliseconds. Queries exceeding this will be cancelled.",
                        "default": 5000,
                        "minimum": 100,
                        "maximum": 60000
                    },
                    "format": {
                        "oneOf": [
                            {"type": "string", "enum": ["json"], "description": "JSON array format"},
                            {"type": "string", "enum": ["csv"], "description": "CSV format"},
                            {"type": "string", "enum": ["tsv"], "description": "Tab-separated values"}
                        ],
                        "description": "Output format for results",
                        "default": "json"
                    }
                },
                "required": ["query"]
            }),
            server_name: Some("database".into()),
        },

        // Tool 5: HTTP request
        ToolDefinition {
            name: "http_request".into(),
            description: "Make an HTTP request to a specified URL with support for various methods, headers, and request bodies. Returns the response status, headers, and body. Supports timeouts and automatic retry logic for transient failures.".into(),
            input_schema: json!({
                "type": "object",
                "title": "HTTP Request Tool",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "url": {
                        "type": "string",
                        "title": "Request URL",
                        "description": "The URL to send the request to. Must include protocol (http/https).",
                        "examples": ["https://api.example.com/users", "http://localhost:8080/health"],
                        "format": "uri"
                    },
                    "method": {
                        "type": "string",
                        "title": "HTTP Method",
                        "description": "The HTTP method to use for the request.",
                        "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"],
                        "default": "GET"
                    },
                    "headers": {
                        "type": "object",
                        "title": "Request Headers",
                        "description": "HTTP headers to include with the request.",
                        "default": {}
                    },
                    "body": {
                        "oneOf": [
                            {"type": "string", "description": "Raw request body"},
                            {"type": "object", "description": "JSON request body"}
                        ],
                        "description": "Request body to send. Only used for POST, PUT, and PATCH methods."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "title": "Request Timeout",
                        "description": "Timeout in milliseconds. Default is 30 seconds.",
                        "default": 30000,
                        "minimum": 100
                    },
                    "follow_redirects": {
                        "type": "boolean",
                        "title": "Follow Redirects",
                        "description": "Whether to automatically follow HTTP redirects.",
                        "default": true
                    }
                },
                "required": ["url"]
            }),
            server_name: Some("http".into()),
        },
    ]
}
