//! Code analysis tool - understand structure, dependencies, complexity

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;

pub fn analyze_code_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "analyze_code".into(),
            description: "Analyze code structure, dependencies, and complexity. Supports Rust, Python, JavaScript, TypeScript.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file or directory to analyze"
                    },
                    "analysis_type": {
                        "type": "string",
                        "enum": ["structure", "dependencies", "complexity", "imports", "all"],
                        "description": "Type of analysis to perform"
                    }
                },
                "required": ["path"]
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::FsRead],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct AnalyzeCodeHandler;

impl ToolHandler for AnalyzeCodeHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            let analysis_type = arguments
                .get("analysis_type")
                .and_then(|t| t.as_str())
                .unwrap_or("all");

            // Check if path is a file or directory
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|e| FerroError::Tool(format!("Cannot access {}: {}", path, e)))?;

            let result = if metadata.is_file() {
                analyze_file(path, analysis_type).await?
            } else {
                analyze_directory(path, analysis_type).await?
            };

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: result,
                is_error: false,
            })
        })
    }
}

async fn analyze_file(path: &str, analysis_type: &str) -> Result<String, FerroError> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => analyze_rust_file(&content, analysis_type),
        "py" => analyze_python_file(&content, analysis_type),
        "js" | "ts" | "jsx" | "tsx" => analyze_javascript_file(&content, analysis_type),
        _ => Ok(format!(
            "⚠️  Unsupported language: {}\nSupported: Rust (.rs), Python (.py), JavaScript/TypeScript (.js, .ts, .jsx, .tsx)",
            ext
        )),
    }
}

fn analyze_rust_file(content: &str, analysis_type: &str) -> Result<String, FerroError> {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    let mut impls = Vec::new();
    let mut mods = Vec::new();
    let mut traits = Vec::new();
    let mut uses = Vec::new();
    let mut consts = Vec::new();
    let mut statics = Vec::new();
    let mut types = Vec::new();
    let mut lines = 0;
    let mut code_lines = 0;
    let mut comment_lines = 0;

    for line in content.lines() {
        lines += 1;
        let trimmed = line.trim();
        let is_comment =
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*");

        if is_comment {
            comment_lines += 1;
        } else if !trimmed.is_empty() {
            code_lines += 1;
        }

        // Functions
        if trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub async fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("async fn ")
        {
            let sig = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("async ")
                .trim_start_matches("fn ");
            let name = sig.split('(').next().unwrap_or("");
            if !name.is_empty() {
                functions.push(name.trim().to_string());
            }
        }

        // Structs
        if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("struct ");
            let name = decl
                .split('{')
                .next()
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                structs.push(name.trim().to_string());
            }
        }

        // Enums
        if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("enum ");
            let name = decl.split('{').next().unwrap_or("");
            if !name.is_empty() {
                enums.push(name.trim().to_string());
            }
        }

        // Impl blocks
        if trimmed.starts_with("impl ") {
            let impl_decl = trimmed.trim_start_matches("impl ");
            let name = impl_decl
                .split('{')
                .next()
                .unwrap_or("")
                .split(" for ")
                .next()
                .unwrap_or(impl_decl);
            if !name.is_empty() {
                impls.push(name.trim().to_string());
            }
        }

        // Modules
        if trimmed.starts_with("pub mod ") || trimmed.starts_with("mod ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("mod ");
            let name = decl
                .split('{')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                mods.push(name.trim().to_string());
            }
        }

        // Traits
        if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("trait ");
            let name = decl.split('{').next().unwrap_or("");
            if !name.is_empty() {
                traits.push(name.trim().to_string());
            }
        }

        // Use statements
        if trimmed.starts_with("use ") {
            let use_decl = trimmed.trim_start_matches("use ");
            let name = use_decl.split(';').next().unwrap_or("");
            if !name.is_empty() {
                uses.push(name.trim().to_string());
            }
        }

        // Constants
        if trimmed.starts_with("pub const ") || trimmed.starts_with("const ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("const ");
            let name = decl
                .split(':')
                .next()
                .unwrap_or("")
                .split('=')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                consts.push(name.trim().to_string());
            }
        }

        // Static variables
        if trimmed.starts_with("pub static ") || trimmed.starts_with("static ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("static ");
            let name = decl
                .split(':')
                .next()
                .unwrap_or("")
                .split('=')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                statics.push(name.trim().to_string());
            }
        }

        // Type aliases
        if trimmed.starts_with("pub type ") || trimmed.starts_with("type ") {
            let decl = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("type ");
            let name = decl
                .split('=')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                types.push(name.trim().to_string());
            }
        }
    }

    let mut output = String::new();
    output.push_str("🦀 Rust Code Analysis\n");
    output.push_str("═══════════════════════════════════════\n\n");

    if analysis_type == "structure" || analysis_type == "all" {
        output.push_str("📊 Structure\n");
        output.push_str(&format!("  Modules:     {}\n", mods.len()));
        output.push_str(&format!("  Structs:     {}\n", structs.len()));
        output.push_str(&format!("  Enums:       {}\n", enums.len()));
        output.push_str(&format!("  Traits:     {}\n", traits.len()));
        output.push_str(&format!("  Impl Blocks: {}\n", impls.len()));
        output.push_str(&format!("  Functions:   {}\n", functions.len()));
        output.push_str(&format!("  Use Statements: {}\n", uses.len()));
        output.push_str(&format!("  Constants:   {}\n", consts.len()));
        output.push_str(&format!("  Statics:     {}\n", statics.len()));
        output.push_str(&format!("  Type Aliases: {}\n", types.len()));
        output.push('\n');
    }

    if analysis_type == "complexity" || analysis_type == "all" {
        output.push_str("📈 Complexity Metrics\n");
        output.push_str(&format!("  Total Lines:     {}\n", lines));
        output.push_str(&format!("  Code Lines:      {}\n", code_lines));
        output.push_str(&format!("  Comment Lines:   {}\n", comment_lines));
        let comment_ratio = if lines > 0 {
            (comment_lines as f64 / lines as f64 * 100.0) as usize
        } else {
            0
        };
        output.push_str(&format!("  Comment Ratio:   {}%\n", comment_ratio));
        output.push('\n');
    }

    if analysis_type == "all" && !functions.is_empty() {
        output.push_str("📝 Functions:\n");
        for fn_name in functions.iter().take(10) {
            output.push_str(&format!("  • {}\n", fn_name));
        }
        if functions.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", functions.len() - 10));
        }
        output.push('\n');
    }

    Ok(output)
}

