//! Result evaluation tool - assess success/failure of actions

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use serde_json::Value;

pub fn evaluate_result_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "evaluate_result".into(),
            description: "Evaluate the result of an action against success criteria. Provides detailed assessment and recommendations.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The task or action that was performed"
                    },
                    "result": {
                        "type": "string",
                        "description": "The actual result achieved"
                    },
                    "success_criteria": {
                        "type": "string",
                        "description": "The criteria for success"
                    },
                    "metrics": {
                        "type": "object",
                        "description": "Optional metrics to evaluate (e.g., performance, quality)"
                    }
                },
                "required": ["task", "result", "success_criteria"]
            }),
            server_name: None,
        },
        required_capabilities: vec![],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct EvaluateResultHandler;

impl ToolHandler for EvaluateResultHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let task = arguments
                .get("task")
                .and_then(|t| t.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'task' argument".into()))?;

            let result = arguments
                .get("result")
                .and_then(|r| r.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'result' argument".into()))?;

            let success_criteria = arguments
                .get("success_criteria")
                .and_then(|s| s.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'success_criteria' argument".into()))?;

            let metrics = arguments.get("metrics");

            let evaluation = perform_evaluation(task, result, success_criteria, metrics);

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: evaluation,
                is_error: false,
            })
        })
    }
}

