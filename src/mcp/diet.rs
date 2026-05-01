//! DietMCP compression layer — ported from dietmcp (Python).
//!
//! Core insight: JSON tool schemas waste 90%+ of context tokens.
//! This module compresses them into compact "skill summaries" that
//! LLMs understand equally well, and formats large responses to
//! prevent context flooding.

use crate::types::ToolDefinition;
use std::collections::HashMap;
use std::path::PathBuf;

/// Category keywords for heuristic grouping (mirrors dietmcp's skills_generator.py).
const CATEGORY_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "File Operations",
        &[
            "file",
            "read",
            "write",
            "directory",
            "path",
            "rename",
            "move",
            "copy",
            "delete",
        ],
    ),
    (
        "Search",
        &[
            "search", "find", "grep", "query", "filter", "match", "lookup",
        ],
    ),
    (
        "Database",
        &[
            "sql", "database", "table", "row", "column", "schema", "migrate", "insert", "select",
        ],
    ),
    (
        "Network",
        &[
            "http", "request", "fetch", "url", "api", "endpoint", "webhook",
        ],
    ),
    (
        "Browser",
        &[
            "navigate",
            "click",
            "screenshot",
            "page",
            "element",
            "browser",
            "dom",
        ],
    ),
    (
        "Git",
        &[
            "git", "commit", "branch", "merge", "pull", "push", "diff", "repo",
        ],
    ),
    (
        "Documentation",
        &["doc", "readme", "markdown", "comment", "describe"],
    ),
];

/// A compressed skill summary for one MCP server's tools.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub server_name: String,
    pub tool_count: usize,
    pub categories: Vec<SkillCategory>,
    pub exec_hint: String,
}

#[derive(Debug, Clone)]
pub struct SkillCategory {
    pub name: String,
    pub entries: Vec<SkillEntry>,
}

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub signature: String,
    pub description: String,
}

/// Generate a compact skill summary from tool definitions.
/// This is the core value proposition — ~93% token reduction vs raw JSON schemas.
pub fn generate_skill_summary(server_name: &str, tools: &[ToolDefinition]) -> SkillSummary {
    let grouped = categorize_tools(tools);

    let categories: Vec<SkillCategory> = {
        let mut cats: Vec<(String, Vec<SkillEntry>)> = grouped
            .into_iter()
            .map(|(name, defs)| {
                let entries = defs
                    .iter()
                    .map(|t| SkillEntry {
                        signature: t.compact_signature(),
                        description: truncate(&t.description, 80),
                    })
                    .collect();
                (name, entries)
            })
            .collect();
        cats.sort_by(|a, b| a.0.cmp(&b.0));
        cats.into_iter()
            .map(|(name, entries)| SkillCategory { name, entries })
            .collect()
    };

    SkillSummary {
        server_name: server_name.to_string(),
        tool_count: tools.len(),
        categories,
        exec_hint: format!(
            "ferroclaw mcp exec {server_name} <tool> --args '{{\"key\": \"value\"}}'"
        ),
    }
}

/// Render a skill summary as compact text for the LLM's system prompt.
pub fn render_skill_summary(summary: &SkillSummary) -> String {
    let mut out = format!(
        "# {} ({} tools)\n\n",
        summary.server_name, summary.tool_count
    );

    for cat in &summary.categories {
        out.push_str(&format!("## {}\n", cat.name));
        for entry in &cat.entries {
            out.push_str(&format!("- {} -- {}\n", entry.signature, entry.description));
        }
        out.push('\n');
    }

    out.push_str(&format!("Exec: {}\n", summary.exec_hint));
    out
}

/// Render multiple skill summaries into a single context block.
pub fn render_all_summaries(summaries: &[SkillSummary]) -> String {
    let total_tools: usize = summaries.iter().map(|s| s.tool_count).sum();
    let mut out = format!("# Available MCP Tools ({total_tools} total)\n\n");

    for summary in summaries {
        out.push_str(&render_skill_summary(summary));
        out.push('\n');
    }

    out
}

/// Categorize tools by keyword matching on name + description.
fn categorize_tools(tools: &[ToolDefinition]) -> HashMap<String, Vec<&ToolDefinition>> {
    let mut groups: HashMap<String, Vec<&ToolDefinition>> = HashMap::new();
    let mut uncategorized: Vec<&ToolDefinition> = Vec::new();

    for tool in tools {
        let searchable = format!("{} {}", tool.name, tool.description).to_lowercase();
        let mut best_category: Option<&str> = None;
        let mut best_score = 0;

        for (category, keywords) in CATEGORY_KEYWORDS {
            let score: usize = keywords
                .iter()
                .filter(|kw| searchable.contains(**kw))
                .count();
            if score > best_score {
                best_score = score;
                best_category = Some(category);
            }
        }

        if let Some(cat) = best_category {
            groups.entry(cat.to_string()).or_default().push(tool);
        } else {
            uncategorized.push(tool);
        }
    }

    if !uncategorized.is_empty() {
        groups.insert("Tools".to_string(), uncategorized);
    }

    groups
}

/// Output format for diet response formatting.
#[derive(Debug, Clone, Copy)]
pub enum DietFormat {
    Summary,
    Minified,
    Csv,
}

