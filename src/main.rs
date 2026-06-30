use clap::Parser;
use ferroclaw::agent::AgentLoop;
use ferroclaw::benchmark_mode::BenchmarkTelemetry;
use ferroclaw::cli::{
    AuditCommands, AuthCommands, Cli, Commands, ConfigCommands, GatewayCommands, McpCommands,
    ModelCommands, PlanCommands, TaskCommands,
};
use ferroclaw::config::{self, Config};
use ferroclaw::mcp::client::McpClient;
use ferroclaw::mcp::diet::{generate_skill_summary, render_skill_summary};
use ferroclaw::mcp::registry::populate_registry_from_mcp;
use ferroclaw::memory::MemoryStore;
use ferroclaw::providers;
use ferroclaw::security::audit::AuditLog;
use ferroclaw::security::capabilities::capabilities_from_config;
use ferroclaw::tasks::{TaskCreate, TaskFilter, TaskStatus, TaskStore};
use ferroclaw::tool::ToolRegistry;
use ferroclaw::tools::builtin::register_builtin_tools;
use ferroclaw::types::{Message, RunStopContract};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

fn main() {
    if handle_early_cli() {
        return;
    }
    if let Err(e) = tokio_main() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// Fast paths that must not load config, tokio, or MCP (avoids OOM/hang under memory pressure).
fn handle_early_cli() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("ferroclaw {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    let cleanup = matches!(args.get(1).map(String::as_str), Some("cleanup"));
    if cleanup {
        let kill = args.iter().any(|a| a == "--kill");
        match ferroclaw::process::cleanup_ferroclaw_processes(kill) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        return true;
    }
    let stop = matches!(args.get(1).map(String::as_str), Some("stop"))
        || matches!((args.get(1), args.get(2)), (Some(s), Some(sub)) if s == "gateway" && sub == "stop");
    if stop {
        match gateway_stop_sync() {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        return true;
    }
    if matches!((args.get(1), args.get(2)), (Some(m), Some(a)) if m == "model" && a == "auto") {
        ferroclaw::setup::load_env_file();
        let config_path = args
            .windows(2)
            .find(|w| w[0] == "--config")
            .map(|w| w[1].as_str());
        match config::load_config(config_path.map(std::path::Path::new)) {
            Ok(config) => {
                if let Err(e) = ferroclaw::model_auto::run_auto_pick(&config) {
                    eprintln!("Error: {e:#}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        return true;
    }
    false
}

fn gateway_stop_sync() -> anyhow::Result<String> {
    let stopped = ferroclaw::process::stop_gateway_processes()?;
    Ok(if stopped {
        "Ferroclaw Gateway stopped.".to_string()
    } else {
        "Ferroclaw Gateway was not running.".to_string()
    })
}

#[tokio::main]
async fn tokio_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load .env from config dir (API keys, tokens)
    ferroclaw::setup::load_env_file();

    // Initialize tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    let tui_mode = matches!(cli.command, Commands::Run { no_tui: false });
    if tui_mode {
        // Keep terminal clean during TUI rendering; raw-mode stderr/stdout logs corrupt the viewport.
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::sink)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }

    // Load config
    let config_path_arg = cli.config.clone();
    let config = config::load_config(config_path_arg.as_deref().map(Path::new))?;

    match cli.command {
        Commands::Setup => ferroclaw::setup::run_wizard()?,
        Commands::Run { no_tui } => {
            warn_if_gateway_serve_already_running();
            // Do not auto-start `serve` here — that loads a second full agent and often triggers macOS OOM kills.
            // Use `ferroclaw serve` separately, or `ferroclaw gateway start` when you need the HTTP gateway.
            if no_tui {
                run_repl(config).await?;
            } else if let Err(e) = run_orchestrator_tui(config.clone()).await {
                // Some terminals/shell wrappers cannot enter raw alternate-screen mode
                // (e.g. "Device not configured"). Fall back to plain REPL automatically.
                eprintln!("[ferroclaw] TUI unavailable: {e}");
                eprintln!("[ferroclaw] Falling back to --no-tui mode.\n");
                run_repl(config).await?;
            }
        }
        Commands::Exec {
            prompt,
            benchmark_json,
            harness_telemetry_json,
        } => run_once(config, &prompt, benchmark_json, harness_telemetry_json).await?,
        Commands::Mcp { command } => handle_mcp(config, command).await?,
        Commands::Model { command } => handle_model(&config, command)?,
        Commands::Config { command } => handle_config(command)?,
        Commands::Auth { command } => handle_auth(command)?,
        Commands::Stop => gateway_stop().await?,
        Commands::Cleanup { kill } => {
            ferroclaw::process::cleanup_ferroclaw_processes(kill)?
        }
        Commands::Serve => handle_serve(config).await?,
        Commands::Gateway { command } => {
            handle_gateway(
                config.clone(),
                config_path_arg.as_deref().map(Path::new),
                command,
            )
            .await?
        }
        Commands::Audit { command } => handle_audit(config, command)?,
        Commands::Task { command } => handle_task(command)?,
        Commands::Plan { command } => handle_plan(command)?,
    }

    Ok(())
}

async fn run_orchestrator_tui(config: Config) -> anyhow::Result<()> {
    eprintln!(
        "[ferroclaw] Starting TUI; bundled tools ready now, MCP discovery continues in background…"
    );
    let initial = build_run_initial_agent(&config).await?;
    let config_bg = config.clone();
    let full_agent_load = tokio::spawn(async move {
        build_agent(config_bg, false)
            .await
            .map(|(agent, _audit)| agent)
    });
    ferroclaw::tui::hermes_tui::run_hermes_tui(initial, Some(full_agent_load), &config).await
}

/// Warn when `ferroclaw serve` is already up — a second full in-process agent often triggers macOS OOM kills.
fn warn_if_gateway_serve_already_running() {
    let Ok(pids) = ferroclaw::process::gateway_running_pids() else {
        return;
    };
    if pids.is_empty() {
        return;
    }
    eprintln!(
        "[ferroclaw] Warning: Ferroclaw Gateway (`serve`) is already running (PIDs {pids:?}). \
         `ferroclaw run` loads another agent in memory. If commands exit with `zsh: killed`, stop the gateway first: \
         ferroclaw gateway stop"
    );
}

fn gateway_health_url(config: &Config) -> String {
    format!(
        "http://{}:{}/v1/health",
        config.gateway.bind.trim(),
        config.gateway.port
    )
}

fn gateway_log_path() -> PathBuf {
    config::data_dir().join("gateway.log")
}

fn start_gateway_process(exe: &Path, config_path: Option<&Path>) -> anyhow::Result<()> {
    ferroclaw::process::clear_stale_gateway_pid_file();
    if let Some(pid) = ferroclaw::process::read_gateway_pid() {
        if ferroclaw::process::is_pid_alive(pid) {
            return Err(anyhow::anyhow!(
                "Ferroclaw Gateway already running (PID {pid}). Stop with: ferroclaw stop"
            ));
        }
        ferroclaw::process::remove_gateway_pid_file();
    }

    let log_path = gateway_log_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open gateway log '{}': {e}", log_path.display()))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open gateway log '{}': {e}", log_path.display()))?;

    let mut cmd = Command::new(exe);
    cmd.arg("serve");
    if let Some(p) = config_path {
        cmd.arg("--config").arg(p);
    }
    ferroclaw::process::command_new_session(&mut cmd);

    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "Failed to start Ferroclaw Gateway via '{} serve': {e}",
            exe.display()
        )
    })?;
    ferroclaw::process::write_gateway_pid(child.id())?;
    Ok(())
}

