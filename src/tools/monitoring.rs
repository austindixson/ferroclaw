//! Logging and monitoring tools - get_logs, trace_execution, measure_metrics

use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ==================== get_logs ====================

pub fn get_logs_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "get_logs".into(),
            description: "Retrieve execution logs with filtering capabilities. Returns structured log entries with timestamps and severity levels.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of log entries to return"
                    },
                    "level": {
                        "type": "string",
                        "enum": ["all", "error", "warning", "info", "debug"],
                        "description": "Filter by log level"
                    },
                    "since": {
                        "type": "string",
                        "description": "ISO timestamp or duration (e.g., '2025-02-10T00:00:00Z' or '1h')"
                    }
                },
                "required": []
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::MemoryRead],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct GetLogsHandler {
    log_store: Arc<Mutex<LogStore>>,
}

impl Default for GetLogsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl GetLogsHandler {
    pub fn new() -> Self {
        Self {
            log_store: Arc::new(Mutex::new(LogStore::new())),
        }
    }
}

impl ToolHandler for GetLogsHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let limit = arguments
                .get("limit")
                .and_then(|l| l.as_u64())
                .unwrap_or(100);

            let level = arguments
                .get("level")
                .and_then(|l| l.as_str())
                .unwrap_or("all");

            let _since = arguments.get("since").and_then(|s| s.as_str());

            let store = self.log_store.lock().await;
            let logs = store.query(limit, level);

            let mut output = String::new();
            output.push_str("📜 Execution Logs\n");
            output.push_str("═══════════════════════════════════\n\n");

            if logs.is_empty() {
                output.push_str("No logs found.\n");
            } else {
                output.push_str(&format!("Found {} log entries:\n\n", logs.len()));

                for log in logs.iter().take(limit as usize) {
                    let icon = match log.level.as_str() {
                        "error" => "🔴",
                        "warning" => "🟡",
                        "info" => "🔵",
                        "debug" => "🟣",
                        _ => "⚪",
                    };

                    output.push_str(&format!(
                        "{} [{}] {} | {}\n",
                        icon,
                        log.timestamp.format("%Y-%m-%d %H:%M:%S"),
                        log.level.to_uppercase(),
                        log.message
                    ));
                }

                if logs.len() as u64 > limit {
                    output.push_str(&format!(
                        "\n... and {} more entries (use --limit to see more)\n",
                        logs.len() as u64 - limit
                    ));
                }
            }

            output.push_str("\n💡 Usage:\n");
            output.push_str("  • Use level filter: \"error\", \"warning\", \"info\", \"debug\"\n");
            output.push_str("  • Use --limit to control output size\n");
            output.push_str("  • Use --since to get recent logs\n");

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: output,
                is_error: false,
            })
        })
    }
}

// ==================== trace_execution ====================

pub fn trace_execution_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "trace_execution".into(),
            description: "Trace tool call chains and execution history. Visualizes the sequence of tool calls and their relationships.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "execution_id": {
                        "type": "string",
                        "description": "Specific execution ID to trace (optional)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["tree", "timeline", "table"],
                        "description": "Output format"
                    }
                },
                "required": []
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::MemoryRead],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct TraceExecutionHandler {
    trace_store: Arc<Mutex<TraceStore>>,
}

impl Default for TraceExecutionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceExecutionHandler {
    pub fn new() -> Self {
        Self {
            trace_store: Arc::new(Mutex::new(TraceStore::new())),
        }
    }
}

impl ToolHandler for TraceExecutionHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let execution_id = arguments.get("execution_id").and_then(|e| e.as_str());

            let format = arguments
                .get("format")
                .and_then(|f| f.as_str())
                .unwrap_or("tree");

            let store = self.trace_store.lock().await;

            let output = match format {
                "timeline" => format_timeline(&store, execution_id).await,
                "table" => format_table(&store, execution_id).await,
                _ => format_tree(&store, execution_id).await,
            };

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: output,
                is_error: false,
            })
        })
    }
}

