//! Code review tool - quality analysis with scoring

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;

pub fn review_code_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "review_code".into(),
            description: "Perform automated code review with quality scoring (0-100), issue detection, and actionable recommendations. Supports Rust, Python, JavaScript.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file or directory to review"
                    },
                    "severity": {
                        "type": "string",
                        "enum": ["all", "high", "medium", "low"],
                        "description": "Filter issues by severity"
                    },
                    "categories": {
                        "type": "string",
                        "enum": ["all", "security", "performance", "style", "correctness", "complexity"],
                        "description": "Filter issues by category"
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

pub struct ReviewCodeHandler;

impl ToolHandler for ReviewCodeHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            let severity = arguments
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("all");

            let categories = arguments
                .get("categories")
                .and_then(|c| c.as_str())
                .unwrap_or("all");

            let content = tokio::fs::read_to_string(path)
                .await
                .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let review = match ext {
                "rs" => review_rust(&content, severity, categories),
                "py" => review_python(&content, severity, categories),
                "js" | "ts" | "jsx" | "tsx" => review_javascript(&content, severity, categories),
                _ => Err(FerroError::Tool(format!(
                    "Unsupported language: {}. Supported: Rust (.rs), Python (.py), JavaScript/TypeScript (.js, .ts, .jsx, .tsx)",
                    ext
                ))),
            }?;

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: review,
                is_error: false,
            })
        })
    }
}

fn review_rust(content: &str, severity: &str, categories: &str) -> Result<String, FerroError> {
    let mut issues = Vec::new();
    let mut score = 100;
    let lines: Vec<&str> = content.lines().collect();

    // Check for various issues
    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Security issues
        if categories == "all" || categories == "security" {
            if line.contains("unwrap()") && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message:
                        "Potential panic from unwrap(). Consider using pattern matching or expect()"
                            .into(),
                    code: line.to_string(),
                });
                score -= 5;
            }

            if line.contains(".clone()") && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "medium".into(),
                    category: "performance".into(),
                    message: "Unnecessary clone() may impact performance".into(),
                    code: line.to_string(),
                });
                score -= 2;
            }

            if line.contains("unsafe") && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message: "Unsafe code detected. Ensure proper safety guarantees".into(),
                    code: line.to_string(),
                });
                score -= 10;
            }
        }

        // Performance issues
        if (categories == "all" || categories == "performance")
            && line.contains(".collect::<Vec<_>>") && !line.contains("//")
        {
            issues.push(CodeIssue {
                line: line_num + 1,
                severity: "low".into(),
                category: "performance".into(),
                message: "Unnecessary collection may impact performance".into(),
                code: line.to_string(),
            });
            score -= 1;
        }

        // Style issues
        if categories == "all" || categories == "style" {
            if line.len() > 100 && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "low".into(),
                    category: "style".into(),
                    message: "Line too long (>100 characters)".into(),
                    code: line.to_string(),
                });
                score -= 1;
            }

            if !line.is_empty() && line.ends_with(' ') {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "low".into(),
                    category: "style".into(),
                    message: "Trailing whitespace".into(),
                    code: line.to_string(),
                });
                score -= 1;
            }
        }

        // Complexity issues
        if categories == "all" || categories == "complexity" {
            let brace_count = line.matches('{').count() + line.matches('}').count();
            if brace_count > 4 && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "medium".into(),
                    category: "complexity".into(),
                    message: "High nesting complexity detected".into(),
                    code: line.to_string(),
                });
                score -= 3;
            }
        }
    }

    // Filter by severity
    let issues = if severity != "all" {
        issues
            .into_iter()
            .filter(|i| i.severity == severity)
            .collect()
    } else {
        issues
    };

    // Ensure score is within bounds
    score = score.clamp(0, 100);

    let mut output = String::new();
    output.push_str("🔍 Code Review Report\n");
    output.push_str("═════════════════════════════\n\n");

    output.push_str(&format!("📊 Overall Score: {}/100\n\n", score));

    if issues.is_empty() {
        output.push_str("✅ No issues found!\n");
        output.push_str("The code looks clean and follows best practices.\n");
    } else {
        output.push_str(&format!("⚠️  Found {} issue(s):\n\n", issues.len()));

        // Group by severity
        let high_severity: Vec<_> = issues.iter().filter(|i| i.severity == "high").collect();
        let medium_severity: Vec<_> = issues.iter().filter(|i| i.severity == "medium").collect();
        let low_severity: Vec<_> = issues.iter().filter(|i| i.severity == "low").collect();

        if !high_severity.is_empty() {
            output.push_str("🔴 High Severity:\n");
            for issue in high_severity {
                output.push_str(&format!(
                    "  Line {}: {} ({})\n",
                    issue.line, issue.message, issue.category
                ));
                output.push_str(&format!("    → {}\n", issue.code.trim()));
            }
            output.push('\n');
        }

        if !medium_severity.is_empty() {
            output.push_str("🟡 Medium Severity:\n");
            for issue in medium_severity.iter().take(5) {
                output.push_str(&format!(
                    "  Line {}: {} ({})\n",
                    issue.line, issue.message, issue.category
                ));
                output.push_str(&format!("    → {}\n", issue.code.trim()));
            }
            if medium_severity.len() > 5 {
                output.push_str(&format!("  ... and {} more\n", medium_severity.len() - 5));
            }
            output.push('\n');
        }

        if !low_severity.is_empty() {
            output.push_str("🟢 Low Severity:\n");
            for issue in low_severity.iter().take(5) {
                output.push_str(&format!(
                    "  Line {}: {} ({})\n",
                    issue.line, issue.message, issue.category
                ));
            }
            if low_severity.len() > 5 {
                output.push_str(&format!("  ... and {} more\n", low_severity.len() - 5));
            }
        }
    }

    output.push_str("\n💡 Recommendations:\n");
    output.push_str("  - Address high-severity issues first\n");
    output.push_str("  - Run `cargo clippy` for additional lints\n");
    output.push_str("  - Consider using `rustfmt` for code formatting\n");

    Ok(output)
}

