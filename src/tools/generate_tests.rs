//! Test generation tool - create unit/integration tests

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;

pub fn generate_tests_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "generate_tests".into(),
            description: "Generate unit tests and integration tests for a file. Supports Rust, Python, JavaScript, TypeScript.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file to generate tests for"
                    },
                    "test_type": {
                        "type": "string",
                        "enum": ["unit", "integration", "both"],
                        "description": "Type of tests to generate"
                    }
                },
                "required": ["path"]
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::FsRead, Capability::FsWrite],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct GenerateTestsHandler;

impl ToolHandler for GenerateTestsHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            let test_type = arguments
                .get("test_type")
                .and_then(|t| t.as_str())
                .unwrap_or("unit");

            let content = tokio::fs::read_to_string(path)
                .await
                .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let test_code = match ext {
                "rs" => generate_rust_tests(&content, test_type).await?,
                "py" => generate_python_tests(&content, test_type).await?,
                "js" | "ts" => generate_javascript_tests(&content, test_type).await?,
                _ => {
                    return Err(FerroError::Tool(format!(
                        "Unsupported language: {}. Supported: Rust (.rs), Python (.py), JavaScript (.js), TypeScript (.ts)",
                        ext
                    )));
                }
            };

            // Write test file
            let test_path = match ext {
                "rs" => path.replace(".rs", "_test.rs"),
                "py" => path.replace(".py", "_test.py"),
                "js" => path.replace(".js", ".test.js"),
                "ts" => path.replace(".ts", ".test.ts"),
                _ => format!("{}.test", path),
            };

            tokio::fs::write(&test_path, test_code)
                .await
                .map_err(|e| FerroError::Tool(format!("Cannot write {}: {}", test_path, e)))?;

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: format!(
                    "✅ Generated {} tests for: {}\nTest file: {}",
                    test_type, path, test_path
                ),
                is_error: false,
            })
        })
    }
}

async fn generate_rust_tests(content: &str, test_type: &str) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str("// Automatically generated tests\n");
    output.push_str("use super::*;\n\n");

    // Extract functions
    let mut functions = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ") {
            let sig = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("async ")
                .trim_start_matches("fn ");
            let name = sig.split('(').next().unwrap_or("");
            if !name.is_empty() {
                functions.push(name.trim().to_string());
            }
        }
    }

    output.push_str("#[cfg(test)]\n");
    output.push_str("mod tests {\n");
    output.push_str("    use super::*;\n\n");

    if test_type == "unit" || test_type == "both" {
        for func in &functions {
            output.push_str("    #[test]\n");
            output.push_str(&format!("    fn test_{}() {{\n", func));
            output.push_str(&format!("        // TODO: Implement test for {}\n", func));
            output.push_str("        assert!(true); // Placeholder\n");
            output.push_str("    }}\n\n");
        }
    }

    if test_type == "integration" || test_type == "both" {
        output.push_str("    // Integration tests\n");
        for func in &functions {
            output.push_str("    #[test]\n");
            output.push_str(&format!("    fn integration_test_{}() {{\n", func));
            output.push_str(&format!(
                "        // TODO: Implement integration test for {}\n",
                func
            ));
            output.push_str("        assert!(true); // Placeholder\n");
            output.push_str("    }}\n\n");
        }
    }

    output.push_str("}\n");

    Ok(output)
}

async fn generate_python_tests(content: &str, test_type: &str) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str("# Automatically generated tests\n");
    output.push_str("import unittest\n");
    output.push_str("from unittest.mock import patch, MagicMock\n");

    // Extract class and function names
    let mut classes = Vec::new();
    let mut functions = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") {
            let decl = trimmed.trim_start_matches("class ");
            let name = decl
                .split(':')
                .next()
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                classes.push(name.trim().to_string());
            }
        } else if trimmed.starts_with("def ") {
            let sig = trimmed.trim_start_matches("def ");
            let name = sig.split('(').next().unwrap_or("");
            if !name.is_empty() && !name.starts_with("_") {
                functions.push(name.trim().to_string());
            }
        }
    }

    output.push('\n');

    if test_type == "unit" || test_type == "both" {
        for func in &functions {
            output.push_str(&format!(
                "class Test{}(unittest.TestCase):\n",
                capitalize(func)
            ));
            output.push_str(&format!("    def test_{}(self):\n", func));
            output.push_str(&format!("        # TODO: Implement test for {}\n", func));
            output.push_str("        self.assertTrue(True) # Placeholder\n\n");
        }

        for cls in &classes {
            output.push_str(&format!("class Test{}(unittest.TestCase):\n", cls));
            output.push_str("    def setUp(self):\n");
            output.push_str(&format!("        # TODO: Setup for {} instance\n", cls));
            output.push_str("        pass\n\n");

            output.push_str(&format!(
                "    def test_{}_basic(self):\n",
                to_snake_case(cls)
            ));
            output.push_str(&format!("        # TODO: Basic test for {}\n", cls));
            output.push_str("        self.assertTrue(True) # Placeholder\n\n");
        }
    }

    if test_type == "integration" || test_type == "both" {
        output.push_str("# Integration tests\n");
        for func in &functions {
            output.push_str(&format!(
                "class IntegrationTest{}(unittest.TestCase):\n",
                capitalize(func)
            ));
            output.push_str(&format!("    def test_{}_integration(self):\n", func));
            output.push_str(&format!(
                "        # TODO: Implement integration test for {}\n",
                func
            ));
            output.push_str("        self.assertTrue(True) # Placeholder\n\n");
        }
    }

    output.push('\n');
    output.push_str("if __name__ == '__main__':\n");
    output.push_str("    unittest.main()\n");

    Ok(output)
}

