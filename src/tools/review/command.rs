//! Code review command with diff analysis and quality scoring
//!
//! Provides comprehensive code review functionality including:
//! - Diff analysis using git2
//! - Quality scoring (complexity, readability, testing, documentation)
//! - Issue detection and categorization
//! - Review report generation

use crate::error::{FerroError, Result};
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::ToolResult;
use git2::{Repository, DiffOptions, StatusOptions, Status};
use regex_lite::Regex;
use serde_json::Value;
use std::path::Path;

pub use super::reporter::{JsonReportGenerator, ReviewReport, ReviewSummary, TextReportGenerator};

pub use super::diff_parser::{DiffHunk, DiffParser};
pub use super::issue_detector::{Issue, IssueCategory, IssueDetector, Severity};
pub use super::quality_analyzer::{QualityAnalyzer, QualityScore};

/// Scope of code review
#[derive(Debug, Clone)]
pub enum ReviewScope {
    /// Review staged changes (index vs HEAD)
    Staged,
    /// Review working tree (working dir vs index)
    WorkingTree,
    /// Review commit range (e.g., "main..HEAD")
    CommitRange(String),
    /// Review all changes (staged + working tree)
    All,
}

/// Wrapper handler that parses arguments from tool calls
pub struct ReviewToolHandler;

impl ToolHandler for ReviewToolHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            // Parse arguments
            let scope_str = arguments
                .get("scope")
                .and_then(|s| s.as_str())
                .unwrap_or("staged");

            let severity_str = arguments
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("low");

            let pattern = arguments
                .get("pattern")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string());

            // Parse scope
            let scope = match scope_str {
                "staged" => ReviewScope::Staged,
                "working" => ReviewScope::WorkingTree,
                "all" => ReviewScope::All,
                _ => return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Invalid scope '{}'. Must be: staged, working, or all", scope_str),
                    is_error: true,
                }),
            };

            // Parse severity
            let severity = match severity_str {
                "critical" => Severity::Critical,
                "high" => Severity::High,
                "medium" => Severity::Medium,
                "low" => Severity::Low,
                _ => return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Invalid severity '{}'. Must be: critical, high, medium, or low", severity_str),
                    is_error: true,
                }),
            };

            // Create handler and execute
            let handler = ReviewHandler::new(scope, severity, pattern);
            handler.call(call_id, arguments).await
        })
    }
}

/// Handler for review tool (parameterized wrapper)
pub struct ReviewHandler {
    scope: ReviewScope,
    min_severity: Severity,
    file_pattern: Option<String>,
}

impl ReviewHandler {
    /// Create a new review handler
    pub fn new(scope: ReviewScope, min_severity: Severity, file_pattern: Option<String>) -> Self {
        ReviewHandler {
            scope,
            min_severity,
            file_pattern,
        }
    }

    /// Execute the review
    fn execute_review(&self, repo: &Repository) -> Result<ReviewReport> {
        // Generate diff based on scope
        let diff_text = match &self.scope {
            ReviewScope::Staged => self.generate_staged_diff(repo)?,
            ReviewScope::WorkingTree => self.generate_working_tree_diff(repo)?,
            ReviewScope::CommitRange(range) => self.generate_range_diff(repo, range)?,
            ReviewScope::All => self.generate_all_diff(repo)?,
        };

        // Parse diff into hunks
        let mut hunks = DiffParser::parse(&diff_text)?;

        // Apply file pattern filter if specified
        if let Some(pattern) = &self.file_pattern {
            hunks = DiffParser::filter_by_pattern(&hunks, pattern);
        }

        // Calculate diff statistics
        let diff_stats = DiffParser::stats(&hunks);

        // Detect issues
        let all_issues = IssueDetector::detect_issues(&hunks);

        // Filter by minimum severity
        let issues = IssueDetector::filter_by_severity(&all_issues, self.min_severity.clone());

        // Calculate quality score
        let quality_score = QualityAnalyzer::calculate_score(&hunks);

        // Count issues by severity
        let (critical_count, high_count, medium_count, low_count) =
            IssueDetector::count_by_severity(&all_issues);

        // Generate summary
        let summary = ReviewSummary {
            files_changed: diff_stats.files_changed,
            lines_added: diff_stats.insertions,
            lines_deleted: diff_stats.deletions,
            issues_count: all_issues.len(),
            critical_count,
            high_count,
            medium_count,
            low_count,
        };

        // Generate recommendations
        let recommendations = self.generate_recommendations(&issues, &quality_score);

        Ok(ReviewReport {
            summary,
            issues,
            quality_score,
            recommendations,
            diff_stats,
        })
    }