async fn format_tree(store: &TraceStore, execution_id: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("🔍 Execution Trace (Tree View)\n");
    output.push_str("═════════════════════════════════════\n\n");

    let executions = if let Some(eid) = execution_id {
        match store.get(eid) {
            Some(exec) => vec![exec],
            None => store.list(),
        }
    } else {
        store.list()
    };

    if executions.is_empty() {
        output.push_str("No execution traces found.\n");
        return output;
    }

    for exec in &executions {
        output.push_str(&format!("📍 Execution ID: {}\n", exec.id));
        output.push_str(&format!(
            "  Started: {}\n",
            exec.start_time.format("%Y-%m-%d %H:%M:%S")
        ));

        if let Some(end_time) = exec.end_time {
            let duration = end_time - exec.start_time;
            output.push_str(&format!(
                "  Ended: {} (duration: {:.2}s)\n",
                end_time.format("%Y-%m-%d %H:%M:%S"),
                duration.num_milliseconds() as f64 / 1000.0
            ));
        } else {
            output.push_str("  Status: Still running\n");
        }

        output.push_str(&format!(
            "  Tool Calls: {} total\n\n",
            exec.tool_calls.len()
        ));

        // Build tree
        output.push_str("  Call Tree:\n");
        for (idx, call) in exec.tool_calls.iter().enumerate() {
            let icon = if call.success { "✅" } else { "❌" };
            let _indent = "    ".repeat(call.depth.min(4));
            output.push_str(&format!(
                "  {}. {} {} ({} ms)\n",
                idx + 1,
                icon,
                call.tool_name,
                call.duration_ms
            ));
        }
        output.push('\n');
    }

    output
}

async fn format_timeline(store: &TraceStore, execution_id: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("🔍 Execution Trace (Timeline View)\n");
    output.push_str("═══════════════════════════════════════\n\n");

    let executions = if let Some(eid) = execution_id {
        match store.get(eid) {
            Some(exec) => vec![exec],
            None => store.list(),
        }
    } else {
        store.list()
    };

    if executions.is_empty() {
        output.push_str("No execution traces found.\n");
        return output;
    }

    let mut all_calls: Vec<_> = executions
        .iter()
        .flat_map(|exec| exec.tool_calls.iter())
        .collect();

    all_calls.sort_by_key(|c| c.timestamp);

    for call in &all_calls {
        let icon = if call.success { "✅" } else { "❌" };
        output.push_str(&format!(
            "[{}] {} {} | {} ({} ms)\n",
            call.timestamp.format("%H:%M:%S"),
            icon,
            call.tool_name,
            call.execution_id,
            call.duration_ms
        ));
    }

    output.push_str("\n💡 Use the tree view for parent-child relationships.\n");

    output
}

async fn format_table(store: &TraceStore, execution_id: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("🔍 Execution Trace (Table View)\n");
    output.push_str("═══════════════════════════════════════\n\n");

    let executions = if let Some(eid) = execution_id {
        match store.get(eid) {
            Some(exec) => vec![exec],
            None => store.list(),
        }
    } else {
        store.list()
    };

    if executions.is_empty() {
        output.push_str("No execution traces found.\n");
        return output;
    }

    let all_calls: Vec<_> = executions
        .iter()
        .flat_map(|exec| exec.tool_calls.iter())
        .collect();

    // Group by tool
    let mut tool_stats: HashMap<String, (usize, u64)> = HashMap::new();
    for call in &all_calls {
        let (count, time) = tool_stats.entry(call.tool_name.clone()).or_insert((0, 0));
        *count += 1;
        *time += call.duration_ms;
    }

    output.push_str("Tool Call Statistics:\n\n");
    output.push_str(&format!(
        "{:<20} | {:>8} | {:>10} (ms)\n",
        "Tool", "Calls", "Total Time"
    ));
    output.push_str(&format!(
        "{:-<20}-+{:->8}-+{:->10}-\n",
        "----------", "--------", "-----------"
    ));

    let mut tools: Vec<_> = tool_stats.keys().cloned().collect();
    tools.sort_by_key(|k| std::cmp::Reverse(tool_stats.get(k).unwrap().0));

    for tool in tools.iter().take(10) {
        if let Some((count, time)) = tool_stats.get(tool) {
            let avg_time = *time as f64 / *count as f64;
            output.push_str(&format!(
                "{:<20} | {:>8} | {:>10.2}\n",
                tool, count, avg_time
            ));
        }
    }

    if tools.len() > 10 {
        output.push_str(&format!("\n... and {} more tools\n", tools.len() - 10));
    }

    output
}

