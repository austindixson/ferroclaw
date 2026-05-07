# ReviewCommand Design Document

## Overview
ReviewCommand provides code review functionality with diff analysis, quality scoring, and issue categorization for the Ferroclaw AI agent framework.

## Data Structures

### Issue
Represents a code quality issue found during review.

```rust
pub struct Issue {
    pub severity: Severity,
    pub category: IssueCategory,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub message: String,
    pub suggestion: Option<String>,
    pub code_snippet: Option<String>,
}
```

### Severity
```rust
pub enum Severity {
    Critical,  // Security vulnerabilities, crashes
    High,      // Major bugs, performance issues
    Medium,    // Style issues, minor bugs
    Low,       // Nitpicks, suggestions
}
```

### IssueCategory
```rust
pub enum IssueCategory {
    Security,     // Injection, auth, crypto
    Performance,  // Inefficient algorithms, memory
    Style,        // Naming, formatting
    Correctness,  // Logic errors, edge cases
    Testing,      // Missing tests, coverage
    Documentation,// Missing docs, unclear comments
    Complexity,   // High cyclomatic complexity
    Maintainability, // Code duplication, coupling
}
```

### QualityScore
```rust
pub struct QualityScore {
    pub total: f64,           // 0-100
    pub complexity: f64,      // 0-100
    pub readability: f64,     // 0-100
    pub testing: f64,         // 0-100
    pub documentation: f64,   // 0-100
}
```

### ReviewReport
```rust
pub struct ReviewReport {
    pub summary: ReviewSummary,
    pub issues: Vec<Issue>,
    pub quality_score: QualityScore,
    pub recommendations: Vec<String>,
    pub diff_stats: DiffStats,
}
```

### ReviewSummary
```rust
pub struct ReviewSummary {
    pub files_changed: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub issues_count: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
}
```

### DiffStats
```rust
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}
```

## Core Components

### 1. DiffParser
Parses git diff output into structured hunks.

```rust
pub struct DiffParser;

impl DiffParser {
    /// Parse diff text into structured hunks
    pub fn parse(diff_text: &str) -> Result<Vec<DiffHunk>>;

    /// Filter hunks by file patterns
    pub fn filter_by_pattern(hunks: &[DiffHunk], pattern: &str) -> Vec<DiffHunk>;
}

pub struct DiffHunk {
    pub file_path: String,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

pub enum DiffLineType {
    Context,
    Added,
    Deleted,
    Range,
}

pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    pub line_number: Option<usize>,
}
```

### 2. QualityAnalyzer
Analyzes code quality metrics.

```rust
pub struct QualityAnalyzer;

impl QualityAnalyzer {
    /// Calculate overall quality score
    pub fn calculate_score(hunks: &[DiffHunk]) -> QualityScore;

    /// Analyze complexity (cyclomatic, nesting)
    pub fn analyze_complexity(hunk: &DiffHunk) -> f64;

    /// Analyze readability (line length, naming)
    pub fn analyze_readability(hunk: &DiffHunk) -> f64;

    /// Check test coverage
    pub fn analyze_testing(hunks: &[DiffHunk]) -> f64;

    /// Check documentation
    pub fn analyze_documentation(hunks: &[DiffHunk]) -> f64;
}
```

### 3. IssueDetector
Finds code quality issues.

```rust
pub struct IssueDetector;

impl IssueDetector {
    /// Detect all issues in diff
    pub fn detect_issues(hunks: &[DiffHunk]) -> Vec<Issue>;

    /// Detect security issues
    fn detect_security(hunk: &DiffHunk) -> Vec<Issue>;

    /// Detect performance issues
    fn detect_performance(hunk: &DiffHunk) -> Vec<Issue>;

    /// Detect style issues
    fn detect_style(hunk: &DiffHunk) -> Vec<Issue>;

    /// Detect correctness issues
    fn detect_correctness(hunk: &DiffHunk) -> Vec<Issue>;

    /// Detect complexity issues
    fn detect_complexity(hunk: &DiffHunk) -> Vec<Issue>;
}
```

### 4. ReviewCommand
Main command handler.

```rust
pub struct ReviewHandler {
    scope: ReviewScope,
    min_severity: Severity,
    file_pattern: Option<String>,
}

pub enum ReviewScope {
    Staged,       // Index vs HEAD
    WorkingTree,  // Working dir vs Index
    CommitRange(String), // e.g., "main..HEAD"
    All,          // All changes
}
```

