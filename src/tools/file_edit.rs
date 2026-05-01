//! FileEditTool - Exact string replacement editing for files.
//!
//! Performs precise string replacement with validation:
//! - Ensures old_string exists and is unique
//! - Atomic write operations
//! - Clear error messages

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::ToolResult;
use serde_json::Value;
use std::io::Write;
use tempfile::NamedTempFile;

/// Handler for file_edit tool
pub struct FileEditHandler;

impl ToolHandler for FileEditHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let result: Result<ToolResult, FerroError> = async {
                let file_path = arguments
                    .get("file_path")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| FerroError::Tool("Missing 'file_path' argument".into()))?
                    .to_string();

                let old_string = arguments
                    .get("old_string")
                    .and_then(|o| o.as_str())
                    .ok_or_else(|| FerroError::Tool("Missing 'old_string' argument".into()))?
                    .to_string();

                let new_string = arguments
                    .get("new_string")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| FerroError::Tool("Missing 'new_string' argument".into()))?
                    .to_string();

                // Read the file
                let content = tokio::fs::read_to_string(&file_path)
                    .await
                    .map_err(|e| FerroError::Tool(format!("Failed to read file '{file_path}': {e}")))?;

                // Check if old_string exists
                if !content.contains(&old_string) {
                    return Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: format!(
                            "Error: The string '{old_string}' was not found in the file '{file_path}'"
                        ),
                        is_error: true,
                    });
                }

                // Check if old_string is unique
                let matches = content.matches(&old_string).count();
                if matches > 1 {
                    return Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: format!(
                            "Error: The string '{old_string}' appears {} times in '{file_path}'. \
                            For safety, the string to replace must be unique. \
                            Please provide more context to make the string unique.",
                            matches
                        ),
                        is_error: true,
                    });
                }

                // Perform the replacement
                let new_content = content.replacen(&old_string, &new_string, 1);

                // Atomic write using tempfile
                let parent_dir = std::path::Path::new(&file_path)
                    .parent()
                    .ok_or_else(|| {
                        FerroError::Tool(format!(
                            "Cannot determine parent directory for '{file_path}'"
                        ))
                    })?
                    .to_path_buf();

                // Create parent directories if they don't exist
                tokio::fs::create_dir_all(&parent_dir)
                    .await
                    .map_err(|e| FerroError::Tool(format!("Failed to create parent directories: {e}")))?;

                // Use blocking task for tempfile operations (not async)
                let file_path_clone = file_path.clone();
                let new_content_clone = new_content.clone();
                let parent_dir_clone = parent_dir.clone();

                tokio::task::spawn_blocking(move || -> Result<(), FerroError> {
                    // Create a temporary file in the same directory as the target
                    let mut temp_file = NamedTempFile::new_in(&parent_dir_clone)
                        .map_err(|e| FerroError::Tool(format!("Failed to create temporary file: {e}")))?;

                    // Write the new content to the temp file
                    temp_file
                        .write_all(new_content_clone.as_bytes())
                        .map_err(|e| FerroError::Tool(format!("Failed to write to temporary file: {e}")))?;

                    // Persist the temp file to the target location (atomic replace)
                    temp_file
                        .persist(&file_path_clone)
                        .map_err(|e| FerroError::Tool(format!("Failed to save file '{file_path_clone}': {e}")))?;

                    Ok(())
                })
                .await
                .map_err(|e| FerroError::Tool(format!("Task join error: {e}")))??;

                Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!(
                        "Successfully replaced '{old_string}' with '{new_string}' in '{file_path}'"
                    ),
                    is_error: false,
                })
            }
            .await;

            match result {
                Ok(r) => Ok(r),
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: e.to_string(),
                    is_error: true,
                }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_simple_single_line_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World\nGoodbye World").unwrap();

        let handler = FileEditHandler;
        let result = handler
            .call(
                "test-1",
                &json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Hello",
                    "new_string": "Hi"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Successfully replaced"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hi World\nGoodbye World");
    }

    #[tokio::test]
    async fn test_multi_line_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5").unwrap();

        let handler = FileEditHandler;
        let result = handler
            .call(
                "test-2",
                &json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Line 2\nLine 3",
                    "new_string": "REPLACED"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Line 1\nREPLACED\nLine 4\nLine 5");
    }

    #[tokio::test]
    async fn test_multiple_matches_should_error() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World\nHello World\nHello World").unwrap();

        let handler = FileEditHandler;
        let result = handler
            .call(
                "test-3",
                &json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Hello",
                    "new_string": "Hi"
                }),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("appears 3 times"));
        assert!(result.content.contains("must be unique"));

        // Verify file was not modified
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello World\nHello World\nHello World");
    }

    #[tokio::test]
    async fn test_string_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello World").unwrap();

        let handler = FileEditHandler;
        let result = handler
            .call(
                "test-4",
                &json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Goodbye",
                    "new_string": "Farewell"
                }),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("was not found"));

        // Verify file was not modified
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello World");
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let handler = FileEditHandler;
        let result = handler
            .call(
                "test-5",
                &json!({
                    "file_path": "/nonexistent/path/to/file.txt",
                    "old_string": "old",
                    "new_string": "new"
                }),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_missing_required_arguments() {
        let handler = FileEditHandler;

        // Missing file_path
        let result = handler
            .call(
                "test-6",
                &json!({
                    "old_string": "old",
                    "new_string": "new"
                }),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'file_path'"));

        // Missing old_string
        let result = handler
            .call(
                "test-7",
                &json!({
                    "file_path": "/tmp/test.txt",
                    "new_string": "new"
                }),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'old_string'"));

        // Missing new_string
        let result = handler
            .call(
                "test-8",
                &json!({
                    "file_path": "/tmp/test.txt",
                    "old_string": "old"
                }),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'new_string'"));
    }
}