fn perform_evaluation(
    task: &str,
    result: &str,
    success_criteria: &str,
    metrics: Option<&Value>,
) -> String {
    let mut output = String::new();
    output.push_str("📊 Result Evaluation\n");
    output.push_str("═══════════════════════════\n\n");

    output.push_str("📋 Task:\n");
    output.push_str(&format!("  {}\n\n", task));

    output.push_str("✅ Success Criteria:\n");
    for line in success_criteria.lines() {
        output.push_str(&format!("  • {}\n", line));
    }
    output.push('\n');

    output.push_str("📤 Result:\n");
    for line in result.lines().take(10) {
        output.push_str(&format!("  {}\n", line));
    }
    if result.lines().count() > 10 {
        output.push_str(&format!(
            "  ... ({} more lines)\n\n",
            result.lines().count() - 10
        ));
    } else {
        output.push('\n');
    }

    // Analyze success criteria
    let criteria_lines: Vec<&str> = success_criteria.lines().collect();
    let mut met_criteria = Vec::new();
    let mut unmet_criteria = Vec::new();

    for criterion in &criteria_lines {
        let criterion = criterion.trim();
        let criterion_lower = criterion.to_lowercase();

        // Check if criterion is met
        let is_met = criterion.contains("no error") && !result.to_lowercase().contains("error")
            || criterion.contains("error") && result.to_lowercase().contains("error")
            || criterion_lower.contains("success") && result.to_lowercase().contains("success")
            || criterion_lower.contains("complete") && result.to_lowercase().contains("complete")
            || criterion_lower.contains("done") && result.to_lowercase().contains("done")
            || criterion_lower.contains("0") && result.contains("0")
            || criterion_lower.contains("true") && result.to_lowercase().contains("true")
            || criterion_lower.contains("ok") && result.to_lowercase().contains("ok")
            || criterion_lower.contains("✓")
            || criterion_lower.contains("✔")
            || criterion_lower.contains("✅");

        if is_met {
            met_criteria.push(criterion);
        } else if !criterion_lower.contains("not") {
            unmet_criteria.push(criterion);
        } else {
            met_criteria.push(criterion);
        }
    }

    output.push_str("📈 Assessment:\n");
    output.push_str(&format!("  Total Criteria: {}\n", criteria_lines.len()));
    output.push_str(&format!("  ✅ Met: {}\n", met_criteria.len()));
    output.push_str(&format!("  ❌ Not Met: {}\n\n", unmet_criteria.len()));

    // Calculate success rate
    let success_rate = if !criteria_lines.is_empty() {
        met_criteria.len() as f64 / criteria_lines.len() as f64 * 100.0
    } else {
        0.0
    };

    let (status, score) = if unmet_criteria.is_empty() {
        ("✅ SUCCESS", success_rate)
    } else if met_criteria.len() > unmet_criteria.len() {
        ("⚠️  PARTIAL SUCCESS", success_rate)
    } else {
        ("❌ FAILED", success_rate)
    };

    output.push_str(&format!("📊 Overall Status: {}\n", status));
    output.push_str(&format!("📊 Success Rate: {:.1}%\n\n", score));

    // Detailed analysis
    if !met_criteria.is_empty() {
        output.push_str("✅ Criteria Met:\n");
        for criterion in &met_criteria {
            output.push_str(&format!("  • {}\n", criterion));
        }
        output.push('\n');
    }

    if !unmet_criteria.is_empty() {
        output.push_str("❌ Criteria Not Met:\n");
        for criterion in &unmet_criteria {
            output.push_str(&format!("  • {}\n", criterion));
        }
        output.push('\n');
    }

    // Metrics evaluation if provided
    if let Some(metrics) = metrics {
        output.push_str("📊 Metrics Evaluation:\n");
        if let Some(obj) = metrics.as_object() {
            for (key, value) in obj {
                match (key.as_str(), value) {
                    ("performance", Value::String(perf)) => {
                        output.push_str(&format!("  Performance: {}\n", perf));
                        if perf.contains("slow") || perf.contains("timeout") {
                            output.push_str("    ⚠️  Performance issues detected\n");
                        } else if perf.contains("fast") || perf.contains("good") {
                            output.push_str("    ✅ Performance is good\n");
                        }
                    }
                    ("quality", Value::Number(quality)) => {
                        let q = quality.as_f64().unwrap_or(0.0);
                        output.push_str(&format!("  Quality Score: {:.1}/100\n", q));
                        if q >= 90.0 {
                            output.push_str("    ✅ Excellent quality\n");
                        } else if q >= 70.0 {
                            output
                                .push_str("    ⚠️  Good quality, room for improvement\n");
                        } else {
                            output.push_str("    ❌ Poor quality, needs improvement\n");
                        }
                    }
                    ("errors", Value::Number(errors)) => {
                        let e = errors.as_i64().unwrap_or(0);
                        output.push_str(&format!("  Errors: {}\n", e));
                        if e == 0 {
                            output.push_str("    ✅ No errors\n");
                        } else {
                            output.push_str("    ❌ Errors detected\n");
                        }
                    }
                    _ => {
                        output.push_str(&format!("  {}: {}\n", key, value));
                    }
                }
            }
        }
        output.push('\n');
    }

    // Recommendations
    output.push_str("💡 Recommendations:\n");

    if unmet_criteria.is_empty() {
        output.push_str("  ✅ All criteria met - great work!\n");
        output.push_str("  • Document the successful approach for future reference\n");
    } else if met_criteria.len() > unmet_criteria.len() {
        output.push_str("  ⚠️  Partial success - some improvements needed:\n");
        output.push_str("  • Review the unmet criteria above\n");
        output.push_str("  • Consider if the approach needs refinement\n");
    } else {
        output.push_str("  ❌ Failed - significant improvements needed:\n");
        output.push_str("  • Re-examine the approach and strategy\n");
        output.push_str("  • Break down the task into smaller steps\n");
        output.push_str("  • Verify understanding of requirements\n");
    }

    if success_rate < 100.0 {
        output.push_str(&format!(
            "  • Success rate is {:.1}% - aim for higher\n",
            success_rate
        ));
    }

    if let Some(metrics) = metrics {
        if metrics.get("performance").is_some() {
            output.push_str("  • Consider performance optimizations\n");
        }
        if metrics.get("quality").is_some() {
            output.push_str("  • Review quality metrics and improve\n");
        }
    }

    output.push_str("  • Document lessons learned for future iterations\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_result_success() {
        let task = "Build the project";
        let result = "Build successful\n✅ Compilation complete\n0 errors";
        let criteria = "No errors\nCompilation successful";

        let evaluation = perform_evaluation(task, result, criteria, None);
        assert!(evaluation.contains("Overall Status: ✅ SUCCESS"));
        assert!(evaluation.contains("Success Rate: 100.0%"));
    }

    #[test]
    fn test_evaluate_result_partial() {
        let task = "Run tests";
        let result =
            "Success - some tests passed\nComplete execution\n5 tests passed\n1 test failed";
        let criteria = "All tests pass\nSuccess\nComplete";

        let evaluation = perform_evaluation(task, result, criteria, None);
        assert!(evaluation.contains("Overall Status: ⚠️  PARTIAL SUCCESS"));
    }

    #[test]
    fn test_evaluate_result_with_metrics() {
        let task = "Optimize function";
        let result = "Function optimized\nPerformance improved";
        let criteria = "Performance improved\nCode quality maintained";

        let metrics = serde_json::json!({
            "performance": "fast",
            "quality": 85
        });

        let evaluation = perform_evaluation(task, result, criteria, Some(&metrics));
        assert!(evaluation.contains("Metrics Evaluation"));
        assert!(evaluation.contains("Performance: fast"));
        assert!(evaluation.contains("Quality Score: 85.0/100"));
    }
}