## Issue Detection Rules

### Security (Critical)
- Hardcoded secrets (API keys, passwords)
- SQL injection patterns (string concatenation in queries)
- Command injection (shell command with user input)
- Missing auth checks
- Insecure crypto algorithms

### Performance (High)
- Nested loops O(n²) without comment
- Missing cache in expensive operations
- Inefficient data structures (Vec for lookups)
- Unnecessary clones in loops
- Blocking I/O in async context

### Style (Medium)
- Line length > 100 characters
- Inconsistent naming (snake_case vs camelCase)
- Missing pub/priv modifiers
- Unused variables
- TODO/FIXME comments

### Correctness (High)
- Unwrap() calls without context
- Empty error handlers
- Missing error propagation
- Potential panics (index access)
- Missing null checks

### Complexity (Medium)
- Nesting depth > 4
- Function length > 50 lines
- Cyclomatic complexity > 10
- Too many parameters (> 5)

### Testing (High)
- New functions without tests
- Missing edge case coverage
- No assertion messages
- Untested error paths

### Documentation (Low)
- Missing pub struct docs
- Missing pub function docs
- Unclear variable names
- Magic numbers without comments

## Scoring Algorithm

### Quality Score Calculation
```
Total Score = (Complexity * 0.3 + Readability * 0.3 + Testing * 0.25 + Documentation * 0.15)

Where each component:
- Complexity: 100 - (avg_complexity / max_complexity * 100)
- Readability: 100 - (style_issues / total_lines * 100)
- Testing: (test_lines / code_lines * 100)
- Documentation: (documented_items / total_items * 100)
```

### Severity Impact on Score
```
Critical: -20 points each
High: -10 points each
Medium: -5 points each
Low: -2 points each
```

## CLI Interface

```bash
/review                           # Review staged changes
/review --scope working           # Review working tree
/review --scope main..HEAD        # Review commit range
/review --severity high           # Only show high+ issues
/review --pattern "**/*.rs"       # Only review Rust files
/review --output json             # JSON output
/review --output text             # Human-readable text (default)
```

## Output Format

### Text Output
```
Review Report
============

Summary
-------
Files changed: 5
Lines added: 234
Lines deleted: 87
Issues found: 12

Issues (Severity: HIGH+)
------------------------
[CRITICAL] src/auth.rs:45
  Category: Security
  Hardcoded API key detected
  Suggestion: Use environment variable

  44:     let api_key = "sk-1234567890";
  45: ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

[HIGH] src/user.rs:123
  Category: Correctness
  Potential panic on unwrap()
  Suggestion: Use ? operator or match

  123:     let user = user.unwrap();
       ^^^^^^^^^^^^^^^^^^^^^^^^

Quality Score
-------------
Total: 72/100
  Complexity: 68/100
  Readability: 85/100
  Testing: 60/100
  Documentation: 75/100

Recommendations
---------------
1. Add tests for auth module (coverage: 30%)
2. Remove hardcoded secrets (2 found)
3. Reduce function length in user.rs (3 functions > 50 lines)
4. Add error context for unwrap() calls (4 instances)
```

### JSON Output
```json
{
  "summary": {
    "files_changed": 5,
    "lines_added": 234,
    "lines_deleted": 87,
    "issues_count": 12,
    "critical_count": 1,
    "high_count": 3,
    "medium_count": 5,
    "low_count": 3
  },
  "issues": [...],
  "quality_score": {
    "total": 72.0,
    "complexity": 68.0,
    "readability": 85.0,
    "testing": 60.0,
    "documentation": 75.0
  },
  "recommendations": [...]
}
```

## Implementation Phases

1. **Data structures** - Define all types
2. **Diff parsing** - Parse git2 diff output
3. **Issue detection** - Implement detectors
4. **Quality scoring** - Calculate metrics
5. **Command handler** - Wire up git2 integration
6. **CLI integration** - Add command to main.rs
7. **Testing** - Unit and integration tests
8. **Documentation** - Example outputs

## Testing Strategy

### Unit Tests
- Diff parsing with various formats
- Issue detection rules
- Quality score calculations
- Severity categorization

### Integration Tests
- Full review workflow with real repos
- Different scopes (staged, working, range)
- File pattern filtering
- Output format generation

### Test Coverage
Target: 80%+ across all modules

## References

- git2-rs documentation
- Conventional Commits specification
- Rust API guidelines
- Clippy lints (inspiration for rules)
- Existing CommitHandler patterns