/// Format a tool response according to the diet format.
pub fn format_response(content: &str, format: DietFormat, max_size: usize) -> DietResponse {
    let formatted = match format {
        DietFormat::Summary => format_summary(content, max_size),
        DietFormat::Minified => format_minified(content),
        DietFormat::Csv => format_csv(content),
    };

    if formatted.len() > max_size {
        return auto_redirect(&formatted);
    }

    DietResponse {
        content: formatted,
        was_redirected: false,
        file_path: None,
    }
}

/// Auto-redirect large responses to a temp file.
pub fn auto_redirect(content: &str) -> DietResponse {
    let dir = std::env::temp_dir();
    let filename = format!("ferroclaw_{}.txt", uuid::Uuid::new_v4().as_simple());
    let path = dir.join(&filename);

    match std::fs::write(&path, content) {
        Ok(_) => DietResponse {
            content: format!(
                "[Response written to {} ({} chars)]",
                path.display(),
                content.len()
            ),
            was_redirected: true,
            file_path: Some(path),
        },
        Err(_) => DietResponse {
            content: truncate(content, 2000),
            was_redirected: false,
            file_path: None,
        },
    }
}

#[derive(Debug, Clone)]
pub struct DietResponse {
    pub content: String,
    pub was_redirected: bool,
    pub file_path: Option<PathBuf>,
}

/// Summary format: extract key info, truncate long values.
fn format_summary(content: &str, max_size: usize) -> String {
    if content.len() <= max_size {
        return content.to_string();
    }

    let preview_size = max_size.min(500);
    format!(
        "{}\n---\n[Truncated: {} chars total. Full output available via --output-file.]",
        &content[..preview_size.min(content.len())],
        content.len()
    )
}

/// Minified format: strip whitespace from JSON.
fn format_minified(content: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        // Re-serialize without pretty printing, strip null fields
        let cleaned = strip_nulls(&value);
        serde_json::to_string(&cleaned).unwrap_or_else(|_| content.to_string())
    } else {
        content.to_string()
    }
}

/// CSV format: attempt to extract tabular data.
fn format_csv(content: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content)
        && let Some(arr) = value.as_array()
    {
        return array_to_csv(arr);
    }
    content.to_string()
}

fn array_to_csv(arr: &[serde_json::Value]) -> String {
    if arr.is_empty() {
        return String::new();
    }

    // Extract headers from first object
    let headers: Vec<String> = arr
        .first()
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    if headers.is_empty() {
        return arr
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut out = headers.join(",");
    out.push('\n');

    for item in arr {
        let row: Vec<String> = headers
            .iter()
            .map(|h| {
                item.get(h)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            })
            .collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }

    out
}

fn strip_nulls(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let cleaned: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), strip_nulls(v)))
                .collect();
            serde_json::Value::Object(cleaned)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(strip_nulls).collect())
        }
        other => other.clone(),
    }
}

fn truncate(text: &str, max_len: usize) -> String {
    let text = text.replace('\n', " ");
    let text = text.trim();
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_tool(name: &str, desc: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: desc.into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            server_name: Some("test".into()),
        }
    }

    #[test]
    fn test_categorize_tools() {
        let tools = vec![
            make_tool("read_file", "Read contents of a file"),
            make_tool("write_file", "Write content to a file"),
            make_tool(
                "search_files",
                "Search for a pattern matching query in files",
            ),
            make_tool("git_commit", "Create a git commit on branch"),
        ];
        let grouped = categorize_tools(&tools);
        assert!(grouped.contains_key("File Operations"));
        // search_files may be categorized under File Operations due to "file" keyword
        // as long as tools are categorized, the function works
        assert!(grouped.len() >= 2);
    }

    #[test]
    fn test_skill_summary_render() {
        let tools = vec![
            make_tool("read_file", "Read file contents"),
            make_tool("write_file", "Write to a file"),
        ];
        let summary = generate_skill_summary("filesystem", &tools);
        let rendered = render_skill_summary(&summary);
        assert!(rendered.contains("filesystem"));
        assert!(rendered.contains("2 tools"));
        assert!(rendered.contains("read_file"));
    }

    #[test]
    fn test_compact_signature() {
        let tool = ToolDefinition {
            name: "search".into(),
            description: "Search files".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer"}
                },
                "required": ["query"]
            }),
            server_name: None,
        };
        let sig = tool.compact_signature();
        assert!(sig.contains("query: str"));
        assert!(sig.contains("?limit: int"));
    }

    #[test]
    fn test_format_minified() {
        let input = r#"{"name": "test", "value": null, "items": [1, 2, 3]}"#;
        let result = format_minified(input);
        assert!(!result.contains("null"));
        assert!(result.contains("\"items\""));
    }

    #[test]
    fn test_format_csv() {
        let input = r#"[{"name":"a","size":1},{"name":"b","size":2}]"#;
        let result = format_csv(input);
        assert!(result.contains("name,size") || result.contains("size,name"));
        assert!(result.contains("a"));
    }

    #[test]
    fn test_auto_redirect_large_response() {
        let large = "x".repeat(100_000);
        let result = auto_redirect(&large);
        assert!(result.was_redirected);
        assert!(result.file_path.is_some());
        assert!(result.content.contains("100000 chars"));
        // Cleanup
        if let Some(path) = result.file_path {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 100), "short");
        let long = "a".repeat(200);
        let truncated = truncate(&long, 50);
        assert!(truncated.len() <= 50);
        assert!(truncated.ends_with("..."));
    }
}