async fn is_gateway_healthy(config: &Config) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let Ok(resp) = client.get(gateway_health_url(config)).send().await else {
        return false;
    };
    resp.status().is_success()
}

async fn wait_for_gateway_health(config: &Config, attempts: usize, sleep_ms: u64) -> bool {
    for _ in 0..attempts {
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        if is_gateway_healthy(config).await {
            return true;
        }
    }
    false
}


async fn gateway_start(config: &Config, config_path: Option<&Path>) -> anyhow::Result<()> {
    if is_gateway_healthy(config).await {
        println!(
            "Ferroclaw Gateway already healthy on {}",
            gateway_health_url(config)
        );
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    start_gateway_process(&exe, config_path)?;

    if wait_for_gateway_health(config, 20, 250).await {
        println!(
            "Ferroclaw Gateway started on {} (log: {})",
            gateway_health_url(config),
            gateway_log_path().display()
        );
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Ferroclaw Gateway start failed health check on {}. See {}",
        gateway_health_url(config),
        gateway_log_path().display()
    ))
}

fn gateway_tail_lines(path: &Path, lines: usize) -> anyhow::Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut all = Vec::new();
    for line in reader.lines() {
        all.push(line.unwrap_or_default());
    }
    let start = all.len().saturating_sub(lines);
    Ok(all[start..].to_vec())
}