async fn generate_javascript_tests(content: &str, test_type: &str) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str("// Automatically generated tests\n\n");

    // Detect test framework preference
    let has_jest = content.contains("jest") || content.contains("describe(");
    let use_jest = has_jest || content.contains("@jest/globals");

    if use_jest {
        output.push_str("describe('Auto-generated tests', () => {\n");
    } else {
        output.push_str("// Using standard assert (consider using Jest or Mocha)\n\n");
    }

    // Extract functions
    let mut functions = Vec::new();
    let mut exports = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("function ")
            || trimmed.contains(" = function")
            || trimmed.contains(" = (")
        {
            let name = if trimmed.starts_with("export function ") {
                let sig = trimmed.trim_start_matches("export function ");
                sig.split('(').next().unwrap_or("")
            } else if trimmed.starts_with("function ") {
                let sig = trimmed.trim_start_matches("function ");
                sig.split('(').next().unwrap_or("")
            } else {
                ""
            };
            let fn_name = name
                .split('(')
                .next()
                .unwrap_or("")
                .split('=')
                .next()
                .unwrap_or("")
                .trim();
            if !fn_name.is_empty() && !fn_name.contains("=") {
                functions.push(fn_name.to_string());
            }
        }

        if trimmed.starts_with("export ") {
            exports.push(trimmed.trim_start_matches("export ").to_string());
        }
    }

    if test_type == "unit" || test_type == "both" {
        for func in &functions {
            if use_jest {
                output.push_str(&format!("  test('{}', () => {{\n", func));
                output.push_str(&format!("    // TODO: Implement test for {}\n", func));
                output.push_str("    expect(true).toBe(true); // Placeholder\n");
                output.push_str("  }});\n\n");
            } else {
                output.push_str(&format!("// test for {}\n", func));
                output.push_str(&format!("function test_{}() {{\n", func));
                output.push_str(&format!("  // TODO: Implement test for {}\n", func));
                output.push_str("  assert(true, 'Placeholder');\n");
                output.push_str("}}\n\n");
            }
        }
    }

    if test_type == "integration" || test_type == "both" {
        for func in &functions {
            if use_jest {
                output.push_str(&format!("  test('{} integration', () => {{\n", func));
                output.push_str(&format!(
                    "    // TODO: Implement integration test for {}\n",
                    func
                ));
                output.push_str("    expect(true).toBe(true); // Placeholder\n");
                output.push_str("  }});\n\n");
            } else {
                output.push_str(&format!("// integration test for {}\n", func));
                output.push_str(&format!("function test_{}_integration() {{\n", func));
                output.push_str(&format!(
                    "  // TODO: Implement integration test for {}\n",
                    func
                ));
                output.push_str("  assert(true, 'Placeholder');\n");
                output.push_str("}}\n\n");
            }
        }
    }

    if use_jest {
        output.push_str("});\n");
    }

    Ok(output)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().collect::<Vec<_>>()[0]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_rust_tests() {
        let code = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(generate_rust_tests(code, "unit"))
            .unwrap();
        assert!(result.contains("test_add"));
        assert!(result.contains("assert!(true)"));
    }

    #[test]
    fn test_generate_python_tests() {
        let code = "def add(a, b):\n    return a + b";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(generate_python_tests(code, "unit"))
            .unwrap();
        assert!(result.contains("TestAdd"));
        assert!(result.contains("assertTrue"));
    }
}
