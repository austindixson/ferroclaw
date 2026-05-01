//! CommitTool - Interactive git commit workflow with conventional commits.
//!
//! Features:
//! - Repository detection and validation
//! - Staged changes analysis
//! - Diff generation and display
//! - Conventional commit message generation
//! - Interactive approval workflow
//! - Commit execution via git2

use crate::error::{FerroError, Result};
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::ToolResult;
use git2::{Repository, Status, StatusOptions};
use regex_lite::Regex;
use serde_json::Value;
use std::path::Path;

/// Commit types following conventional commits specification
const COMMIT_TYPES: &[(&str, &str)] = &[
    ("feat", "A new feature"),
    ("fix", "A bug fix"),
    ("docs", "Documentation only changes"),
    (
        "style",
        "Changes that do not affect the meaning of the code",
    ),
    (
        "refactor",
        "A code change that neither fixes a bug nor adds a feature",
    ),
    ("perf", "A code change that improves performance"),
    ("test", "Adding missing tests or correcting existing tests"),
    (
        "build",
        "Changes that affect the build system or external dependencies",
    ),
    ("ci", "Changes to CI configuration files and scripts"),
    ("chore", "Other changes that don't modify src or test files"),
    ("revert", "Reverts a previous commit"),
];

/// Handler for commit tool
pub struct CommitHandler;

impl ToolHandler for CommitHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let result: Result<ToolResult> = async {
                // Parse arguments
                let yes = arguments
                    .get("yes")
                    .and_then(|y| y.as_bool())
                    .unwrap_or(false);

                let amend = arguments
                    .get("amend")
                    .and_then(|a| a.as_bool())
                    .unwrap_or(false);

                let repo_path = arguments
                    .get("repo_path")
                    .and_then(|p| p.as_str())
                    .unwrap_or(".");

                // Find and open repository
                let repo = find_repository(repo_path)?;

                // Check for staged changes
                let staged_changes = get_staged_changes(&repo)?;

                if staged_changes.is_empty() && !amend {
                    return Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: "No staged changes found. Use 'git add' to stage files first.".to_string(),
                        is_error: true,
                    });
                }

                // Generate diff
                let diff = generate_diff(&repo)?;

                // Get recent commits for style analysis
                let recent_commits = get_recent_commits(&repo, 10)?;

                // Generate commit message
                let commit_message =
                    generate_commit_message(&staged_changes, &diff, &recent_commits)?;

                // Interactive approval (if not auto-approve)
                if !yes {
                    // In a real implementation, this would show the message and diff
                    // and wait for user input. For now, we'll include the diff in the output
                    let preview = format!(
                        "Proposed commit:\n\n{}\n\nFiles changed:\n{}\n\nDiff:\n{}\n\nUse --yes flag to auto-approve.",
                        commit_message,
                        staged_changes.join("\n"),
                        diff
                    );
                    return Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: preview,
                        is_error: false,
                    });
                }

                // Create the commit
                let commit_id = create_commit(&repo, &commit_message, amend)?;

                Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!(
                        "Commit created successfully!\n\nCommit ID: {}\nMessage:\n{}",
                        commit_id, commit_message
                    ),
                    is_error: false,
                })
            }
            .await;

            result
        })
    }
}

/// Find a git repository by searching upward from the given path
fn find_repository(path: &str) -> Result<Repository> {
    let path_obj = Path::new(path);

    // Try to open as a repository
    match Repository::open(path_obj) {
        Ok(repo) => Ok(repo),
        Err(_e) => {
            // If it's not found, try searching upward
            if path_obj.is_absolute() {
                Err(FerroError::Tool(format!(
                    "Not a git repository (or any parent up to mount point): {}",
                    path
                )))
            } else {
                // Get current directory and search from there
                let current_dir = std::env::current_dir().map_err(|e| {
                    FerroError::Tool(format!("Failed to get current directory: {}", e))
                })?;

                Repository::discover(&current_dir).map_err(|_| {
                    FerroError::Tool(format!(
                        "Not a git repository (or any parent up to mount point): {}",
                        current_dir.display()
                    ))
                })
            }
        }
    }
}

/// Get list of staged changes
fn get_staged_changes(repo: &Repository) -> Result<Vec<String>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(false);
    opts.recurse_untracked_dirs(false);

    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| FerroError::Tool(format!("Failed to get git status: {}", e)))?;

    let mut staged_files = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("<unknown>");

        // Check if file is staged in index
        if status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_RENAMED
                | Status::INDEX_TYPECHANGE
                | Status::INDEX_DELETED,
        ) {
            staged_files.push(path.to_string());
        }
    }

    Ok(staged_files)
}