// ==================== measure_metrics ====================

pub fn measure_metrics_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "measure_metrics".into(),
            description: "Measure and report performance metrics for tools and operations. Includes timing, memory usage, and throughput statistics.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Specific tool to measure (optional)"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["all", "fast", "slow", "errors"],
                        "description": "Category of metrics to report"
                    }
                },
                "required": []
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::MemoryRead],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct MeasureMetricsHandler {
    metrics_store: Arc<Mutex<MetricsStore>>,
}

impl Default for MeasureMetricsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasureMetricsHandler {
    pub fn new() -> Self {
        Self {
            metrics_store: Arc::new(Mutex::new(MetricsStore::new())),
        }
    }
}

impl ToolHandler for MeasureMetricsHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let tool_name = arguments.get("tool").and_then(|t| t.as_str());

            let category = arguments
                .get("category")
                .and_then(|c| c.as_str())
                .unwrap_or("all");

            let store = self.metrics_store.lock().await;
            let metrics = if let Some(tool) = tool_name {
                store.get_tool_metrics(tool)
            } else {
                store.get_all_metrics()
            };

            let output = format_metrics_report(&metrics, category);

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: output,
                is_error: false,
            })
        })
    }
}

fn format_metrics_report(metrics: &ToolMetrics, category: &str) -> String {
    let mut output = String::new();
    output.push_str("📊 Performance Metrics\n");
    output.push_str("═══════════════════════════════════\n\n");

    match category {
        "all" => {
            output.push_str(&format!("Total Tool Calls: {}\n", metrics.total_calls));
            output.push_str(&format!(
                "Total Duration: {:.2}s\n",
                metrics.total_duration.num_milliseconds() as f64 / 1000.0
            ));
            output.push_str(&format!(
                "Average Call Time: {:.2}ms\n",
                metrics.avg_duration_ms
            ));
            output.push_str(&format!(
                "Success Rate: {:.1}%\n",
                metrics.success_rate * 100.0
            ));
            output.push_str(&format!("Error Rate: {:.1}%\n", metrics.error_rate * 100.0));
            output.push_str("\nTop 5 Slowest Tools:\n");
            for (i, (tool, time)) in metrics.slowest_tools.iter().take(5).enumerate() {
                output.push_str(&format!("  {}. {} ({:.2}ms)\n", i + 1, tool, time));
            }
            output.push_str("\nTop 5 Most Called Tools:\n");
            for (i, (tool, count)) in metrics.most_called.iter().take(5).enumerate() {
                output.push_str(&format!("  {}. {} ({} calls)\n", i + 1, tool, count));
            }
        }
        "fast" => {
            output.push_str("Top 5 Fastest Tools (< 10ms):\n");
            let count = metrics
                .fastest_tools
                .iter()
                .filter(|(_, time)| *time < 10.0)
                .count();

            if count == 0 {
                output.push_str("  No tools faster than 10ms\n");
            } else {
                for (i, (tool, time)) in metrics
                    .fastest_tools
                    .iter()
                    .filter(|(_, time)| *time < 10.0)
                    .take(5)
                    .enumerate()
                {
                    output.push_str(&format!("  {}. {} ({:.2}ms)\n", i + 1, tool, time));
                }
            }
        }
        "slow" => {
            output.push_str("Top 10 Slowest Tools (> 100ms):\n");
            let count = metrics
                .slowest_tools
                .iter()
                .filter(|(_, time)| *time > 100.0)
                .count();

            if count == 0 {
                output.push_str("  No tools slower than 100ms\n");
            } else {
                for (i, (tool, time)) in metrics
                    .slowest_tools
                    .iter()
                    .filter(|(_, time)| *time > 100.0)
                    .take(10)
                    .enumerate()
                {
                    output.push_str(&format!("  {}. {} ({:.2}ms)\n", i + 1, tool, time));
                }
            }
        }
        "errors" => {
            output.push_str(&format!("Total Errors: {}\n\n", metrics.total_errors));
            if metrics.error_tools.is_empty() {
                output.push_str("No errors recorded.\n");
            } else {
                output.push_str("Tools with Errors:\n");
                for (i, (tool, errors)) in metrics.error_tools.iter().take(10).enumerate() {
                    output.push_str(&format!("  {}. {} ({} errors)\n", i + 1, tool, errors));
                }
            }
        }
        _ => {}
    }

    output.push_str("\n💡 Recommendations:\n");
    output.push_str("  • Investigate tools with high error rates\n");
    output.push_str("  • Optimize tools with long duration\n");
    output.push_str("  • Consider caching results for frequently called tools\n");

    output
}

