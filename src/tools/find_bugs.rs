//! Bug finding tool - static analysis for issues

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;

pub fn find_bugs_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "find_bugs".into(),
            description: "Find bugs and potential issues in code using static analysis patterns. Supports Rust, Python, JavaScript.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file or directory to analyze"
                    },
                    "bug_type": {
                        "type": "string",
                        "enum": ["all", "security", "logic", "concurrency", "memory", "performance"],
                        "description": "Type of bugs to find"
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

pub struct FindBugsHandler;

impl ToolHandler for FindBugsHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            let bug_type = arguments
                .get("bug_type")
                .and_then(|b| b.as_str())
                .unwrap_or("all");

            let content = tokio::fs::read_to_string(path)
                .await
                .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let bugs = match ext {
                "rs" => find_rust_bugs(&content, bug_type),
                "py" => find_python_bugs(&content, bug_type),
                "js" | "ts" | "jsx" | "tsx" => find_javascript_bugs(&content, bug_type),
                _ => Err(FerroError::Tool(format!(
                    "Unsupported language: {}. Supported: Rust (.rs), Python (.py), JavaScript/TypeScript (.js, .ts, .jsx, .tsx)",
                    ext
                ))),
            }?;

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: bugs,
                is_error: false,
            })
        })
    }
}

fn find_rust_bugs(content: &str, bug_type: &str) -> Result<String, FerroError> {
    let mut bugs = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Security bugs
        if bug_type == "all" || bug_type == "security" {
            if line.contains("unsafe") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Unsafe code detected".into(),
                    description:
                        "Unsafe code block detected. Ensure proper safety guarantees are in place."
                            .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("expect(") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "medium".into(),
                    title: "Potential panic in production".into(),
                    description:
                        "expect() will panic the program. Consider returning a Result instead."
                            .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("transmute") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Unsafe transmute".into(),
                    description: "transmute bypasses Rust's type safety. Ensure this is absolutely necessary.".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("mem::uninitialized") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Use of uninitialized memory".into(),
                    description:
                        "mem::uninitialized is deprecated and unsafe. Use MaybeUninit instead."
                            .into(),
                    code: line.to_string(),
                });
            }
        }

        // Logic bugs
        if bug_type == "all" || bug_type == "logic" {
            if line.contains("== None") || line.contains("!= None") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Comparison with None".into(),
                    description:
                        "In Rust, use `.is_none()` or `.is_some()` instead of comparing with None."
                            .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("if let Some(x) = Some(") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Redundant Some pattern".into(),
                    description:
                        "if let Some(x) = Some(y) can be simplified to if let Some(x) = y.".into(),
                    code: line.to_string(),
                });
            }
        }

        // Memory bugs
        if bug_type == "all" || bug_type == "memory" {
            if line.contains("Vec::new()")
                && line.contains(".push")
                && !line.contains("with_capacity")
                && !line.contains("//")
            {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "memory".into(),
                    severity: "low".into(),
                    title: "Unnecessary reallocations".into(),
                    description: "Consider using Vec::with_capacity() to avoid reallocations."
                        .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("Box::new(") && !line.contains("Box::pin(") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "memory".into(),
                    severity: "low".into(),
                    title: "Potential unpin issue".into(),
                    description: "Consider using Box::pin for self-referential types.".into(),
                    code: line.to_string(),
                });
            }
        }

        // Concurrency bugs
        if bug_type == "all" || bug_type == "concurrency" {
            if line.contains("Arc<Mutex<") && line.contains(".clone()") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "concurrency".into(),
                    severity: "low".into(),
                    title: "Potential lock contention".into(),
                    description: "Frequent cloning of Arc may cause performance issues.".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("thread::spawn")
                && !line.contains("JoinHandle")
                && !line.contains("//")
            {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "concurrency".into(),
                    severity: "medium".into(),
                    title: "Unjoined thread".into(),
                    description:
                        "Thread spawned but never joined. Use JoinHandle or forget explicitly."
                            .into(),
                    code: line.to_string(),
                });
            }
        }
    }

    format_bugs_report("Rust", &bugs, bug_type)
}