fn analyze_python_file(content: &str, analysis_type: &str) -> Result<String, FerroError> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut lines = 0;
    let mut code_lines = 0;
    let mut comment_lines = 0;

    for line in content.lines() {
        lines += 1;
        let trimmed = line.trim();
        let is_comment =
            trimmed.starts_with("#") || trimmed.starts_with("'''") || trimmed.starts_with("\"\"\"");

        if is_comment {
            comment_lines += 1;
        } else if !trimmed.is_empty() {
            code_lines += 1;
        }

        // Functions
        if trimmed.starts_with("def ") {
            let sig = trimmed.trim_start_matches("def ");
            let name = sig.split('(').next().unwrap_or("");
            if !name.is_empty() {
                functions.push(name.trim().to_string());
            }
        }

        // Classes
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
        }

        // Imports
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            let import_decl = trimmed
                .trim_start_matches("import ")
                .trim_start_matches("from ");
            let name = import_decl
                .split(" import")
                .next()
                .unwrap_or("")
                .split(" as ")
                .next()
                .unwrap_or("")
                .split(',')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                imports.push(name.trim().to_string());
            }
        }
    }

    let mut output = String::new();
    output.push_str("🐍 Python Code Analysis\n");
    output.push_str("═══════════════════════════════════════\n\n");

    if analysis_type == "structure" || analysis_type == "all" {
        output.push_str("📊 Structure\n");
        output.push_str(&format!("  Classes:     {}\n", classes.len()));
        output.push_str(&format!("  Functions:   {}\n", functions.len()));
        output.push_str(&format!("  Imports:     {}\n", imports.len()));
        output.push('\n');
    }

    if analysis_type == "complexity" || analysis_type == "all" {
        output.push_str("📈 Complexity Metrics\n");
        output.push_str(&format!("  Total Lines:     {}\n", lines));
        output.push_str(&format!("  Code Lines:      {}\n", code_lines));
        output.push_str(&format!("  Comment Lines:   {}\n", comment_lines));
        let comment_ratio = if lines > 0 {
            (comment_lines as f64 / lines as f64 * 100.0) as usize
        } else {
            0
        };
        output.push_str(&format!("  Comment Ratio:   {}%\n", comment_ratio));
        output.push('\n');
    }

    Ok(output)
}