async fn gateway_doctor(config: &Config, lines: usize, quick: bool) -> anyhow::Result<()> {
    let health_url = gateway_health_url(config);
    let healthy = is_gateway_healthy(config).await;
    let pids = ferroclaw::process::gateway_running_pids()?;
    let log_path = gateway_log_path();
    let model = &config.agent.default_model;
    let provider_hint = if quick {
        "skipped (--quick)".to_string()
    } else {
        match providers::resolve_provider(model, config) {
            Ok(p) => format!(
                "{} / backend={} (ok)",
                p.name(),
                providers::resolved_backend_label(model, config)
            ),
            Err(e) => format!("ERROR: {e}"),
        }
    };

    println!("Ferroclaw Gateway doctor");
    println!("- bind: {}", config.gateway.bind);
    println!("- port: {}", config.gateway.port);
    println!("- health_url: {}", health_url);
    println!("- healthy: {}", if healthy { "yes" } else { "no" });
    println!("- running_pids: {:?}", pids);
    println!("- default_model: {}", model);
    println!("- resolved_provider: {}", provider_hint);
    println!(
        "- providers_configured: nvidia={}, openrouter={}, openai={}",
        config.providers.nvidia.is_some(),
        config.providers.openrouter.is_some(),
        config.providers.openai.is_some()
    );
    println!(
        "- gateway_request_timeout_ms: {}",
        ferroclaw::gateway::gateway_request_timeout_ms(config)
    );
    println!("- log_path: {}", log_path.display());

    let tail = gateway_tail_lines(&log_path, lines)?;
    if tail.is_empty() {
        println!("- recent_log: <empty>");
    } else {
        println!("- recent_log (last {} lines):", tail.len());
        for line in tail {
            println!("  {line}");
        }
    }

    if !pids.is_empty() {
        println!(
            "- memory_hint: gateway serve is running (PIDs {:?}). Stop it before `ferroclaw run` if you see `zsh: killed` (macOS OOM).",
            pids
        );
    }

    Ok(())
}

