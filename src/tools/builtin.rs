//! Built-in tools: filesystem, bash, web_fetch, memory, file_edit, glob, commit, agent,
//! analyze_code, collaboration, refactor_code, generate_tests, review_code, find_bugs,
//! execute_code, evaluate_result, monitoring, build.

use crate::error::FerroError;
use crate::memory::MemoryStore;
use crate::tool::{ToolFuture, ToolHandler, ToolRegistry};
use crate::tools::agent::{AgentTool, agent_tool_meta};
use crate::tools::glob::{GlobTool, glob_tool_meta};
use crate::tools::grep::{GrepTool, grep_tool_meta};
use crate::types::{Capability, ToolDefinition, ToolMeta, ToolResult, ToolSource};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::tools::commit::CommitHandler;
use crate::tools::file_edit::FileEditHandler;

// New tool imports
use crate::tools::analyze_code::{AnalyzeCodeHandler, analyze_code_meta};
use crate::tools::build::{BuildHandler, build_meta};
use crate::tools::collaboration::{
    CommentHandler, NotifyUserHandler, RequestApprovalHandler, ShareContextHandler, comment_meta,
    notify_user_meta, request_approval_meta, share_context_meta,
};
use crate::tools::evaluate_result::{EvaluateResultHandler, evaluate_result_meta};
use crate::tools::execute_code::{ExecuteCodeHandler, execute_code_meta};
use crate::tools::find_bugs::{FindBugsHandler, find_bugs_meta};
use crate::tools::generate_tests::{GenerateTestsHandler, generate_tests_meta};
use crate::tools::monitoring::{
    GetLogsHandler, MeasureMetricsHandler, TraceExecutionHandler, get_logs_meta,
    measure_metrics_meta, trace_execution_meta,
};
use crate::tools::refactor_code::{RefactorCodeHandler, refactor_code_meta};
use crate::tools::review_code::{ReviewCodeHandler, review_code_meta};