fn analyze_javascript_file(content: &str, analysis_type: &str) -> Result<String, FerroError> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut lines = 0;
    let mut code_lines = 0;
    let mut comment_lines = 0;

    for line in content.lines() {
        lines += 1;
        let trimmed = line.trim();
        let is_comment =
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*");

        if is_comment {
            comment_lines += 1;
        } else if !trimmed.is_empty() {
            code_lines += 1;
        }

        // Functions (function keyword, arrow functions)
        if trimmed.starts_with("function ")
            || trimmed.contains(" = (")
            || trimmed.contains("= async (")
            || trimmed.contains(" = function")
        {
            let name = if trimmed.starts_with("function ") {
                let sig = trimmed.trim_start_matches("function ");
                sig.split('(').next().unwrap_or("")
            } else if trimmed.contains("export function ") {
                let sig = trimmed.trim_start_matches("export function ");
                sig.split('(').next().unwrap_or("")
            } else {
                // Try to extract from variable assignment
                if let Some(idx) = trimmed.find("function ") {
                    &trimmed[idx + 9..]
                } else if let Some(idx) = trimmed.find("=") {
                    trimmed[..idx].trim()
                } else {
                    ""
                }
            };
            let fn_name = name
                .split('(')
                .next()
                .unwrap_or("")
                .split('{')
                .next()
                .unwrap_or("")
                .trim();
            if !fn_name.is_empty() && fn_name != "=" {
                functions.push(fn_name.to_string());
            }
        }

        // Classes
        if trimmed.starts_with("class ") || trimmed.starts_with("export class ") {
            let decl = trimmed
                .trim_start_matches("export ")
                .trim_start_matches("class ");
            let name = decl
                .split('{')
                .next()
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .split(" extends ")
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                classes.push(name.to_string());
            }
        }

        // Imports (ES6)
        if trimmed.starts_with("import ") {
            imports.push(trimmed.trim_start_matches("import ").to_string());
        }

        // Exports
        if trimmed.starts_with("export ") {
            exports.push(trimmed.trim_start_matches("export ").to_string());
        }
    }

    let mut output = String::new();
    output.push_str("📜 JavaScript/TypeScript Code Analysis\n");
    output.push_str("════════════════════════════════════════════\n\n");

    if analysis_type == "structure" || analysis_type == "all" {
        output.push_str("📊 Structure\n");
        output.push_str(&format!("  Classes:    {}\n", classes.len()));
        output.push_str(&format!("  Functions:  {}\n", functions.len()));
        output.push_str(&format!("  Imports:    {}\n", imports.len()));
        output.push_str(&format!("  Exports:    {}\n", exports.len()));
        output.push('\n');
    }

    if analysis_type == "complexity" || analysis_type == "all" {
        output.push_str("📈 Complexity Metrics\n");
        output.push_str(&format!("  Total Lines:     {}\n", lines));
        output.push_str(&format!("  Code Lines:      {}\n", code_lines));
        output.push_str(&format!("  Comment Lines:   {}\n", comment_lines));
        let comment_ratio = if lines > 0 {
            (comment_lines as f64 / lines as f64 * 100.0) as usize
        } else {
            0
        };
        output.push_str(&format!("  Comment Ratio:   {}%\n", comment_ratio));
        output.push('\n');
    }

    Ok(output)
}

async fn analyze_directory(path: &str, _analysis_type: &str) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str(&format!("📁 Directory Analysis: {}\n", path));
    output.push_str("═══════════════════════════════════════\n\n");

    let mut entries = tokio::fs::read_dir(path)
        .await
        .map_err(|e| FerroError::Tool(format!("Cannot list {}: {}", path, e)))?;

    let mut file_count = 0;
    let mut dir_count = 0;
    let mut rust_files = Vec::new();
    let mut python_files = Vec::new();
    let mut js_files = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_path = entry.path();

        if let Ok(ft) = entry.file_type().await {
            if ft.is_file() {
                file_count += 1;
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

                match ext {
                    "rs" => rust_files.push(file_name),
                    "py" => python_files.push(file_name),
                    "js" | "ts" | "jsx" | "tsx" => js_files.push(file_name),
                    _ => {}
                }
            } else if ft.is_dir() {
                dir_count += 1;
            }
        }
    }

    output.push_str("📊 Overview\n");
    output.push_str(&format!("  Directories: {}\n", dir_count));
    output.push_str(&format!("  Files:       {}\n", file_count));
    output.push_str(&format!("  Rust files:  {}\n", rust_files.len()));
    output.push_str(&format!("  Python files: {}\n", python_files.len()));
    output.push_str(&format!("  JS/TS files:  {}\n", js_files.len()));
    output.push('\n');

    if !rust_files.is_empty() {
        output.push_str("🦀 Rust Files:\n");
        for file in rust_files.iter().take(5) {
            output.push_str(&format!("  • {}\n", file));
        }
        if rust_files.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", rust_files.len() - 5));
        }
        output.push('\n');
    }

    if !python_files.is_empty() {
        output.push_str("🐍 Python Files:\n");
        for file in python_files.iter().take(5) {
            output.push_str(&format!("  • {}\n", file));
        }
        if python_files.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", python_files.len() - 5));
        }
        output.push('\n');
    }

    if !js_files.is_empty() {
        output.push_str("📜 JS/TS Files:\n");
        for file in js_files.iter().take(5) {
            output.push_str(&format!("  • {}\n", file));
        }
        if js_files.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", js_files.len() - 5));
        }
        output.push('\n');
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_rust_file() {
        let code = r#"
pub struct MyStruct {
    value: i32,
}

pub fn my_function(x: i32) -> i32 {
    x * 2
}

impl MyStruct {
    pub fn new() -> Self {
        Self { value: 0 }
    }
}
"#;
        let result = analyze_rust_file(code, "structure").unwrap();
        assert!(result.contains("Structs:"));
        assert!(result.contains("Functions:"));
        assert!(result.contains("Impl Blocks:"));
    }

    #[test]
    fn test_analyze_python_file() {
        let code = r#"
class MyClass:
    def __init__(self, value):
        self.value = value
    
    def my_method(self):
        return self.value * 2
"#;
        let result = analyze_python_file(code, "structure").unwrap();
        assert!(result.contains("Classes:"));
        assert!(result.contains("Functions:"));
    }
}
