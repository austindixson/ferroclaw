//! GrepTool - Content search with regex patterns
//!
//! Provides powerful content search capabilities:
//! - Regex pattern matching
//! - Glob file filtering
//! - Multiple output modes (content, files_with_matches, count)
//! - Context lines (-A/-B/-C flags)
//! - Line numbers in content mode
//! - Case-insensitive search (-i flag)

use crate::error::{FerroError, Result};
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::{ToolDefinition, ToolMeta, ToolResult, ToolSource};
use regex_lite::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Maximum number of results to return by default
const DEFAULT_LIMIT: usize = 250;

/// Output mode for grep results
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputMode {
    Content,
    FilesWithMatches,
    Count,
}

impl OutputMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "content" => Some(OutputMode::Content),
            "files_with_matches" => Some(OutputMode::FilesWithMatches),
            "count" => Some(OutputMode::Count),
            _ => None,
        }
    }
}

/// GrepTool handler for content search
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    /// Execute grep search
    async fn execute_grep(&self, pattern: &str, args: &GrepArgs) -> Result<String> {
        // Build regex
        let regex = self.build_regex(pattern, args.case_insensitive)?;

        // Determine search path
        let search_path = PathBuf::from(&args.path);
        let search_targets = if search_path.exists() {
            if search_path.is_file() {
                vec![search_path]
            } else {
                // It's a directory, find matching files
                self.find_files(&search_path, args)
            }
        } else {
            return Err(FerroError::Tool(format!(
                "Path does not exist: {}",
                args.path
            )));
        };

        // Execute search based on output mode
        match args.output_mode {
            OutputMode::Content => self.search_content(&regex, &search_targets, args).await,
            OutputMode::FilesWithMatches => self.search_files(&regex, &search_targets, args).await,
            OutputMode::Count => self.search_count(&regex, &search_targets, args).await,
        }
    }

    /// Build regex from pattern
    fn build_regex(&self, pattern: &str, case_insensitive: bool) -> Result<Regex> {
        RegexBuilder::new(pattern)
            .case_insensitive(case_insensitive)
            .dot_matches_new_line(false) // By default, . doesn't match newlines
            .build()
            .map_err(|e| FerroError::Tool(format!("Invalid regex pattern: {e}")))
    }

    /// Find files matching glob pattern
    fn find_files(&self, base_path: &Path, args: &GrepArgs) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self.find_files_recursive_sync(base_path, args, &mut files);
        files
    }

    /// Recursively find files
    fn find_files_recursive_sync(
        &self,
        current_dir: &Path,
        args: &GrepArgs,
        files: &mut Vec<PathBuf>,
    ) {
        let mut entries = match std::fs::read_dir(current_dir) {
            Ok(e) => e,
            Err(_) => return, // Skip directories we can't read
        };

        while let Some(Ok(entry)) = entries.next() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden directories and common VCS directories
            if path.is_dir() {
                if file_name.starts_with('.') {
                    continue;
                }
                if matches!(file_name.as_str(), "node_modules" | "target" | ".git") {
                    continue;
                }
                self.find_files_recursive_sync(&path, args, files);
                continue;
            }

            // Check if file matches glob pattern
            if let Some(glob_pattern) = &args.glob
                && !self.glob_matches(glob_pattern, &file_name)
            {
                continue;
            }

            files.push(path);
        }
    }

    /// Check if filename matches glob pattern
    fn glob_matches(&self, pattern: &str, filename: &str) -> bool {
        // Simple glob matching (supports * and ?)
        let pattern_chars: Vec<char> = pattern.chars().collect();
        let filename_chars: Vec<char> = filename.chars().collect();
        let mut p_idx = 0;
        let mut f_idx = 0;
        let mut star_idx = -1i32;
        let mut match_idx = 0i32;

        while f_idx < filename_chars.len() {
            if p_idx < pattern_chars.len()
                && (pattern_chars[p_idx] == filename_chars[f_idx] || pattern_chars[p_idx] == '?')
            {
                p_idx += 1;
                f_idx += 1;
            } else if p_idx < pattern_chars.len() && pattern_chars[p_idx] == '*' {
                star_idx = p_idx as i32;
                match_idx = f_idx as i32;
                p_idx += 1;
            } else if star_idx != -1 {
                p_idx = (star_idx + 1) as usize;
                match_idx += 1;
                f_idx = match_idx as usize;
            } else {
                return false;
            }
        }

        while p_idx < pattern_chars.len() && pattern_chars[p_idx] == '*' {
            p_idx += 1;
        }

        p_idx == pattern_chars.len()
    }

    /// Search with content output mode
    async fn search_content(
        &self,
        regex: &Regex,
        files: &[PathBuf],
        args: &GrepArgs,
    ) -> Result<String> {
        let mut results = Vec::new();
        let mut total_lines = 0;

        for file_path in files {
            if total_lines >= args.head_limit {
                break;
            }

            let content = fs::read_to_string(file_path).await?;
            let lines: Vec<&str> = content.lines().collect();

            for (line_num, line) in lines.iter().enumerate() {
                if regex.is_match(line) {
                    let line_num_1based = line_num + 1;

                    // Add context before
                    if args.context_before > 0 {
                        let start = line_num.saturating_sub(args.context_before);
                        for ctx_line_num in start..line_num {
                            if let Some(ctx_line) = lines.get(ctx_line_num) {
                                results.push(format!(
                                    "{}-{}",
                                    file_path.display(),
                                    ctx_line_num + 1
                                ));
                                results.push(ctx_line.to_string());
                                total_lines += 1;
                            }
                        }
                    }

                    // Add matching line
                    if args.show_line_numbers {
                        results.push(format!("{}:{}", file_path.display(), line_num_1based));
                    } else {
                        results.push(format!("{}-", file_path.display()));
                    }
                    results.push(line.to_string());
                    total_lines += 1;

                    // Add context after
                    if args.context_after > 0 {
                        let end = (line_num + 1 + args.context_after).min(lines.len());
                        for ctx_line_num in (line_num + 1)..end {
                            if let Some(ctx_line) = lines.get(ctx_line_num) {
                                results.push(format!(
                                    "{}-{}",
                                    file_path.display(),
                                    ctx_line_num + 1
                                ));
                                results.push(ctx_line.to_string());
                                total_lines += 1;
                            }
                        }
                    }

                    if total_lines >= args.head_limit {
                        break;
                    }
                }
            }
        }

        if results.is_empty() {
            Ok("No matches found".to_string())
        } else {
            // Apply offset
            let offset = args.offset.min(results.len());
            let results = &results[offset..];

            // Apply head_limit
            let limit = args.head_limit.min(results.len());
            let results = &results[..limit];

            let truncated = results.len() < (total_lines - args.offset);
            let mut output = results.join("\n");

            if truncated {
                output.push_str("\n\n(Results truncated)");
            }

            Ok(output)
        }
    }

    /// Search with files_with_matches output mode
    async fn search_files(
        &self,
        regex: &Regex,
        files: &[PathBuf],
        args: &GrepArgs,
    ) -> Result<String> {
        let mut matching_files = Vec::new();

        for file_path in files {
            let content = fs::read_to_string(file_path).await?;
            if regex.is_match(&content) {
                matching_files.push(file_path.display().to_string());
            }
        }

        if matching_files.is_empty() {
            Ok("No matches found".to_string())
        } else {
            // Apply offset
            let offset = args.offset.min(matching_files.len());
            let matching_files = &matching_files[offset..];

            // Apply head_limit
            let limit = args.head_limit.min(matching_files.len());
            let matching_files = &matching_files[..limit];

            let truncated = matching_files.len() < (files.len() - args.offset);
            let mut output = matching_files.join("\n");

            if truncated {
                output.push_str("\n\n(Results truncated)");
            }

            Ok(output)
        }
    }

    /// Search with count output mode
    async fn search_count(
        &self,
        regex: &Regex,
        files: &[PathBuf],
        args: &GrepArgs,
    ) -> Result<String> {
        let mut counts = Vec::new();

        for file_path in files {
            let content = fs::read_to_string(file_path).await?;
            let count = regex.find_iter(&content).count();
            if count > 0 {
                counts.push(format!("{}: {}", file_path.display(), count));
            }
        }

        if counts.is_empty() {
            Ok("No matches found".to_string())
        } else {
            // Apply offset
            let offset = args.offset.min(counts.len());
            let counts = &counts[offset..];

            // Apply head_limit
            let limit = args.head_limit.min(counts.len());
            let counts = &counts[..limit];

            let truncated = counts.len() < (files.len() - args.offset);
            let mut output = counts.join("\n");

            if truncated {
                output.push_str("\n\n(Results truncated)");
            }

            Ok(output)
        }
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Arguments for grep search
struct GrepArgs {
    path: String,
    glob: Option<String>,
    output_mode: OutputMode,
    context_before: usize,
    context_after: usize,
    show_line_numbers: bool,
    case_insensitive: bool,
    head_limit: usize,
    offset: usize,
}

impl GrepArgs {
    fn from_value(value: &Value) -> Result<Self> {
        let path = value
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or(".")
            .to_string();

        let glob = value.get("glob").and_then(|g| g.as_str()).map(String::from);

        let output_mode = value
            .get("output_mode")
            .and_then(|m| m.as_str())
            .and_then(OutputMode::from_str)
            .unwrap_or(OutputMode::FilesWithMatches);

        let context_before = value
            .get("-B")
            .or(value.get("context"))
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as usize;

        let context_after = value
            .get("-A")
            .or(value.get("context"))
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as usize;

        // -C takes precedence over -A/-B
        let (context_before, context_after) = if let Some(c) = value.get("-C") {
            let c = c.as_u64().unwrap_or(0) as usize;
            (c, c)
        } else {
            (context_before, context_after)
        };

        let show_line_numbers = value.get("-n").and_then(|n| n.as_bool()).unwrap_or(true);

        let case_insensitive = value.get("-i").and_then(|i| i.as_bool()).unwrap_or(false);

        let head_limit = value
            .get("head_limit")
            .and_then(|l| l.as_u64())
            .unwrap_or(DEFAULT_LIMIT as u64) as usize;

        let offset = value.get("offset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;

        Ok(GrepArgs {
            path,
            glob,
            output_mode,
            context_before,
            context_after,
            show_line_numbers,
            case_insensitive,
            head_limit,
            offset,
        })
    }
}

impl ToolHandler for GrepTool {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let pattern = arguments
                .get("pattern")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'pattern' argument".into()))?;

            let args = GrepArgs::from_value(arguments)?;

            let result = self.execute_grep(pattern, &args).await?;

            Ok(ToolResult {
                call_id: call_id.to_string(),
                content: result,
                is_error: false,
            })
        })
    }
}