fn find_python_bugs(content: &str, bug_type: &str) -> Result<String, FerroError> {
    let mut bugs = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Security bugs
        if bug_type == "all" || bug_type == "security" {
            if line.contains("pickle.loads(") && !line.contains("#") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Insecure pickle deserialization".into(),
                    description:
                        "pickle.loads can execute arbitrary code. Use a safer format like JSON."
                            .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("exec(") && !line.contains("#") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Arbitrary code execution".into(),
                    description: "exec() can execute arbitrary code. Very dangerous!".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("subprocess.call(")
                && !line.contains("shell=False")
                && !line.contains("#")
            {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Shell injection risk".into(),
                    description: "subprocess.call() with shell=True is vulnerable to shell injection. Use shell=False.".into(),
                    code: line.to_string(),
                });
            }
        }

        // Logic bugs
        if bug_type == "all" || bug_type == "logic" {
            if line.contains("== None") || line.contains("!= None") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Comparison with None".into(),
                    description: "Use 'is None' or 'is not None' instead of '==' or '!='.".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("== True")
                || line.contains("!= True")
                || line.contains("== False")
                || line.contains("!= False")
            {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Comparison with boolean".into(),
                    description: "Use 'if x' or 'if not x' instead of comparing to True/False."
                        .into(),
                    code: line.to_string(),
                });
            }
        }

        // Memory bugs
        if (bug_type == "all" || bug_type == "memory")
            && line.contains("list.append(")
            && !line.contains("//")
        {
            let in_loop = lines
                .get(line_num.saturating_sub(1).saturating_sub(5)..=line_num)
                .map(|lines| {
                    lines
                        .iter()
                        .any(|l| l.contains("for ") || l.contains("while "))
                })
                .unwrap_or(false);

            if in_loop {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "memory".into(),
                    severity: "low".into(),
                    title: "List reallocation in loop".into(),
                    description:
                        "Consider pre-allocating list with known size or using list comprehension."
                            .into(),
                    code: line.to_string(),
                });
            }
        }
    }

    format_bugs_report("Python", &bugs, bug_type)
}

fn find_javascript_bugs(content: &str, bug_type: &str) -> Result<String, FerroError> {
    let mut bugs = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Security bugs
        if bug_type == "all" || bug_type == "security" {
            if line.contains("innerHTML") && !line.contains("//") && !line.contains("/*") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "XSS vulnerability".into(),
                    description: "innerHTML can lead to XSS. Use textContent or sanitize input."
                        .into(),
                    code: line.to_string(),
                });
            }

            if line.contains("eval(") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "high".into(),
                    title: "Arbitrary code execution".into(),
                    description: "eval() can execute arbitrary code. Very dangerous!".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("document.write(") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "security".into(),
                    severity: "medium".into(),
                    title: "Unsafe document.write".into(),
                    description: "document.write() can lead to security issues. Use DOM manipulation instead.".into(),
                    code: line.to_string(),
                });
            }
        }

        // Logic bugs
        if bug_type == "all" || bug_type == "logic" {
            if line.contains("== null")
                || line.contains("!= null")
                || line.contains("== undefined")
                || line.contains("!= undefined")
            {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Loose equality comparison".into(),
                    description: "Use === and !== for strict equality checks.".into(),
                    code: line.to_string(),
                });
            }

            if line.contains("var ") && !line.contains("//") {
                bugs.push(BugReport {
                    line: line_num + 1,
                    bug_type: "logic".into(),
                    severity: "low".into(),
                    title: "Use of var".into(),
                    description: "Prefer let/const over var for better scoping.".into(),
                    code: line.to_string(),
                });
            }
        }

        // Memory bugs
        if (bug_type == "all" || bug_type == "memory")
            && line.contains("Array(")
            && !line.contains("//")
        {
            bugs.push(BugReport {
                line: line_num + 1,
                bug_type: "memory".into(),
                severity: "low".into(),
                title: "Array constructor".into(),
                description: "Use array literal [] instead of Array() for better performance."
                    .into(),
                code: line.to_string(),
            });
        }
    }

    format_bugs_report("JavaScript/TypeScript", &bugs, bug_type)
}