/// Generate diff of staged changes
fn generate_diff(repo: &Repository) -> Result<String> {
    let head = repo
        .head()
        .map_err(|e| FerroError::Tool(format!("Failed to get HEAD: {}", e)))?;

    let head_commit = head
        .peel_to_commit()
        .map_err(|e| FerroError::Tool(format!("Failed to peel HEAD to commit: {}", e)))?;

    let head_tree = head_commit
        .tree()
        .map_err(|e| FerroError::Tool(format!("Failed to get HEAD tree: {}", e)))?;

    let mut index = repo
        .index()
        .map_err(|e| FerroError::Tool(format!("Failed to get index: {}", e)))?;

    let index_tree = index
        .write_tree()
        .map_err(|e| FerroError::Tool(format!("Failed to write index tree: {}", e)))?;

    let index_tree = repo
        .find_tree(index_tree)
        .map_err(|e| FerroError::Tool(format!("Failed to find index tree: {}", e)))?;

    let diff = repo
        .diff_tree_to_tree(Some(&head_tree), Some(&index_tree), None)
        .map_err(|e| FerroError::Tool(format!("Failed to generate diff: {}", e)))?;

    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if origin == ' ' || origin == '+' || origin == '-' || origin == '=' {
            diff_text.push(origin);
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        }
        true
    })
    .map_err(|e| FerroError::Tool(format!("Failed to format diff: {}", e)))?;

    Ok(diff_text)
}

/// Get recent commits for style analysis
fn get_recent_commits(repo: &Repository, count: usize) -> Result<Vec<String>> {
    let head = repo
        .head()
        .map_err(|e| FerroError::Tool(format!("Failed to get HEAD: {}", e)))?;

    let head_commit = head
        .peel_to_commit()
        .map_err(|e| FerroError::Tool(format!("Failed to peel HEAD to commit: {}", e)))?;

    let mut revwalk = repo
        .revwalk()
        .map_err(|e| FerroError::Tool(format!("Failed to create revwalk: {}", e)))?;

    revwalk
        .push(head_commit.id())
        .map_err(|e| FerroError::Tool(format!("Failed to push HEAD to revwalk: {}", e)))?;

    let mut commits = Vec::new();
    for (i, oid) in revwalk.enumerate() {
        if i >= count {
            break;
        }

        let oid = oid.map_err(|e| FerroError::Tool(format!("Failed to get OID: {}", e)))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| FerroError::Tool(format!("Failed to find commit: {}", e)))?;

        if let Some(msg) = commit.message() {
            commits.push(msg.to_string());
        }
    }

    Ok(commits)
}

/// Generate conventional commit message based on changes
fn generate_commit_message(
    staged_changes: &[String],
    diff: &str,
    _recent_commits: &[String],
) -> Result<String> {
    // Analyze the diff to determine commit type
    let commit_type = infer_commit_type(diff);

    // Extract the main change description
    let description = extract_description(diff, staged_changes);

    // Validate conventional commit format
    validate_commit_format(&commit_type, &description)?;

    Ok(format!("{}: {}", commit_type, description))
}

/// Infer commit type from diff content
fn infer_commit_type(diff: &str) -> String {
    let diff_lower = diff.to_lowercase();

    // Check for test changes
    if diff_lower.contains("test") || diff_lower.contains("spec") {
        return "test".to_string();
    }

    // Check for documentation
    if diff_lower.contains("readme") || diff_lower.contains(".md") || diff_lower.contains("doc") {
        return "docs".to_string();
    }

    // Check for CI/build
    if diff_lower.contains(".github")
        || diff_lower.contains(".gitlab-ci")
        || diff_lower.contains("dockerfile")
        || diff_lower.contains("makefile")
    {
        return "ci".to_string();
    }

    // Check for dependencies
    if diff_lower.contains("cargo.toml")
        || diff_lower.contains("package.json")
        || diff_lower.contains("requirements.txt")
    {
        return "build".to_string();
    }

    // Check for performance
    if diff_lower.contains("optimize")
        || diff_lower.contains("performance")
        || diff_lower.contains("speed")
    {
        return "perf".to_string();
    }

    // Check for refactor
    if diff_lower.contains("refactor")
        || diff_lower.contains("extract")
        || diff_lower.contains("simplify")
    {
        return "refactor".to_string();
    }

    // Check for bug fix keywords
    if diff_lower.contains("fix")
        || diff_lower.contains("bug")
        || diff_lower.contains("error")
        || diff_lower.contains("issue")
    {
        return "fix".to_string();
    }

    // Default to feat for new functionality
    "feat".to_string()
}

/// Extract description from diff and changed files
fn extract_description(diff: &str, staged_changes: &[String]) -> String {
    // Look for added functions, structs, or classes
    let re_fn = Regex::new(r"^\+fn\s+(\w+)").unwrap();
    let re_struct = Regex::new(r"^\+pub struct\s+(\w+)").unwrap();
    let re_impl = Regex::new(r"^\+impl\s+(\w+)").unwrap();

    for line in diff.lines() {
        if let Some(caps) = re_fn.captures(line) {
            return format!("Add {} function", caps.get(1).unwrap().as_str());
        }
        if let Some(caps) = re_struct.captures(line) {
            return format!("Add {} struct", caps.get(1).unwrap().as_str());
        }
        if let Some(caps) = re_impl.captures(line) {
            return format!("Implement {} trait", caps.get(1).unwrap().as_str());
        }
    }

    // If no specific pattern found, use file names
    if !staged_changes.is_empty() {
        let file = &staged_changes[0];
        if file.contains("test") {
            return format!(
                "Add tests for {}",
                file.replace("_test.rs", "").replace(".rs", "")
            );
        }
        return format!("Update {}", file);
    }

    "Update codebase".to_string()
}