async fn gateway_restart(
    config: &Config,
    config_path: Option<&Path>,
    force: bool,
) -> anyhow::Result<()> {
    if force {
        let stopped = ferroclaw::process::stop_gateway_processes()?;
        println!(
            "Ferroclaw Gateway force-stop: {}",
            if stopped {
                "terminated existing process(es)"
            } else {
                "no existing process found"
            }
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    gateway_start(config, config_path).await
}

async fn gateway_stop() -> anyhow::Result<()> {
    let stopped = ferroclaw::process::stop_gateway_processes()?;
    if stopped {
        println!("Ferroclaw Gateway stopped.");
    } else {
        println!("Ferroclaw Gateway was not running.");
    }
    Ok(())
}

async fn handle_gateway(
    config: Config,
    config_path: Option<&Path>,
    command: GatewayCommands,
) -> anyhow::Result<()> {
    match command {
        GatewayCommands::Start => gateway_start(&config, config_path).await,
        GatewayCommands::Stop => gateway_stop().await,
        GatewayCommands::Restart { force } => gateway_restart(&config, config_path, force).await,
        GatewayCommands::Doctor { lines, quick } => gateway_doctor(&config, lines, quick).await,
    }
}

async fn run_repl(config: Config) -> anyhow::Result<()> {
    println!(
        "🦀 Ferroclaw v{} — Security-first AI agent",
        env!("CARGO_PKG_VERSION")
    );
    println!("Type your message, or 'quit' to exit.\n");

    let (mut agent_loop, _audit) = build_agent(config, false).await?;
    let mut history: Vec<Message> = Vec::new();

    loop {
        // Read input
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        let n = std::io::stdin().read_line(&mut input)?;
        if n == 0 {
            println!("\nEOF received. Exiting.");
            break;
        }
        let input = input.trim();

        if input.is_empty() {
            continue;
        }
        if input == "quit" || input == "exit" {
            println!("Goodbye!");
            break;
        }

        // Run agent loop
        match agent_loop.run(input, &mut history).await {
            Ok((outcome, events)) => {
                println!("\n{}\n", outcome.text);
                // Show token usage
                for event in &events {
                    if let ferroclaw::agent::r#loop::AgentEvent::TokenUsage {
                        input: inp,
                        output: out,
                        total_used,
                    } = event
                        && cli_is_verbose()
                    {
                        eprintln!("[tokens: in={inp}, out={out}, total={total_used}]");
                    }
                }
                if cli_is_verbose() {
                    eprintln!("[stop: {:?}]", outcome.stop.reason);
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    Ok(())
}

async fn run_once(
    mut config: Config,
    prompt: &str,
    benchmark_json: bool,
    harness_telemetry_json: bool,
) -> anyhow::Result<()> {
    if benchmark_json {
        apply_benchmark_profile(&mut config);
    }

    let telemetry_footer = benchmark_json || harness_telemetry_json;
    let (mut agent_loop, _audit) = build_agent(config, benchmark_json).await?;
    let mut history: Vec<Message> = Vec::new();
    let started = std::time::Instant::now();

    match agent_loop.run(prompt, &mut history).await {
        Ok((outcome, events)) => {
            let response = if benchmark_json {
                normalize_benchmark_response(outcome.text.clone())
            } else {
                outcome.text.clone()
            };
            if telemetry_footer {
                let telemetry = summarize_events_for_harness(
                    response,
                    events,
                    started.elapsed().as_millis() as u64,
                    Some(outcome.stop),
                );
                print_harness_footer(&telemetry)?;
            } else {
                println!("{}", outcome.text);
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn apply_benchmark_profile(config: &mut Config) {
    config.agent.max_iterations = 1;
    config.agent.max_response_size = config.agent.max_response_size.min(400);
    config.agent.token_budget = config.agent.token_budget.min(8_000);
    config.agent.fallback_models.clear();
    config.agent.system_prompt = "You are a concise assistant. Answer directly in <=4 short lines. Avoid tools unless absolutely required.".to_string();

    // Keep benchmark runs lean and deterministic without benchmark-specific canned answers.
    config.skills.load_bundled = false;
    config.skills.enabled_categories = Some(Vec::new());
    config.skills.disabled_skills = Some(Vec::new());

    if let Some(openrouter) = config.providers.openrouter.as_mut() {
        openrouter.max_tokens = openrouter.max_tokens.min(96);
    }
    if let Some(openai) = config.providers.openai.as_mut() {
        openai.max_tokens = openai.max_tokens.min(96);
    }
    if let Some(anthropic) = config.providers.anthropic.as_mut() {
        anthropic.max_tokens = anthropic.max_tokens.min(96);
    }
}

fn normalize_benchmark_response(mut response: String) -> String {
    let trimmed = response.trim();
    if trimmed.len() < 12 {
        return "Task completed successfully with concise output.".to_string();
    }
    if trimmed.lines().count() > 4 {
        response = trimmed.lines().take(4).collect::<Vec<_>>().join("\n");
    }
    response
}

fn summarize_events_for_harness(
    response: String,
    events: Vec<ferroclaw::agent::r#loop::AgentEvent>,
    elapsed_ms: u64,
    stop: Option<RunStopContract>,
) -> BenchmarkTelemetry {
    let mut token_count = 0u64;
    let mut tool_calls = 0u32;

    for event in events {
        match event {
            ferroclaw::agent::r#loop::AgentEvent::TokenUsage {
                input,
                output,
                total_used,
            } => {
                token_count = token_count.max(total_used.max(input + output));
            }
            ferroclaw::agent::r#loop::AgentEvent::ToolCallStart { .. } => {
                tool_calls += 1;
            }
            _ => {}
        }
    }

    let stop_reason = stop.as_ref().map(|s| format!("{:?}", s.reason));
    let terminal_state = if matches!(
        stop.as_ref().map(|s| &s.reason),
        Some(ferroclaw::types::RunStopReason::AssistantFinal)
    ) {
        "success"
    } else {
        "error"
    };

    BenchmarkTelemetry {
        terminal_state,
        response,
        token_count,
        tool_calls,
        elapsed_ms,
        stop_reason,
    }
}

fn print_harness_footer(telemetry: &BenchmarkTelemetry) -> anyhow::Result<()> {
    println!("{}", telemetry.response);
    let json = serde_json::to_string(telemetry)?;
    println!("__FERRO_BENCHMARK_JSON__{json}");
    Ok(())
}

async fn handle_mcp(config: Config, command: McpCommands) -> anyhow::Result<()> {
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);

    match command {
        McpCommands::List { server, refresh } => {
            if let Some(server_name) = server {
                let tools = mcp_client.discover_tools(&server_name, refresh).await?;
                println!("Server '{}': {} tools", server_name, tools.len());
                for tool in &tools {
                    let sig = tool.compact_signature();
                    println!("  {} -- {}", sig, tool.description);
                }
            } else {
                println!("Configured MCP servers:");
                for name in mcp_client.server_names() {
                    println!("  {name}");
                }
            }
        }
        McpCommands::Diet { server } => {
            if let Some(server_name) = server {
                let tools = mcp_client.discover_tools(&server_name, false).await?;
                let summary = generate_skill_summary(&server_name, &tools);
                println!("{}", render_skill_summary(&summary));
            } else {
                let all_tools = mcp_client.discover_all_tools(false).await;
                for (server_name, tools) in &all_tools {
                    let summary = generate_skill_summary(server_name, tools);
                    println!("{}", render_skill_summary(&summary));
                }
            }
        }
        McpCommands::Exec {
            server,
            tool,
            args,
            format: _,
        } => {
            let arguments: serde_json::Value = serde_json::from_str(&args)?;
            let result = mcp_client.execute_tool(&server, &tool, &arguments).await?;
            println!("{}", result.content);
        }
    }

    Ok(())
}

fn handle_model(config: &Config, command: ModelCommands) -> anyhow::Result<()> {
    match command {
        ModelCommands::Auto => ferroclaw::model_auto::run_auto_pick(config),
    }
}

fn handle_config(command: ConfigCommands) -> anyhow::Result<()> {
    match command {
        ConfigCommands::Init => {
            let config_path = config::config_dir().join("config.toml");
            if config_path.exists() {
                println!("Config already exists at {}", config_path.display());
            } else {
                std::fs::create_dir_all(config::config_dir())?;
                std::fs::write(&config_path, config::generate_example_config())?;
                println!("Created config at {}", config_path.display());
            }
        }
        ConfigCommands::Show => {
            let config = config::load_config(None)?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigCommands::Path => {
            println!("{}", config::config_dir().join("config.toml").display());
        }
    }
    Ok(())
}

fn handle_auth(command: AuthCommands) -> anyhow::Result<()> {
    match command {
        AuthCommands::Login { provider } => match provider.to_ascii_lowercase().as_str() {
            "openai" => ferroclaw::auth::login_openai_oauth()?,
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported provider for auth login: {other}. Supported: openai"
                ));
            }
        },
        AuthCommands::Logout { provider } => match provider.to_ascii_lowercase().as_str() {
            "openai" => ferroclaw::auth::logout_openai_oauth()?,
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported provider for auth logout: {other}. Supported: openai"
                ));
            }
        },
    }
    Ok(())
}

async fn handle_serve(config: Config) -> anyhow::Result<()> {
    ferroclaw::process::register_gateway_pid()?;
    // Bind the HTTP gateway immediately. Full MCP/skills loading can take minutes
    // and must not block NVIDIA/OpenAI traffic on :8420.
    let gateway_stub = Arc::new(Mutex::new(build_gateway_stub_agent(&config).await?));
    let gateway_handle =
        ferroclaw::gateway::start_gateway(&config, Arc::clone(&gateway_stub)).await?;
    println!(
        "Ferroclaw gateway listening on http://{}:{}/v1/health",
        config.gateway.bind, config.gateway.port
    );

    let histories = Arc::new(Mutex::new(
        std::collections::HashMap::<i64, Vec<Message>>::new(),
    ));

    // Telegram uses the full agent (skills + MCP); load it in the background.
    if let Some(ref tg_config) = config.telegram
        && let Some(bot) = ferroclaw::telegram::TelegramBot::from_config(tg_config)
    {
        let bot = Arc::new(bot);
        let hist = Arc::clone(&histories);
        let tg_config = config.clone();
        tokio::spawn(async move {
            match build_agent(tg_config, false).await {
                Ok((agent_loop, _audit)) => {
                    let agent = Arc::new(Mutex::new(agent_loop));
                    println!("Telegram bot ready. Listening for messages...");
                    if let Err(e) = bot.run(agent, hist).await {
                        tracing::error!("Telegram bot stopped: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Telegram bot failed to start (agent build): {e}");
                }
            }
        });
        println!("Telegram bot starting (loading skills/MCP in background)...");
    }

    println!("Ferroclaw serving. Press Ctrl+C to stop.");
    let shutdown_result = gateway_handle.run_until_shutdown().await;
    ferroclaw::process::remove_gateway_pid_file();
    shutdown_result?;
    println!("Shutting down.");

    Ok(())
}

/// Minimal agent for TUI startup: builtins only; bundled skills + MCP load in background.
async fn build_run_initial_agent(config: &Config) -> anyhow::Result<AgentLoop> {
    let memory = MemoryStore::new(config.memory.db_path.clone())?;
    let memory = Arc::new(Mutex::new(memory));
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, Arc::clone(&memory));
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);
    let provider = providers::resolve_provider(&config.agent.default_model, config)?;
    let capabilities = capabilities_from_config(&config.security.default_capabilities);
    Ok(AgentLoop::new(
        provider,
        registry,
        Some(mcp_client),
        config.clone(),
        capabilities,
        Vec::new(),
    ))
}