/// Register all built-in tools into the registry.
pub fn register_builtin_tools(registry: &mut ToolRegistry, memory: Arc<Mutex<MemoryStore>>) {
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "read_file".into(),
                description: "Read the contents of a file at the given path".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to read"
                        }
                    },
                    "required": ["path"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::FsRead],
            source: ToolSource::Builtin,
        },
        Box::new(ReadFileHandler),
    );

    // write_file
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "write_file".into(),
                description: "Write content to a file at the given path".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::FsWrite],
            source: ToolSource::Builtin,
        },
        Box::new(WriteFileHandler),
    );

    // list_directory
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "list_directory".into(),
                description: "List the contents of a directory".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the directory to list"
                        }
                    },
                    "required": ["path"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::FsRead],
            source: ToolSource::Builtin,
        },
        Box::new(ListDirectoryHandler),
    );

    // bash
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "bash".into(),
                description: "Execute a bash command and return stdout/stderr".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The bash command to execute"
                        },
                        "background": {
                            "type": "boolean",
                            "description": "Run command in background and return immediately with PID/log path"
                        }
                    },
                    "required": ["command"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::ProcessExec],
            source: ToolSource::Builtin,
        },
        Box::new(BashHandler),
    );

    // web_fetch
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "web_fetch".into(),
                description: "Fetch content from a URL via HTTP GET".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        }
                    },
                    "required": ["url"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::NetOutbound],
            source: ToolSource::Builtin,
        },
        Box::new(WebFetchHandler),
    );

    // memory_search
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "memory_search".into(),
                description: "Search stored memories using full-text search".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        }
                    },
                    "required": ["query"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::MemoryRead],
            source: ToolSource::Builtin,
        },
        Box::new(MemorySearchHandler {
            store: Arc::clone(&memory),
        }),
    );

    // memory_store
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "memory_store".into(),
                description: "Store a key-value pair in persistent memory".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Memory key (e.g. 'user_preference_theme')"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to remember"
                        }
                    },
                    "required": ["key", "content"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::MemoryWrite],
            source: ToolSource::Builtin,
        },
        Box::new(MemoryStoreHandler {
            store: Arc::clone(&memory),
        }),
    );

    // file_edit
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "file_edit".into(),
                description: "Perform exact string replacement in a file. Ensures that old_string exists and is unique before making changes. Atomic write operations for safety.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path to the file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact string to replace (must be unique in the file)"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement string"
                        }
                    },
                    "required": ["file_path", "old_string", "new_string"]
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::FsRead, Capability::FsWrite],
            source: ToolSource::Builtin,
        },
        Box::new(FileEditHandler),
    );

    // glob
    registry.register(glob_tool_meta(), Box::new(GlobTool::new()));

    // grep
    registry.register(grep_tool_meta(), Box::new(GrepTool::new()));

    // agent
    registry.register(agent_tool_meta(), Box::new(AgentTool::new()));

    // commit
    registry.register(
        ToolMeta {
            definition: ToolDefinition {
                name: "commit".into(),
                description: "Create a git commit with conventional commits format. Analyzes staged changes, generates commit message, and optionally creates the commit. Supports --yes flag for auto-approval and --amend for amending previous commit.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "repo_path": {
                            "type": "string",
                            "description": "Path to the git repository (default: current directory)"
                        },
                        "yes": {
                            "type": "boolean",
                            "description": "Auto-approve commit without interactive prompt"
                        },
                        "amend": {
                            "type": "boolean",
                            "description": "Amend the previous commit instead of creating a new one"
                        }
                    },
                    "required": []
                }),
                server_name: None,
            },
            required_capabilities: vec![Capability::ProcessExec],
            source: ToolSource::Builtin,
        },
        Box::new(CommitHandler),
    );

    // ========== NEW TOOLS ==========

    // analyze_code
    registry.register(analyze_code_meta(), Box::new(AnalyzeCodeHandler));

    // Collaboration tools
    registry.register(notify_user_meta(), Box::new(NotifyUserHandler));
    registry.register(request_approval_meta(), Box::new(RequestApprovalHandler));
    registry.register(share_context_meta(), Box::new(ShareContextHandler));
    registry.register(comment_meta(), Box::new(CommentHandler));

    // Code intelligence tools
    registry.register(refactor_code_meta(), Box::new(RefactorCodeHandler));
    registry.register(generate_tests_meta(), Box::new(GenerateTestsHandler));
    registry.register(review_code_meta(), Box::new(ReviewCodeHandler));
    registry.register(find_bugs_meta(), Box::new(FindBugsHandler));

    // Execution tools
    registry.register(execute_code_meta(), Box::new(ExecuteCodeHandler));

    // Reasoning tools
    registry.register(evaluate_result_meta(), Box::new(EvaluateResultHandler));

    // Monitoring tools
    registry.register(get_logs_meta(), Box::new(GetLogsHandler::new()));
    registry.register(
        trace_execution_meta(),
        Box::new(TraceExecutionHandler::new()),
    );
    registry.register(
        measure_metrics_meta(),
        Box::new(MeasureMetricsHandler::new()),
    );

    // Development workflow tools
    registry.register(build_meta(), Box::new(BuildHandler));
}

// --- Tool Handlers ---

struct ReadFileHandler;
impl ToolHandler for ReadFileHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            match tokio::fs::read_to_string(path).await {
                Ok(content) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content,
                    is_error: false,
                }),
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Error reading {path}: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}

struct WriteFileHandler;
impl ToolHandler for WriteFileHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;
            let content = arguments
                .get("content")
                .and_then(|c| c.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'content' argument".into()))?;

            match tokio::fs::write(path, content).await {
                Ok(_) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Successfully wrote {} bytes to {path}", content.len()),
                    is_error: false,
                }),
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Error writing {path}: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}

struct ListDirectoryHandler;
impl ToolHandler for ListDirectoryHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'path' argument".into()))?;

            match tokio::fs::read_dir(path).await {
                Ok(mut entries) => {
                    let mut items = Vec::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let file_type = entry.file_type().await.ok();
                        let suffix = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                            "/"
                        } else {
                            ""
                        };
                        items.push(format!("{name}{suffix}"));
                    }
                    items.sort();
                    Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: items.join("\n"),
                        is_error: false,
                    })
                }
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Error listing {path}: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}

struct BashHandler;

fn looks_like_long_running_server(command: &str) -> bool {
    let c = command.to_lowercase();
    let hints = [
        "npm run dev",
        "pnpm dev",
        "yarn dev",
        "vite",
        "next dev",
        "python -m http.server",
        "uvicorn",
        "flask run",
        "rails server",
        "cargo run",
        "docker compose up",
        "serve ",
    ];
    hints.iter().any(|h| c.contains(h))
}