    /// Generate diff for staged changes
    fn generate_staged_diff(&self, repo: &Repository) -> Result<String> {
        let head = repo.head().map_err(|e| {
            FerroError::Tool(format!("Failed to get HEAD: {}", e))
        })?;

        let head_commit = head.peel_to_commit().map_err(|e| {
            FerroError::Tool(format!("Failed to peel HEAD to commit: {}", e))
        })?;

        let head_tree = head_commit.tree().map_err(|e| {
            FerroError::Tool(format!("Failed to get HEAD tree: {}", e))
        })?;

        let mut index = repo.index().map_err(|e| {
            FerroError::Tool(format!("Failed to get index: {}", e))
        })?;

        let index_tree = index.write_tree().map_err(|e| {
            FerroError::Tool(format!("Failed to write index tree: {}", e))
        })?;

        let index_tree = repo
            .find_tree(index_tree)
            .map_err(|e| FerroError::Tool(format!("Failed to find index tree: {}", e)))?;

        let diff = repo
            .diff_tree_to_tree(Some(&head_tree), Some(&index_tree), None)
            .map_err(|e| FerroError::Tool(format!("Failed to generate diff: {}", e)))?;

        self.format_diff(diff)
    }

    /// Generate diff for working tree
    fn generate_working_tree_diff(&self, repo: &Repository) -> Result<String> {
        let mut index = repo.index().map_err(|e| {
            FerroError::Tool(format!("Failed to get index: {}", e))
        })?;

        let index_tree = index.write_tree().map_err(|e| {
            FerroError::Tool(format!("Failed to write index tree: {}", e))
        })?;

        let index_tree = repo
            .find_tree(index_tree)
            .map_err(|e| FerroError::Tool(format!("Failed to find index tree: {}", e)))?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.include_untracked(true);

        let diff = repo
            .diff_tree_to_workdir(Some(&index_tree), Some(&mut diff_opts))
            .map_err(|e| FerroError::Tool(format!("Failed to generate diff: {}", e)))?;

        self.format_diff(diff)
    }

    /// Generate diff for commit range
    fn generate_range_diff(&self, repo: &Repository, range: &str) -> Result<String> {
        // Parse range (e.g., "main..HEAD")
        let parts: Vec<&str> = range.split("..").collect();
        if parts.len() != 2 {
            return Err(FerroError::Tool(format!(
                "Invalid commit range '{}'. Expected format: 'from..to'",
                range
            )));
        }

        let from = parts[0];
        let to = parts[1];

        // Resolve refs
        let from_obj = repo.revparse_single(from).map_err(|e| {
            FerroError::Tool(format!("Failed to resolve '{}': {}", from, e))
        })?;

        let to_obj = repo.revparse_single(to).map_err(|e| {
            FerroError::Tool(format!("Failed to resolve '{}': {}", to, e))
        })?;

        let from_tree = from_obj.peel_to_tree().map_err(|e| {
            FerroError::Tool(format!("Failed to peel '{}' to tree: {}", from, e))
        })?;

        let to_tree = to_obj.peel_to_tree().map_err(|e| {
            FerroError::Tool(format!("Failed to peel '{}' to tree: {}", to, e))
        })?;

        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(|e| FerroError::Tool(format!("Failed to generate diff: {}", e)))?;

        self.format_diff(diff)
    }

    /// Generate diff for all changes (staged + working tree)
    fn generate_all_diff(&self, repo: &Repository) -> Result<String> {
        // Combine staged and working tree diffs
        let staged_diff = self.generate_staged_diff(repo)?;
        let working_diff = self.generate_working_tree_diff(repo)?;

        Ok(format!("{}\n{}", staged_diff, working_diff))
    }

