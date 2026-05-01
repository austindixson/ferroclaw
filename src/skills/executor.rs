//! Skill execution — bash command interpolation and dispatch.

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::ToolResult;
use serde_json::Value;
use std::collections::HashMap;

/// Executes bash-type skills by interpolating arguments into a command template.
///
/// Template syntax:
/// - `{{name}}` — required parameter, error if missing
/// - `{{?name}}` — optional parameter, replaced with empty string if missing
pub struct BashSkillHandler {
    command_template: String,
}

impl BashSkillHandler {
    pub fn new(command_template: String) -> Self {
        Self { command_template }
    }

    pub fn interpolate(&self, arguments: &Value) -> std::result::Result<String, String> {
        let args: HashMap<String, String> = match arguments.as_object() {
            Some(obj) => obj
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), val)
                })
                .collect(),
            None => HashMap::new(),
        };

        let mut result = self.command_template.clone();

        // Replace optional params first ({{?name}})
        let optional_re = regex_lite::Regex::new(r"\{\{\?(\w+)\}\}").unwrap();
        result = optional_re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                let name = &caps[1];
                args.get(name).cloned().unwrap_or_default()
            })
            .to_string();

        // Replace required params ({{name}})
        let required_re = regex_lite::Regex::new(r"\{\{(\w+)\}\}").unwrap();
        let mut missing = Vec::new();
        result = required_re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                let name = &caps[1];
                match args.get(name) {
                    Some(val) => val.clone(),
                    None => {
                        missing.push(name.to_string());
                        String::new()
                    }
                }
            })
            .to_string();

        if !missing.is_empty() {
            return Err(format!(
                "Missing required parameters: {}",
                missing.join(", ")
            ));
        }

        // Collapse multiple spaces from empty optional params
        let multi_space = regex_lite::Regex::new(r"  +").unwrap();
        result = multi_space.replace_all(&result, " ").trim().to_string();

        Ok(result)
    }
}

impl ToolHandler for BashSkillHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let command = self.interpolate(arguments).map_err(FerroError::Tool)?;

            let output = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&command)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Failed to execute: {e}")))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let content = if output.status.success() {
                if stdout.is_empty() {
                    "(no output)".to_string()
                } else {
                    stdout.to_string()
                }
            } else {
                format!(
                    "Exit code: {}\nStdout: {stdout}\nStderr: {stderr}",
                    output.status.code().unwrap_or(-1)
                )
            };

            // Truncate large outputs
            let content = if content.len() > 50_000 {
                format!(
                    "{}...\n[Truncated: {} total chars]",
                    &content[..50_000],
                    content.len()
                )
            } else {
                content
            };

            Ok(ToolResult {
                call_id: call_id.to_string(),
                content,
                is_error: !output.status.success(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_required() {
        let handler = BashSkillHandler::new("echo {{message}}".into());
        let args = serde_json::json!({"message": "hello"});
        let result = handler.interpolate(&args).unwrap();
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_interpolate_optional_present() {
        let handler = BashSkillHandler::new("git log {{?flags}} --oneline".into());
        let args = serde_json::json!({"flags": "-n 5"});
        let result = handler.interpolate(&args).unwrap();
        assert_eq!(result, "git log -n 5 --oneline");
    }

    #[test]
    fn test_interpolate_optional_missing() {
        let handler = BashSkillHandler::new("git log {{?flags}} --oneline".into());
        let args = serde_json::json!({});
        let result = handler.interpolate(&args).unwrap();
        assert_eq!(result, "git log --oneline");
    }

    #[test]
    fn test_interpolate_missing_required() {
        let handler = BashSkillHandler::new("echo {{message}}".into());
        let args = serde_json::json!({});
        let result = handler.interpolate(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("message"));
    }

    #[test]
    fn test_interpolate_multiple_params() {
        let handler = BashSkillHandler::new("find {{path}} -name '{{pattern}}' {{?extra}}".into());
        let args = serde_json::json!({"path": "/tmp", "pattern": "*.rs"});
        let result = handler.interpolate(&args).unwrap();
        assert_eq!(result, "find /tmp -name '*.rs'");
    }
}