impl ToolHandler for BashHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let command = arguments
                .get("command")
                .and_then(|c| c.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'command' argument".into()))?;
            let requested_background = arguments
                .get("background")
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            let should_background = requested_background || looks_like_long_running_server(command);

            if should_background {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let log_dir = std::path::PathBuf::from("/tmp/ferroclaw-bg");
                tokio::fs::create_dir_all(&log_dir)
                    .await
                    .map_err(|e| FerroError::Tool(format!("Failed to create bg log dir: {e}")))?;
                let log_path = log_dir.join(format!("{call_id}-{ts}.log"));
                let wrapped = format!(
                    "nohup bash -lc {} > {} 2>&1 & echo $!",
                    serde_json::to_string(command)
                        .map_err(|e| FerroError::Tool(format!("Failed to quote command: {e}")))?,
                    serde_json::to_string(log_path.to_string_lossy().as_ref())
                        .map_err(|e| FerroError::Tool(format!("Failed to quote log path: {e}")))?
                );
                let output = tokio::process::Command::new("bash")
                    .arg("-lc")
                    .arg(wrapped)
                    .output()
                    .await
                    .map_err(|e| FerroError::Tool(format!("Failed to start background command: {e}")))?;
                let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let content = format!(
                    "Background process started.\npid: {}\nlog: {}\nstop: kill {}\nstatus: ps -p {} -o pid=,ppid=,stat=,etime=,command=\n",
                    pid,
                    log_path.display(),
                    pid,
                    pid
                );
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content,
                    is_error: pid.is_empty(),
                });
            }

            let output = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Failed to execute command: {e}")))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let content = if output.status.success() {
                stdout.to_string()
            } else {
                format!(
                    "Exit code: {}\nStdout: {stdout}\nStderr: {stderr}",
                    output.status.code().unwrap_or(-1)
                )
            };

            Ok(ToolResult {
                call_id: call_id.to_string(),
                content,
                is_error: !output.status.success(),
            })
        })
    }
}

struct WebFetchHandler;
impl ToolHandler for WebFetchHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let url = arguments
                .get("url")
                .and_then(|u| u.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'url' argument".into()))?;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| FerroError::Tool(format!("HTTP client error: {e}")))?;

            match client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp
                        .text()
                        .await
                        .unwrap_or_else(|e| format!("Failed to read body: {e}"));

                    // Limit response size
                    let truncated = if body.len() > 50_000 {
                        format!(
                            "{}...\n[Truncated: {} total chars]",
                            &body[..50_000],
                            body.len()
                        )
                    } else {
                        body
                    };

                    Ok(ToolResult {
                        call_id: call_id.to_string(),
                        content: format!("[{status}]\n{truncated}"),
                        is_error: !status.is_success(),
                    })
                }
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Fetch error: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}

struct MemorySearchHandler {
    store: Arc<Mutex<MemoryStore>>,
}
impl ToolHandler for MemorySearchHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let query = arguments
                .get("query")
                .and_then(|q| q.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'query' argument".into()))?;

            let store = self.store.lock().await;
            match store.search(query, 10) {
                Ok(memories) => {
                    if memories.is_empty() {
                        Ok(ToolResult {
                            call_id: call_id.to_string(),
                            content: "No memories found.".into(),
                            is_error: false,
                        })
                    } else {
                        let results: Vec<String> = memories
                            .iter()
                            .map(|m| format!("[{}] {}", m.key, m.content))
                            .collect();
                        Ok(ToolResult {
                            call_id: call_id.to_string(),
                            content: results.join("\n"),
                            is_error: false,
                        })
                    }
                }
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Memory search error: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}

struct MemoryStoreHandler {
    store: Arc<Mutex<MemoryStore>>,
}
impl ToolHandler for MemoryStoreHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let key = arguments
                .get("key")
                .and_then(|k| k.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'key' argument".into()))?;
            let content = arguments
                .get("content")
                .and_then(|c| c.as_str())
                .ok_or_else(|| FerroError::Tool("Missing 'content' argument".into()))?;

            let store = self.store.lock().await;
            match store.insert(key, content) {
                Ok(_) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Stored memory '{key}'"),
                    is_error: false,
                }),
                Err(e) => Ok(ToolResult {
                    call_id: call_id.to_string(),
                    content: format!("Memory store error: {e}"),
                    is_error: true,
                }),
            }
        })
    }
}