// ==================== Data Structures ====================

#[derive(Debug, Clone)]
struct LogEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    level: String,
    message: String,
}

struct LogStore {
    entries: Vec<LogEntry>,
}

impl LogStore {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn log(&mut self, level: &str, message: &str) {
        self.entries.push(LogEntry {
            timestamp: chrono::Utc::now(),
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    fn query(&self, limit: u64, level: &str) -> Vec<LogEntry> {
        let mut filtered = self.entries.clone();

        if level != "all" {
            filtered.retain(|e| e.level == level);
        }

        filtered.reverse();
        filtered.truncate(limit as usize);
        filtered
    }
}

#[derive(Debug, Clone)]
struct ToolCall {
    timestamp: chrono::DateTime<chrono::Utc>,
    execution_id: String,
    tool_name: String,
    success: bool,
    duration_ms: u64,
    depth: usize,
}

#[derive(Debug, Clone)]
struct ExecutionTrace {
    id: String,
    start_time: chrono::DateTime<chrono::Utc>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
    tool_calls: Vec<ToolCall>,
}

struct TraceStore {
    executions: Vec<ExecutionTrace>,
}

impl TraceStore {
    fn new() -> Self {
        Self {
            executions: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn start_execution(&mut self, id: &str) {
        self.executions.push(ExecutionTrace {
            id: id.to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            tool_calls: Vec::new(),
        });
    }

    #[allow(dead_code)]
    fn add_tool_call(
        &mut self,
        execution_id: &str,
        tool_name: &str,
        success: bool,
        duration_ms: u64,
        depth: usize,
    ) {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == execution_id) {
            exec.tool_calls.push(ToolCall {
                timestamp: chrono::Utc::now(),
                execution_id: execution_id.to_string(),
                tool_name: tool_name.to_string(),
                success,
                duration_ms,
                depth,
            });
        }
    }

    #[allow(dead_code)]
    fn end_execution(&mut self, id: &str) {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == id) {
            exec.end_time = Some(chrono::Utc::now());
        }
    }

    fn get(&self, id: &str) -> Option<ExecutionTrace> {
        self.executions.iter().find(|e| e.id == id).cloned()
    }

    fn list(&self) -> Vec<ExecutionTrace> {
        self.executions.clone()
    }
}

#[derive(Debug, Clone)]
struct ToolMetrics {
    total_calls: usize,
    total_duration: chrono::Duration,
    avg_duration_ms: f64,
    success_rate: f64,
    error_rate: f64,
    total_errors: usize,
    fastest_tools: Vec<(String, f64)>,
    slowest_tools: Vec<(String, f64)>,
    most_called: Vec<(String, usize)>,
    error_tools: Vec<(String, usize)>,
}

struct MetricsStore {
    tool_calls: Vec<(String, u64, bool)>,
}

impl MetricsStore {
    fn new() -> Self {
        Self {
            tool_calls: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn record_call(&mut self, tool_name: &str, duration_ms: u64, success: bool) {
        self.tool_calls
            .push((tool_name.to_string(), duration_ms, success));
    }

    fn get_tool_metrics(&self, tool_name: &str) -> ToolMetrics {
        let calls: Vec<_> = self
            .tool_calls
            .iter()
            .filter(|(name, _, _)| name == tool_name)
            .cloned()
            .collect();

        let total_calls = calls.len();
        let total_duration = calls.iter().map(|(_, d, _)| d).sum::<u64>();
        let avg_duration_ms = if total_calls > 0 {
            total_duration as f64 / total_calls as f64
        } else {
            0.0
        };
        let success_count = calls.iter().filter(|(_, _, s)| *s).count();
        let success_rate = if total_calls > 0 {
            success_count as f64 / total_calls as f64
        } else {
            0.0
        };
        let error_rate = 1.0 - success_rate;

        ToolMetrics {
            total_calls,
            total_duration: chrono::Duration::milliseconds(total_duration as i64),
            avg_duration_ms,
            success_rate,
            error_rate,
            total_errors: total_calls - success_count,
            fastest_tools: calls
                .iter()
                .map(|(n, d, _)| (n.clone(), *d as f64))
                .collect(),
            slowest_tools: calls
                .iter()
                .map(|(n, d, _)| (n.clone(), *d as f64))
                .collect(),
            most_called: vec![(tool_name.to_string(), total_calls)],
            error_tools: vec![(tool_name.to_string(), total_calls - success_count)],
        }
    }

    fn get_all_metrics(&self) -> ToolMetrics {
        let tool_calls: Vec<_> = self.tool_calls.clone();

        let total_calls = tool_calls.len();
        let total_duration: u64 = tool_calls.iter().map(|(_, d, _)| d).sum();
        let avg_duration_ms = if total_calls > 0 {
            total_duration as f64 / total_calls as f64
        } else {
            0.0
        };
        let success_count = tool_calls.iter().filter(|(_, _, s)| *s).count();
        let success_rate = if total_calls > 0 {
            success_count as f64 / total_calls as f64
        } else {
            0.0
        };
        let error_rate = 1.0 - success_rate;

        // Group by tool
        let mut tool_stats: std::collections::HashMap<String, (usize, u64, usize)> =
            std::collections::HashMap::new();
        for (name, duration, success) in &tool_calls {
            let (count, time, errors) = tool_stats.entry(name.clone()).or_insert((0, 0, 0));
            *count += 1;
            *time += duration;
            if !success {
                *errors += 1;
            }
        }

        let mut slowest_tools: Vec<_> = tool_stats
            .iter()
            .map(|(name, (_, _, time))| (name.clone(), *time as f64))
            .collect();
        slowest_tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        slowest_tools.truncate(10);

        let mut fast_tools: Vec<_> = tool_stats
            .iter()
            .map(|(name, (_, _, time))| (name.clone(), *time as f64))
            .collect();
        fast_tools.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut most_called: Vec<_> = tool_stats
            .iter()
            .map(|(name, (count, _, _))| (name.clone(), *count))
            .collect();
        most_called.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let mut error_tools: Vec<_> = tool_stats
            .iter()
            .map(|(name, (_, _, errors))| (name.clone(), *errors))
            .collect();
        error_tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        ToolMetrics {
            total_calls,
            total_duration: chrono::Duration::milliseconds(total_duration as i64),
            avg_duration_ms,
            success_rate,
            error_rate,
            total_errors: total_calls - success_count,
            fastest_tools: fast_tools,
            slowest_tools,
            most_called,
            error_tools,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_store() {
        let mut store = LogStore::new();
        store.log("info", "Test message");
        store.log("error", "Error message");

        let logs = store.query(10, "all");
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn test_trace_store() {
        let mut store = TraceStore::new();
        store.start_execution("exec_1");
        store.add_tool_call("exec_1", "tool1", true, 100, 0);
        store.add_tool_call("exec_1", "tool2", false, 200, 1);
        store.end_execution("exec_1");

        let exec = store.get("exec_1");
        assert!(exec.is_some());
        assert_eq!(exec.unwrap().tool_calls.len(), 2);
    }
}