/// Create the GrepTool metadata for registration
pub fn grep_tool_meta() -> ToolMeta {
    ToolMeta {
        definition: ToolDefinition {
            name: "grep".into(),
            description: "Search for regex patterns in file contents. Supports multiple output modes, context lines, and file filtering.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The regular expression pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in (default: current directory)"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Glob pattern to filter files (e.g., '*.rs', '*.js')"
                    },
                    "output_mode": {
                        "type": "string",
                        "enum": ["content", "files_with_matches", "count"],
                        "description": "Output mode: 'content' shows matching lines, 'files_with_matches' shows file paths, 'count' shows match counts"
                    },
                    "-B": {
                        "type": "number",
                        "description": "Number of lines to show before each match"
                    },
                    "-A": {
                        "type": "number",
                        "description": "Number of lines to show after each match"
                    },
                    "-C": {
                        "type": "number",
                        "description": "Number of lines to show before and after each match"
                    },
                    "-n": {
                        "type": "boolean",
                        "description": "Show line numbers in output (default: true)"
                    },
                    "-i": {
                        "type": "boolean",
                        "description": "Case insensitive search"
                    },
                    "head_limit": {
                        "type": "number",
                        "description": "Limit output to first N results (default: 250)"
                    },
                    "offset": {
                        "type": "number",
                        "description": "Skip first N results (default: 0)"
                    }
                },
                "required": ["pattern"]
            }),
            server_name: None,
        },
        required_capabilities: vec![],
        source: ToolSource::Builtin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_basic_pattern_matching() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World\nGoodbye World\nHello Rust").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-1",
                &json!({
                    "pattern": "Hello",
                    "path": file_path.to_str().unwrap(),
                    "output_mode": "content"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_files_with_matches_mode() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "Hello World").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "Goodbye World").unwrap();
        fs::write(temp_dir.path().join("file3.txt"), "Hello Rust").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-2",
                &json!({
                    "pattern": "Hello",
                    "path": temp_dir.path().to_str().unwrap(),
                    "output_mode": "files_with_matches"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("file1.txt"));
        assert!(result.content.contains("file3.txt"));
        assert!(!result.content.contains("file2.txt"));
    }

    #[tokio::test]
    async fn test_count_mode() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(
            &file_path,
            "Hello World\nHello Rust\nHello Test\nGoodbye World",
        )
        .unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-3",
                &json!({
                    "pattern": "Hello",
                    "path": file_path.to_str().unwrap(),
                    "output_mode": "count"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains(": 3"));
    }

    #[tokio::test]
    async fn test_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "HELLO\nhello\nHello\nHeLLo").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-4",
                &json!({
                    "pattern": "hello",
                    "path": file_path.to_str().unwrap(),
                    "output_mode": "content",
                    "-i": true
                }),
            )
            .await
            .expect("Tool call should succeed");

        assert!(!result.is_error);
        // All lines should be present in output
        assert!(
            result.content.contains("HELLO")
                || result.content.contains("hello")
                || result.content.contains("Hello")
        );
        // Count lines with file path prefix
        let line_count = result
            .content
            .lines()
            .filter(|l| {
                l.contains("test.txt")
                    || l.contains("HELLO")
                    || l.contains("hello")
                    || l.contains("Hello")
            })
            .count();
        assert!(
            line_count >= 4,
            "Expected at least 4 lines in output, got: {}",
            line_count
        );
    }

    #[tokio::test]
    async fn test_glob_filtering() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.rs"), "Hello Rust").unwrap();
        fs::write(temp_dir.path().join("test.txt"), "Hello World").unwrap();
        fs::write(temp_dir.path().join("test.js"), "Hello JavaScript").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-5",
                &json!({
                    "pattern": "Hello",
                    "path": temp_dir.path().to_str().unwrap(),
                    "glob": "*.rs",
                    "output_mode": "files_with_matches"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("test.rs"));
        assert!(!result.content.contains("test.txt"));
        assert!(!result.content.contains("test.js"));
    }

    #[tokio::test]
    async fn test_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-6",
                &json!({
                    "pattern": "Goodbye",
                    "path": file_path.to_str().unwrap(),
                    "output_mode": "content"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_missing_pattern_argument() {
        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-7",
                &json!({
                    "path": "/tmp/test.txt"
                }),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Missing 'pattern'"));
    }

    #[tokio::test]
    async fn test_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World").unwrap();

        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-8",
                &json!({
                    "pattern": "[invalid(",
                    "path": file_path.to_str().unwrap(),
                    "output_mode": "content"
                }),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let tool = GrepTool::new();
        let result = tool
            .call(
                "test-9",
                &json!({
                    "pattern": "test",
                    "path": "/nonexistent/path/to/file.txt"
                }),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_glob_matches() {
        let tool = GrepTool::new();

        assert!(tool.glob_matches("*.rs", "test.rs"));
        assert!(tool.glob_matches("*.rs", "main.rs"));
        assert!(!tool.glob_matches("*.rs", "test.txt"));
        assert!(tool.glob_matches("*", "test.rs"));
        assert!(tool.glob_matches("test*", "test.rs"));
        assert!(tool.glob_matches("*test", "mytest"));
        assert!(tool.glob_matches("*.txt", "file.txt"));
    }

    #[test]
    fn test_output_mode_from_str() {
        assert_eq!(OutputMode::from_str("content"), Some(OutputMode::Content));
        assert_eq!(
            OutputMode::from_str("files_with_matches"),
            Some(OutputMode::FilesWithMatches)
        );
        assert_eq!(OutputMode::from_str("count"), Some(OutputMode::Count));
        assert_eq!(OutputMode::from_str("invalid"), None);
    }
}