fn format_bugs_report(
    language: &str,
    bugs: &[BugReport],
    bug_type: &str,
) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str(&format!("🐛 Bug Analysis Report: {}\n", language));
    output.push_str("═══════════════════════════════════════════════\n\n");

    if bugs.is_empty() {
        output.push_str("✅ No bugs found!\n");
        output.push_str(&format!(
            "The code appears to be free of {} bugs.\n\n",
            bug_type
        ));
        output.push_str("💡 Recommendations:\n");
        output.push_str("  - Run language-specific linters for additional checks\n");
        output.push_str("  - Use static analysis tools like Clippy, Pylint, ESLint\n");
        output.push_str("  - Consider using formal verification tools for critical code\n");
        return Ok(output);
    }

    output.push_str(&format!("⚠️  Found {} potential bug(s)\n\n", bugs.len()));

    // Group by severity
    let high_severity: Vec<_> = bugs.iter().filter(|b| b.severity == "high").collect();
    let medium_severity: Vec<_> = bugs.iter().filter(|b| b.severity == "medium").collect();
    let low_severity: Vec<_> = bugs.iter().filter(|b| b.severity == "low").collect();

    if !high_severity.is_empty() {
        output.push_str("🔴 High Severity:\n");
        for bug in high_severity {
            output.push_str(&format!(
                "  Line {}: {} ({})\n",
                bug.line, bug.title, bug.bug_type
            ));
            output.push_str(&format!("    → {}\n", bug.description));
            output.push_str(&format!("    → {}\n\n", bug.code.trim()));
        }
    }

    if !medium_severity.is_empty() {
        output.push_str("🟡 Medium Severity:\n");
        for bug in medium_severity.iter().take(5) {
            output.push_str(&format!(
                "  Line {}: {} ({})\n",
                bug.line, bug.title, bug.bug_type
            ));
            output.push_str(&format!("    → {}\n", bug.description));
            output.push_str(&format!("    → {}\n\n", bug.code.trim()));
        }
        if medium_severity.len() > 5 {
            output.push_str(&format!("  ... and {} more\n\n", medium_severity.len() - 5));
        }
    }

    if !low_severity.is_empty() {
        output.push_str("🟢 Low Severity:\n");
        for bug in low_severity.iter().take(5) {
            output.push_str(&format!(
                "  Line {}: {} ({})\n",
                bug.line, bug.title, bug.bug_type
            ));
            output.push_str(&format!("    → {}\n", bug.description));
            output.push_str(&format!("    → {}\n\n", bug.code.trim()));
        }
        if low_severity.len() > 5 {
            output.push_str(&format!("  ... and {} more\n\n", low_severity.len() - 5));
        }
    }

    output.push_str("💡 Recommendations:\n");
    output.push_str("  - Address high-severity bugs first\n");
    output.push_str("  - Run language-specific static analysis tools\n");
    output.push_str("  - Use security-focused tools like bandit (Python), cargo-audit (Rust)\n");

    Ok(output)
}

struct BugReport {
    line: usize,
    bug_type: String,
    severity: String,
    title: String,
    description: String,
    code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_rust_bugs() {
        let code = r#"
fn main() {
    let x: *mut i32 = unsafe { std::mem::zeroed() };
    println!("{}", x);
}
"#;
        let result = find_rust_bugs(code, "all").unwrap();
        assert!(result.contains("Bug Analysis Report"));
        assert!(result.contains("High Severity"));
    }

    #[test]
    fn test_find_python_bugs() {
        let code = r#"
def foo():
    x = exec("print('hello')")
    return x
"#;
        let result = find_python_bugs(code, "all").unwrap();
        assert!(result.contains("Bug Analysis Report"));
        assert!(result.contains("🔴 High Severity"));
    }

    #[test]
    fn test_find_javascript_bugs() {
        let code = r#"
function foo() {
    div.innerHTML = userInput;
}
"#;
        let result = find_javascript_bugs(code, "all").unwrap();
        assert!(result.contains("Bug Analysis Report"));
        assert!(result.contains("High Severity"));
    }
}