    /// Format git2 diff to string
    fn format_diff(&self, diff: git2::Diff) -> Result<String> {
        let mut diff_text = String::new();

        diff.print(
            git2::DiffFormat::Patch,
            |_delta, _hunk, line| {
                let origin = line.origin();
                if origin == ' ' || origin == '+' || origin == '-' || origin == '=' {
                    diff_text.push(origin);
                    diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
                }
                true
            },
        )
        .map_err(|e| FerroError::Tool(format!("Failed to format diff: {}", e)))?;

        Ok(diff_text)
    }

    /// Generate recommendations based on issues and quality score
    fn generate_recommendations(&self, issues: &[Issue], quality_score: &QualityScore) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Analyze issues by category
        let category_counts = IssueDetector::count_by_category(issues);

        // Security recommendations
        if let Some(&count) = category_counts.get(&IssueCategory::Security) {
            if count > 0 {
                recommendations.push(format!(
                    "🔒 Address {} security issue(s) immediately - hardcoded secrets or injection vulnerabilities detected",
                    count
                ));
            }
        }

        // Correctness recommendations
        if let Some(&count) = category_counts.get(&IssueCategory::Correctness) {
            if count > 0 {
                recommendations.push(format!(
                    "🐛 Fix {} correctness issue(s) - potential panics or error handling problems",
                    count
                ));
            }
        }

        // Testing recommendations
        if quality_score.testing < 60.0 {
            recommendations.push(format!(
                "🧪 Improve test coverage (current score: {:.0}/100) - add tests for new functionality",
                quality_score.testing
            ));
        }

        // Documentation recommendations
        if quality_score.documentation < 70.0 {
            recommendations.push(format!(
                "📚 Improve documentation (current score: {:.0}/100) - add doc comments for public items",
                quality_score.documentation
            ));
        }

        // Complexity recommendations
        if quality_score.complexity < 70.0 {
            recommendations.push(format!(
                "🔧 Reduce complexity (current score: {:.0}/100) - break down long functions and reduce nesting",
                quality_score.complexity
            ));
        }

        // Performance recommendations
        if let Some(&count) = category_counts.get(&IssueCategory::Performance) {
            if count > 0 {
                recommendations.push(format!(
                    "⚡ Review {} performance issue(s) - look for inefficient algorithms or unnecessary clones",
                    count
                ));
            }
        }

        // Style recommendations
        if let Some(&count) = category_counts.get(&IssueCategory::Style) {
            if count > 2 {
                recommendations.push(format!(
                    "✨ Address {} style issue(s) - improve code consistency and readability",
                    count
                ));
            }
        }

        // Overall quality recommendation
        if quality_score.total < 70.0 {
            recommendations.push(format!(
                "⭐ Overall quality score is {:.0}/100 (grade {}) - focus on critical and high severity issues first",
                quality_score.total,
                quality_score.grade()
            ));
        }

        recommendations
    }
}

impl ToolHandler for ReviewHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let result: Result<ToolResult> = (|| async {
                // Parse arguments
                let repo_path = arguments
                    .get("repo_path")
                    .and_then(|p| p.as_str())
                    .unwrap_or(".");

                let output_format = arguments
                    .get("output")
                    .and_then(|o| o.as_str())
                    .unwrap_or("text");

                // Find repository
                let repo = self.find_repository(repo_path)?;

                // Execute review
                let report = self.execute_review(&repo)?;

                // Format output
                let content = match output_format {
                    "json" => JsonReportGenerator::generate(&report),
                    _ => TextReportGenerator::generate(&report),
                };

                Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content,
                    is_error: false,
                })
            })()
            .await;

            result
        })
    }
}

impl ReviewHandler {
    /// Find a git repository (reuse from commit.rs)
    fn find_repository(&self, path: &str) -> Result<Repository> {
        let path_obj = Path::new(path);

        match Repository::open(path_obj) {
            Ok(repo) => Ok(repo),
            Err(_e) => {
                if path_obj.is_absolute() {
                    Err(FerroError::Tool(format!(
                        "Not a git repository (or any parent up to mount point): {}",
                        path
                    )))
                } else {
                    let current_dir = std::env::current_dir()
                        .map_err(|e| FerroError::Tool(format!("Failed to get current directory: {}", e)))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_scope_display() {
        let scope = ReviewScope::Staged;
        match scope {
            ReviewScope::Staged => {},
            ReviewScope::WorkingTree => {},
            ReviewScope::CommitRange(s) => assert_eq!(s, "main..HEAD"),
            ReviewScope::All => {},
        }
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }
}