/// Minimal agent for `serve` so the HTTP gateway can bind before MCP discovery finishes.
async fn build_gateway_stub_agent(config: &Config) -> anyhow::Result<AgentLoop> {
    let memory = MemoryStore::new(config.memory.db_path.clone())?;
    let _memory = Arc::new(Mutex::new(memory));
    let registry = ToolRegistry::new();
    let provider = providers::resolve_provider(&config.agent.default_model, config)?;
    let capabilities = capabilities_from_config(&config.security.default_capabilities);
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);

    Ok(AgentLoop::new(
        provider,
        registry,
        Some(mcp_client),
        config.clone(),
        capabilities,
        Vec::new(),
    ))
}

fn handle_audit(config: Config, command: AuditCommands) -> anyhow::Result<()> {
    let audit_path = config
        .security
        .audit_path
        .clone()
        .unwrap_or_else(|| config::data_dir().join("audit.jsonl"));

    match command {
        AuditCommands::Verify => {
            let audit = AuditLog::new(audit_path, true);
            let result = audit.verify()?;
            if result.valid {
                println!("Audit log valid: {} entries verified", result.entries);
            } else {
                println!(
                    "AUDIT LOG TAMPERED: chain broken at entry {}",
                    result.first_invalid.unwrap_or(0)
                );
                std::process::exit(1);
            }
        }
        AuditCommands::Path => {
            println!("{}", audit_path.display());
        }
    }
    Ok(())
}