/// Validate commit message format
fn validate_commit_format(commit_type: &str, description: &str) -> Result<()> {
    // Check if commit type is valid
    let valid_type = COMMIT_TYPES.iter().any(|(t, _)| *t == commit_type);

    if !valid_type {
        return Err(FerroError::Tool(format!(
            "Invalid commit type '{}'. Valid types: {}",
            commit_type,
            COMMIT_TYPES
                .iter()
                .map(|(t, _)| *t)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    // Check description length
    if description.is_empty() {
        return Err(FerroError::Tool(
            "Commit description cannot be empty".to_string(),
        ));
    }

    if description.len() > 72 {
        return Err(FerroError::Tool(
            "Commit description too long (max 72 characters)".to_string(),
        ));
    }

    Ok(())
}

/// Create a commit with the given message
fn create_commit(repo: &Repository, message: &str, _amend: bool) -> Result<String> {
    let signature = repo
        .signature()
        .or_else(|_| git2::Signature::now("Ferroclaw", "ferroclaw@example.com"))
        .map_err(|e| FerroError::Tool(format!("Failed to create signature: {}", e)))?;

    let mut index = repo
        .index()
        .map_err(|e| FerroError::Tool(format!("Failed to get index: {}", e)))?;

    let tree_id = index
        .write_tree()
        .map_err(|e| FerroError::Tool(format!("Failed to write tree: {}", e)))?;

    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| FerroError::Tool(format!("Failed to find tree: {}", e)))?;

    let head_commit = if _amend {
        // Get current HEAD to amend
        let head = repo
            .head()
            .map_err(|e| FerroError::Tool(format!("Failed to get HEAD for amend: {}", e)))?;

        Some(
            head.peel_to_commit()
                .map_err(|e| FerroError::Tool(format!("Failed to peel HEAD to commit: {}", e)))?,
        )
    } else {
        // Get parent commit
        let head = repo
            .head()
            .map_err(|e| FerroError::Tool(format!("Failed to get HEAD: {}", e)))?;

        Some(
            head.peel_to_commit()
                .map_err(|e| FerroError::Tool(format!("Failed to peel HEAD to commit: {}", e)))?,
        )
    };

    let parents = match &head_commit {
        Some(commit) => vec![commit],
        None => vec![],
    };

    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .map_err(|e| FerroError::Tool(format!("Failed to create commit: {}", e)))?;

    Ok(oid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_type_inference() {
        // Test feature inference
        let feat_diff = "+fn new_feature() {\n+    println!(\"new\");\n+}";
        assert_eq!(infer_commit_type(feat_diff), "feat");

        // Test fix inference
        let fix_diff = "-fn broken() {\n+fn fixed() {\n+    // fix bug\n}";
        assert_eq!(infer_commit_type(fix_diff), "fix");

        // Test test inference
        let test_diff = "+#[test]\n+fn test_something() {\n+}";
        assert_eq!(infer_commit_type(test_diff), "test");

        // Test docs inference
        let docs_diff = "+# Documentation\n+New docs here";
        assert_eq!(infer_commit_type(docs_diff), "docs");

        // Test refactor inference
        let refactor_diff = "+fn refactored() {\n+    // simplified\n}";
        assert_eq!(infer_commit_type(refactor_diff), "refactor");
    }

    #[test]
    fn test_description_extraction() {
        // Test function extraction
        let fn_diff = "+fn my_function() -> Result<()> {\n+    Ok(())\n+}";
        assert_eq!(
            extract_description(fn_diff, &[]),
            "Add my_function function"
        );

        // Test struct extraction
        let struct_diff = "+pub struct MyStruct {\n+    field: String,\n+}";
        assert_eq!(extract_description(struct_diff, &[]), "Add MyStruct struct");

        // Test fallback to file name
        assert_eq!(
            extract_description("", &["src/main.rs".to_string()]),
            "Update src/main.rs"
        );
    }

    #[test]
    fn test_commit_format_validation() {
        // Valid commit
        assert!(validate_commit_format("feat", "Add new feature").is_ok());

        // Invalid type
        assert!(validate_commit_format("invalid", "description").is_err());

        // Empty description
        assert!(validate_commit_format("feat", "").is_err());

        // Too long description
        let long_desc = "a".repeat(100);
        assert!(validate_commit_format("feat", &long_desc).is_err());
    }
}
