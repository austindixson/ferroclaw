use clap::Parser;
use ferroclaw::agent::AgentLoop;
use ferroclaw::cli::{AuditCommands, Cli, Commands, ConfigCommands, McpCommands};
use ferroclaw::config::{self, Config};
use ferroclaw::mcp::client::McpClient;
use ferroclaw::mcp::diet::{generate_skill_summary, render_skill_summary};
use ferroclaw::mcp::registry::populate_registry_from_mcp;
use ferroclaw::memory::MemoryStore;
use ferroclaw::providers;
use ferroclaw::security::audit::AuditLog;
use ferroclaw::security::capabilities::capabilities_from_config;
use ferroclaw::tool::ToolRegistry;
use ferroclaw::tools::builtin::register_builtin_tools;
use ferroclaw::types::Message;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load .env from config dir (API keys, tokens)
    ferroclaw::setup::load_env_file();

    // Initialize tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Load config
    let config = config::load_config(cli.config.as_deref().map(Path::new))?;

    match cli.command {
        Commands::Setup => ferroclaw::setup::run_wizard()?,
        Commands::Run { no_tui } => {
            if no_tui {
                run_repl(config).await?;
            } else {
                run_tui(config).await?;
            }
        }
        Commands::Exec { prompt } => run_once(config, &prompt).await?,
        Commands::Mcp { command } => handle_mcp(config, command).await?,
        Commands::Config { command } => handle_config(command)?,
        Commands::Serve => handle_serve(config).await?,
        Commands::Audit { command } => handle_audit(config, command)?,
    }

    Ok(())
}

async fn run_tui(config: Config) -> anyhow::Result<()> {
    let (agent_loop, _audit) = build_agent(config.clone()).await?;
    ferroclaw::tui::run_tui(agent_loop, &config).await
}

async fn run_repl(config: Config) -> anyhow::Result<()> {
    println!("🦀 Ferroclaw v{} — Security-first AI agent", env!("CARGO_PKG_VERSION"));
    println!("Type your message, or 'quit' to exit.\n");

    let (mut agent_loop, _audit) = build_agent(config).await?;
    let mut history: Vec<Message> = Vec::new();

    loop {
        // Read input
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
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
            Ok((response, events)) => {
                println!("\n{response}\n");
                // Show token usage
                for event in &events {
                    if let ferroclaw::agent::r#loop::AgentEvent::TokenUsage {
                        input: inp,
                        output: out,
                        total_used,
                    } = event
                    {
                        if cli_is_verbose() {
                            eprintln!("[tokens: in={inp}, out={out}, total={total_used}]");
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    Ok(())
}

async fn run_once(config: Config, prompt: &str) -> anyhow::Result<()> {
    let (mut agent_loop, _audit) = build_agent(config).await?;
    let mut history: Vec<Message> = Vec::new();

    match agent_loop.run(prompt, &mut history).await {
        Ok((response, _)) => {
            println!("{response}");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

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

async fn handle_serve(config: Config) -> anyhow::Result<()> {
    let (agent_loop, _audit) = build_agent(config.clone()).await?;
    let agent_loop = Arc::new(Mutex::new(agent_loop));
    let histories = Arc::new(Mutex::new(
        std::collections::HashMap::<i64, Vec<Message>>::new(),
    ));

    // Start Telegram bot if configured
    if let Some(ref tg_config) = config.telegram {
        if let Some(bot) = ferroclaw::telegram::TelegramBot::from_config(tg_config) {
            let bot = Arc::new(bot);
            let agent = Arc::clone(&agent_loop);
            let hist = Arc::clone(&histories);
            tokio::spawn(async move {
                if let Err(e) = bot.run(agent, hist).await {
                    tracing::error!("Telegram bot stopped: {e}");
                }
            });
            println!("Telegram bot started. Listening for messages...");
        }
    }

    // Start gateway
    ferroclaw::gateway::start_gateway(&config).await?;

    // Keep running (gateway is currently a stub, so just wait)
    println!("Ferroclaw serving. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down.");

    Ok(())
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

async fn build_agent(config: Config) -> anyhow::Result<(AgentLoop, AuditLog)> {
    // Initialize memory
    let memory = MemoryStore::new(config.memory.db_path.clone())?;
    let memory = Arc::new(Mutex::new(memory));

    // Initialize tool registry with built-in tools
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, Arc::clone(&memory));

    // Load bundled + custom skills
    let skill_stats = ferroclaw::skills::loader::load_and_register_skills(
        &mut registry,
        &config.skills,
    )?;
    tracing::info!("{skill_stats}");

    // Initialize MCP client and discover tools
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);
    let skill_summaries = populate_registry_from_mcp(&mut registry, &mcp_client).await;

    tracing::info!("Registered {} tools total ({} MCP servers)", registry.len(), config.mcp_servers.len());

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