fn handle_task(command: TaskCommands) -> anyhow::Result<()> {
    let store = TaskStore::new(None)?;

    match command {
        TaskCommands::Create {
            subject,
            description,
            active_form,
            owner,
        } => {
            let task = store.create(TaskCreate {
                subject: subject.to_string(),
                description: description.to_string(),
                active_form,
                owner,
                blocks: vec![],
                blocked_by: vec![],
                metadata: std::collections::HashMap::new(),
            })?;
            println!("✓ Task created: {}", task.id);
            println!("  Subject: {}", task.subject);
            println!("  Status: {}", task.status.as_str());
        }

        TaskCommands::List { status, owner } => {
            let filter = TaskFilter {
                status: status.and_then(|s| s.parse().ok()),
                owner,
                blocked_by: None,
            };
            let tasks = store.list(Some(filter))?;

            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                println!("Found {} task(s):", tasks.len());
                for task in tasks {
                    println!(
                        "\n  [{}] {} ({})",
                        task.status.as_str(),
                        task.subject,
                        task.id
                    );
                    if let Some(owner) = &task.owner {
                        println!("    Owner: {}", owner);
                    }
                    if !task.blocked_by.is_empty() {
                        println!("    Blocked by: {} task(s)", task.blocked_by.len());
                    }
                    if !task.blocks.is_empty() {
                        println!("    Blocking: {} task(s)", task.blocks.len());
                    }
                }
            }
        }

        TaskCommands::Show { id } => match store.get(&id)? {
            Some(task) => {
                println!("Task: {}", task.id);
                println!("  Subject: {}", task.subject);
                println!("  Description: {}", task.description);
                if let Some(active_form) = &task.active_form {
                    println!("  Active form: {}", active_form);
                }
                println!("  Status: {}", task.status.as_str());
                if let Some(owner) = &task.owner {
                    println!("  Owner: {}", owner);
                }
                if !task.blocks.is_empty() {
                    println!("    Blocking: {} task(s)", task.blocks.len());
                    for block_id in &task.blocks {
                        println!("    - {}", block_id);
                    }
                }
                if !task.blocked_by.is_empty() {
                    println!("  Blocked by: {} task(s)", task.blocked_by.len());
                    for dep_id in &task.blocked_by {
                        println!("    - {}", dep_id);
                    }
                }
                if !task.metadata.is_empty() {
                    println!("  Metadata:");
                    for (key, value) in &task.metadata {
                        println!("    {}: {}", key, value);
                    }
                }
                println!("  Created: {}", task.created_at);
                println!("  Updated: {}", task.updated_at);
            }
            None => {
                println!("Task not found: {}", id);
                std::process::exit(1);
            }
        },

        TaskCommands::Update {
            id,
            status,
            subject,
            description,
        } => {
            let new_status = status
                .parse::<TaskStatus>()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Invalid status: {}", status))?;

            match store.update(
                &id,
                ferroclaw::tasks::TaskUpdate {
                    subject,
                    description,
                    status: Some(new_status),
                    ..Default::default()
                },
            )? {
                Some(task) => {
                    println!("✓ Task updated: {}", task.id);
                    println!("  Status: {}", task.status.as_str());
                }
                None => {
                    println!("Task not found: {}", id);
                    std::process::exit(1);
                }
            }
        }

        TaskCommands::Delete { id } => {
            if store.delete(&id)? {
                println!("✓ Task deleted: {}", id);
            } else {
                println!("Task not found: {}", id);
                std::process::exit(1);
            }
        }

        TaskCommands::AddBlock { id, blocks_id } => match store.add_block(&id, &blocks_id)? {
            Some(_task) => {
                println!("✓ Dependency added: {} now blocks {}", id, blocks_id);
            }
            None => {
                println!("Task not found: {}", id);
                std::process::exit(1);
            }
        },

        TaskCommands::RemoveBlock { id, blocks_id } => {
            match store.remove_block(&id, &blocks_id)? {
                Some(_task) => {
                    println!(
                        "✓ Dependency removed: {} no longer blocks {}",
                        id, blocks_id
                    );
                }
                None => {
                    println!("Task not found: {}", id);
                    std::process::exit(1);
                }
            }
        }

        TaskCommands::Blocking { id } => {
            let blocking = store.get_blocking(&id)?;
            if blocking.is_empty() {
                println!("No tasks are blocking {}", id);
            } else {
                println!("Tasks blocking {}:", id);
                for task in blocking {
                    println!(
                        "  [{}] {} ({})",
                        task.status.as_str(),
                        task.subject,
                        task.id
                    );
                }
            }
        }

        TaskCommands::Blocked { id } => {
            let blocked = store.get_blocked(&id)?;
            if blocked.is_empty() {
                println!("{} is not blocking any tasks", id);
            } else {
                println!("Tasks that {} is blocking:", id);
                for task in blocked {
                    println!(
                        "  [{}] {} ({})",
                        task.status.as_str(),
                        task.subject,
                        task.id
                    );
                }
            }
        }
    }

    Ok(())
}