fn review_python(content: &str, severity: &str, categories: &str) -> Result<String, FerroError> {
    let mut issues = Vec::new();
    let mut score = 100;
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        if categories == "all" || categories == "security" {
            if line.contains("eval(") && !line.contains("#") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message: "Avoid using eval() - security risk".into(),
                    code: line.to_string(),
                });
                score -= 10;
            }

            if line.contains("exec(") && !line.contains("#") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message: "Avoid using exec() - security risk".into(),
                    code: line.to_string(),
                });
                score -= 10;
            }
        }

        if (categories == "all" || categories == "performance")
            && line.contains("import *") && !line.contains("#")
        {
            issues.push(CodeIssue {
                line: line_num + 1,
                severity: "low".into(),
                category: "performance".into(),
                message: "Wildcard imports can impact performance".into(),
                code: line.to_string(),
            });
            score -= 2;
        }

        if (categories == "all" || categories == "style")
            && line.len() > 88 && !line.contains("#")
        {
            issues.push(CodeIssue {
                line: line_num + 1,
                severity: "low".into(),
                category: "style".into(),
                message: "Line too long (>79 characters, PEP8 recommended)".into(),
                code: line.to_string(),
            });
            score -= 1;
        }
    }

    let issues = if severity != "all" {
        issues
            .into_iter()
            .filter(|i| i.severity == severity)
            .collect()
    } else {
        issues
    };

    score = score.clamp(0, 100);

    let mut output = String::new();
    output.push_str("🔍 Python Code Review Report\n");
    output.push_str("════════════════════════════════════\n\n");
    output.push_str(&format!("📊 Overall Score: {}/100\n\n", score));

    if issues.is_empty() {
        output.push_str("✅ No issues found!\n");
    } else {
        output.push_str(&format!("⚠️  Found {} issue(s):\n\n", issues.len()));
        for issue in &issues {
            let icon = match issue.severity.as_str() {
                "high" => "🔴",
                "medium" => "🟡",
                "low" => "🟢",
                _ => "⚪",
            };
            output.push_str(&format!(
                "{} Line {}: {} ({})\n",
                icon, issue.line, issue.message, issue.category
            ));
            output.push_str(&format!("  → {}\n", issue.code.trim()));
        }
    }

    output.push_str("\n💡 Recommendations:\n");
    output.push_str("  - Run `ruff` or `pylint` for additional checks\n");
    output.push_str("  - Follow PEP 8 style guidelines\n");
    output.push_str("  - Use type hints where appropriate\n");

    Ok(output)
}