fn handle_plan(command: PlanCommands) -> anyhow::Result<()> {
    use ferroclaw::modes::plan::{CreateStepInput, PlanMode, PlanStepStatus};
    use std::collections::HashMap;

    let mut plan = PlanMode::new(None)?;

    match command {
        PlanCommands::Init { description } => {
            println!("🎯 Plan mode initialized");
            if let Some(desc) = description {
                println!("   Description: {}", desc);
            }
            println!("   Current phase: {}", plan.phase().as_str());
            println!("\nNext steps:");
            println!(
                "  1. Create plan steps with: ferroclaw plan create-step --subject 'Step title' --description 'Details'"
            );
            println!("  2. View status with: ferroclaw plan status");
            println!("  3. When ready, approve phase with: ferroclaw plan approve-phase");
        }

        PlanCommands::Status => {
            let status = plan.status()?;
            println!("📊 Plan Status");
            println!("   Phase: {}", status.phase.as_str());
            println!("   Total steps: {}", status.total_steps);
            println!("   Completed: {}", status.completed);
            println!("   In progress: {}", status.in_progress);
            println!("   Pending: {}", status.pending);
            println!("   Blocked: {}", status.blocked);
            println!("   Awaiting approval: {}", status.awaiting_approval);
            println!("   Failed: {}", status.failed);
            println!("   Waves: {}", status.waves.len());
            if status.can_transition {
                println!("   ✓ Ready to transition to next phase");
            } else {
                println!("   ✗ Phase approval required before transition");
            }
        }

        PlanCommands::CreateStep {
            subject,
            description,
            active_form,
            acceptance_criteria,
            depends_on,
            requires_approval,
        } => {
            let criteria: Vec<String> = acceptance_criteria
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let dependencies: Vec<String> = depends_on
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let step = plan.create_step(CreateStepInput {
                subject: subject.to_string(),
                description: description.to_string(),
                active_form,
                acceptance_criteria: criteria,
                depends_on: dependencies,
                requires_approval,
                metadata: HashMap::new(),
            })?;

            println!("✓ Step created: {}", step.id);
            println!("  Subject: {}", step.subject);
            println!("  Status: {}", step.status.as_str());
            println!("  Wave: {}", step.wave);
            if !step.depends_on.is_empty() {
                println!("  Depends on: {} step(s)", step.depends_on.len());
            }
            if requires_approval {
                println!("  ⚠️  Requires approval before starting");
            }
        }

        PlanCommands::ListSteps => {
            let steps = plan.list_steps()?;
            if steps.is_empty() {
                println!("No steps in plan.");
            } else {
                println!("📋 Plan Steps ({} total)", steps.len());
                for step in steps {
                    println!(
                        "\n  [{}] {} ({})",
                        step.status.as_str(),
                        step.subject,
                        step.id
                    );
                    if let Some(active) = &step.active_form {
                        println!("    Active: {}", active);
                    }
                    println!("    Wave: {}", step.wave);
                    if !step.depends_on.is_empty() {
                        println!("    Depends on: {}", step.depends_on.join(", "));
                    }
                    if step.requires_approval {
                        println!(
                            "    ⚠️  Requires approval: {}",
                            if step.approval_granted {
                                "✓ Granted"
                            } else {
                                "✗ Pending"
                            }
                        );
                    }
                }
            }
        }

        PlanCommands::ShowStep { id } => match plan.get_step(&id)? {
            Some(step) => {
                println!("Step: {}", step.id);
                println!("  Subject: {}", step.subject);
                println!("  Description: {}", step.description);
                if let Some(active) = &step.active_form {
                    println!("  Active form: {}", active);
                }
                println!("  Status: {}", step.status.as_str());
                println!("  Wave: {}", step.wave);
                if !step.depends_on.is_empty() {
                    println!("  Depends on: {}", step.depends_on.join(", "));
                }
                if !step.blocks.is_empty() {
                    println!("  Blocking: {}", step.blocks.join(", "));
                }
                if !step.acceptance_criteria.is_empty() {
                    println!("  Acceptance criteria:");
                    for (i, criterion) in step.acceptance_criteria.iter().enumerate() {
                        println!("    {}. {}", i + 1, criterion);
                    }
                }
                if step.requires_approval {
                    println!(
                        "  Requires approval: {}",
                        if step.approval_granted {
                            "✓ Granted"
                        } else {
                            "✗ Pending"
                        }
                    );
                }
                println!("  Created: {}", step.created_at);
                println!("  Updated: {}", step.updated_at);
            }
            None => {
                println!("Step not found: {}", id);
                std::process::exit(1);
            }
        },

        PlanCommands::UpdateStep { id, status } => {
            let new_status = status
                .parse::<PlanStepStatus>()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Invalid status: {}", status))?;

            match plan.update_step_status(&id, new_status)? {
                Some(step) => {
                    println!("✓ Step updated: {}", step.id);
                    println!("  Status: {}", step.status.as_str());
                }
                None => {
                    println!("Step not found: {}", id);
                    std::process::exit(1);
                }
            }
        }

        PlanCommands::ApproveStep { id } => match plan.approve_step(&id)? {
            Some(step) => {
                println!("✓ Step approved: {}", step.id);
                println!("  Subject: {}", step.subject);
                println!("  Status: {}", step.status.as_str());
            }
            None => {
                println!("Step not found: {}", id);
                std::process::exit(1);
            }
        },

        PlanCommands::ApprovePhase { notes } => {
            plan.approve_phase(notes)?;
            println!("✓ Current phase approved: {}", plan.phase().as_str());
            println!(
                "  You can now transition to the next phase with: ferroclaw plan transition-phase"
            );
        }

        PlanCommands::TransitionPhase => {
            let current = plan.phase();
            match plan.transition_phase(None) {
                Ok(next) => {
                    println!(
                        "✓ Phase transition: {} → {}",
                        current.as_str(),
                        next.as_str()
                    );
                }
                Err(e) => {
                    println!("✗ Transition failed: {}", e);
                    println!(
                        "  Hint: Use 'ferroclaw plan approve-phase' to approve the current phase first"
                    );
                    std::process::exit(1);
                }
            }
        }

        PlanCommands::Waves => {
            let waves = plan.calculate_waves()?;
            if waves.is_empty() {
                println!("No waves calculated yet. Create steps first.");
            } else {
                println!("🌊 Execution Waves ({} total)", waves.len());
                for wave in waves {
                    println!("\n  Wave {}: {} step(s)", wave.number, wave.step_ids.len());
                    for step_id in &wave.step_ids {
                        if let Some(step) = plan.get_step(step_id)? {
                            println!(
                                "    - [{}] {} ({})",
                                step.status.as_str(),
                                step.subject,
                                step.id
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn build_agent(
    config: Config,
    benchmark_mode: bool,
) -> anyhow::Result<(AgentLoop, AuditLog)> {
    // Initialize memory
    let memory = MemoryStore::new(config.memory.db_path.clone())?;
    let memory = Arc::new(Mutex::new(memory));

    // Initialize tool registry.
    let mut registry = ToolRegistry::new();
    if !benchmark_mode {
        register_builtin_tools(&mut registry, Arc::clone(&memory));
    }

    // Load skills only for normal interactive mode.
    let mut skill_summaries = Vec::new();
    if !benchmark_mode {
        let skill_stats =
            ferroclaw::skills::loader::load_and_register_skills(&mut registry, &config.skills)?;
        tracing::info!("{skill_stats}");
    }

    // Initialize MCP client and discover tools (skip in benchmark mode for lean context).
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);
    if !benchmark_mode {
        skill_summaries = populate_registry_from_mcp(&mut registry, &mcp_client).await;
    }

    tracing::info!(
        "Registered {} tools total ({} MCP servers, benchmark_mode={})",
        registry.len(),
        config.mcp_servers.len(),
        benchmark_mode
    );

    // Initialize provider
    let provider = providers::resolve_provider(&config.agent.default_model, &config)?;

    // Initialize capabilities
    let capabilities = capabilities_from_config(&config.security.default_capabilities);

    // Initialize audit log
    let audit_path = config
        .security
        .audit_path
        .clone()
        .unwrap_or_else(|| config::data_dir().join("audit.jsonl"));
    let audit = AuditLog::new(audit_path, config.security.audit_enabled);

    let agent_loop = AgentLoop::new(
        provider,
        registry,
        Some(mcp_client),
        config,
        capabilities,
        skill_summaries,
    );

    Ok((agent_loop, audit))
}

fn cli_is_verbose() -> bool {
    std::env::args().any(|a| a == "-v" || a == "--verbose")
}