fn review_javascript(
    content: &str,
    severity: &str,
    categories: &str,
) -> Result<String, FerroError> {
    let mut issues = Vec::new();
    let mut score = 100;
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        if categories == "all" || categories == "security" {
            if line.contains("innerHTML") && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message: "Direct innerHTML usage can lead to XSS vulnerabilities".into(),
                    code: line.to_string(),
                });
                score -= 10;
            }

            if line.contains("eval(") && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "high".into(),
                    category: "security".into(),
                    message: "Avoid using eval() - security risk".into(),
                    code: line.to_string(),
                });
                score -= 10;
            }
        }

        if (categories == "all" || categories == "style")
            && line.contains("var ") && !line.contains("//") && !line.contains("for(")
        {
            issues.push(CodeIssue {
                line: line_num + 1,
                severity: "low".into(),
                category: "style".into(),
                message: "Consider using let/const instead of var".into(),
                code: line.to_string(),
            });
            score -= 1;
        }

        if categories == "all" || categories == "complexity" {
            let brace_count = line.matches('{').count() + line.matches('}').count();
            if brace_count > 4 && !line.contains("//") {
                issues.push(CodeIssue {
                    line: line_num + 1,
                    severity: "medium".into(),
                    category: "complexity".into(),
                    message: "High nesting complexity detected".into(),
                    code: line.to_string(),
                });
                score -= 3;
            }
        }
    }

    let issues = if severity != "all" {
        issues
            .into_iter()
            .filter(|i| i.severity == severity)
            .collect()
    } else {
        issues
    };

    score = score.clamp(0, 100);

    let mut output = String::new();
    output.push_str("🔍 JavaScript/TypeScript Code Review Report\n");
    output.push_str("═════════════════════════════════════════\n\n");
    output.push_str(&format!("📊 Overall Score: {}/100\n\n", score));

    if issues.is_empty() {
        output.push_str("✅ No issues found!\n");
    } else {
        output.push_str(&format!("⚠️  Found {} issue(s):\n\n", issues.len()));
        for issue in &issues {
            let icon = match issue.severity.as_str() {
                "high" => "🔴",
                "medium" => "🟡",
                "low" => "🟢",
                _ => "⚪",
            };
            output.push_str(&format!(
                "{} Line {}: {} ({})\n",
                icon, issue.line, issue.message, issue.category
            ));
            output.push_str(&format!("  → {}\n", issue.code.trim()));
        }
    }

    output.push_str("\n💡 Recommendations:\n");
    output.push_str("  - Run ESLint for additional checks\n");
    output.push_str("  - Use TypeScript for type safety\n");
    output.push_str("  - Consider using Prettier for code formatting\n");

    Ok(output)
}

struct CodeIssue {
    line: usize,
    severity: String,
    category: String,
    message: String,
    code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_rust() {
        let code = r#"
fn main() {
    let x = Some(5).unwrap();
    let y = x.clone();
    println!("{}", y);
}
"#;
        let result = review_rust(code, "all", "all").unwrap();
        assert!(result.contains("Overall Score:"));
        assert!(result.contains("issue(s)"));
    }

    #[test]
    fn test_review_python() {
        let code = r#"
def foo():
    x = eval("1 + 1")
    return x
"#;
        let result = review_python(code, "all", "all").unwrap();
        assert!(result.contains("Overall Score:"));
        assert!(result.contains("eval"));
    }
}
