//! Interactive onboarding wizard.
//!
//! Fully automated: collects API keys, stores them in .env, verifies the
//! connection, and writes config.toml. User runs `ferroclaw run` immediately after.

use crate::config;
use crate::skills::manifest::SkillCategory;
use std::collections::HashMap;
use std::io::{self, Write};

/// Run the full onboarding wizard.
pub fn run_wizard() -> anyhow::Result<()> {
    clear_screen();
    print_banner();

    let config_path = config::config_dir().join("config.toml");
    let env_path = config::config_dir().join(".env");

    if config_path.exists() {
        println!("  A config already exists at:");
        println!("  {}\n", config_path.display());
        if !confirm("  Overwrite it?", false) {
            println!("\n  Kept existing config. Run `ferroclaw run` to start.");
            return Ok(());
        }
        println!();
    }

    let mut env_vars: Vec<(String, String)> = Vec::new();

    // ── Step 1: Provider + API Key ──────────────────────────────────────
    section_header("1/7", "LLM Provider");
    println!("  Which LLM provider will you use?\n");
    println!("    1) Anthropic   (Claude Sonnet, Opus, Haiku)");
    println!("    2) OpenAI      (GPT-4o, GPT-4o-mini)");
    println!("    3) Zai GLM     (GLM-5, GLM-4.5, GLM-4.6)");
    println!("    4) OpenRouter  (multi-model gateway)");
    println!("    5) NVIDIA      (NIM OpenAI-compatible API)");
    println!("    6) Multiple providers\n");

    let provider_choice = prompt_choice("  Select [1-6]", 1, 6, 1);

    let mut providers = ProviderSetup::default();

    match provider_choice {
        1 => providers.anthropic = setup_provider("Anthropic", "ANTHROPIC_API_KEY", &mut env_vars),
        2 => {
            providers.openai = setup_provider_with_url(
                "OpenAI",
                "OPENAI_API_KEY",
                "https://api.openai.com/v1",
                &mut env_vars,
            )
        }
        3 => providers.zai = setup_provider("Zai GLM", "ZAI_API_KEY", &mut env_vars),
        4 => {
            providers.openrouter = setup_provider("OpenRouter", "OPENROUTER_API_KEY", &mut env_vars)
        }
        5 => {
            providers.nvidia = setup_provider_with_url(
                "NVIDIA",
                "NVIDIA_API_KEY",
                "https://integrate.api.nvidia.com/v1",
                &mut env_vars,
            )
        }
        6 => {
            println!("\n  Configure each provider you want:\n");
            if confirm("    Anthropic?", true) {
                providers.anthropic =
                    setup_provider("Anthropic", "ANTHROPIC_API_KEY", &mut env_vars);
            }
            if confirm("    OpenAI?", false) {
                providers.openai = setup_provider_with_url(
                    "OpenAI",
                    "OPENAI_API_KEY",
                    "https://api.openai.com/v1",
                    &mut env_vars,
                );
            }
            if confirm("    OpenAI Codex (OAuth token)?", false) {
                providers.openai_codex = setup_provider_with_url(
                    "OpenAI Codex",
                    "OPENAI_OAUTH_TOKEN",
                    "https://chatgpt.com/backend-api/codex",
                    &mut env_vars,
                );
            }
            if confirm("    Google (Gemini OpenAI-compatible endpoint)?", false) {
                providers.google = setup_provider_with_url(
                    "Google",
                    "GEMINI_API_KEY",
                    "https://generativelanguage.googleapis.com/v1beta/openai",
                    &mut env_vars,
                );
            }
            if confirm("    xAI?", false) {
                providers.xai = setup_provider_with_url(
                    "xAI",
                    "XAI_API_KEY",
                    "https://api.x.ai/v1",
                    &mut env_vars,
                );
            }
            if confirm("    NVIDIA NIM (OpenAI-compatible endpoint)?", false) {
                providers.nvidia = setup_provider_with_url(
                    "NVIDIA",
                    "NVIDIA_API_KEY",
                    "https://integrate.api.nvidia.com/v1",
                    &mut env_vars,
                );
            }
            if confirm("    Zai GLM?", false) {
                providers.zai = setup_provider("Zai GLM", "ZAI_API_KEY", &mut env_vars);
            }
            if confirm("    llama.cpp (local OpenAI-compatible server)?", false) {
                providers.llamacpp = setup_provider_with_url(
                    "llama.cpp",
                    "LLAMACPP_API_KEY",
                    "http://127.0.0.1:8000/v1",
                    &mut env_vars,
                );
            }
            if confirm("    Mistral?", false) {
                providers.mistral = setup_provider_with_url(
                    "Mistral",
                    "MISTRAL_API_KEY",
                    "https://api.mistral.ai/v1",
                    &mut env_vars,
                );
            }
            if confirm("    Azure OpenAI?", false) {
                providers.azure_openai = setup_provider_with_url(
                    "Azure OpenAI",
                    "AZURE_OPENAI_API_KEY",
                    "https://<resource>.openai.azure.com/openai/v1",
                    &mut env_vars,
                );
            }
            if confirm("    GitHub Copilot?", false) {
                providers.github_copilot = setup_provider_with_url(
                    "GitHub Copilot",
                    "GITHUB_COPILOT_API_KEY",
                    "https://api.githubcopilot.com",
                    &mut env_vars,
                );
            }
            if confirm("    Google Vertex (OpenAI-compatible endpoint)?", false) {
                providers.google_vertex = setup_provider_with_url(
                    "Google Vertex",
                    "GOOGLE_VERTEX_API_KEY",
                    "https://aiplatform.googleapis.com/v1/projects/<project>/locations/<location>/endpoints/openapi",
                    &mut env_vars,
                );
            }
            if confirm("    Bedrock (OpenAI-compatible endpoint)?", false) {
                providers.bedrock = setup_provider_with_url(
                    "Bedrock",
                    "AWS_BEARER_TOKEN_BEDROCK",
                    "https://bedrock-runtime.<region>.amazonaws.com/openai/v1",
                    &mut env_vars,
                );
            }
            if confirm("    OpenRouter?", false) {
                providers.openrouter =
                    setup_provider("OpenRouter", "OPENROUTER_API_KEY", &mut env_vars);
            }
        }
        _ => providers.anthropic = setup_provider("Anthropic", "ANTHROPIC_API_KEY", &mut env_vars),
    }

    // ── Step 2: Default Model ───────────────────────────────────────────
    section_header("2/7", "Default Model");
    let default_model = pick_default_model(&providers);
    println!("  Default model: {default_model}\n");

    // ── Step 3: Security Profile ────────────────────────────────────────
    section_header("3/7", "Security Profile");
    println!("  How much tool access should the agent have?\n");
    println!("    1) Restricted   Read-only. No shell, no writes, no network.");
    println!("                    Capabilities: fs_read, memory_read\n");
    println!("    2) Standard     Read + web + memory. No shell or file writes.");
    println!(
        "                    Capabilities: fs_read, net_outbound, memory_read, memory_write\n"
    );
    println!("    3) Full         All capabilities enabled (for trusted environments).");
    println!("                    Capabilities: all 8 types\n");

    let security_choice = prompt_choice("  Select [1-3]", 1, 3, 2);

    let capabilities = match security_choice {
        1 => vec!["fs_read", "memory_read"],
        2 => vec!["fs_read", "net_outbound", "memory_read", "memory_write"],
        3 => vec![
            "fs_read",
            "fs_write",
            "net_outbound",
            "net_listen",
            "process_exec",
            "memory_read",
            "memory_write",
            "browser_control",
        ],
        _ => vec!["fs_read", "net_outbound", "memory_read", "memory_write"],
    };

    let profile_name = match security_choice {
        1 => "restricted",
        3 => "full",
        _ => "standard",
    };
    println!("  Security profile: {profile_name}\n");

    // ── Step 4: Skills ──────────────────────────────────────────────────
    section_header("4/7", "Skills");
    println!("  Ferroclaw bundles 84 skills across 16 categories.\n");
    println!("    1) All categories (recommended)");
    println!("    2) Choose categories");
    println!("    3) None (built-in tools + MCP only)\n");

    let skills_choice = prompt_choice("  Select [1-3]", 1, 3, 1);

    let enabled_categories: Option<Vec<String>> = match skills_choice {
        2 => Some(pick_skill_categories()),
        3 => {
            println!("  Skills disabled. Re-enable anytime in config.toml.\n");
            None
        }
        _ => {
            println!("  All 84 skills enabled.\n");
            None
        }
    };
    let load_bundled = skills_choice != 3;

    // ── Step 5: Messaging Channels ──────────────────────────────────────
    section_header("5/7", "Messaging Channels");
    println!("  Set up messaging channels? (Telegram, Discord, Slack, etc.)");
    println!("  You can always add these later in config.toml.\n");

    let mut channels = ChannelSetup::default();

    if confirm("  Configure messaging now?", false) {
        println!();
        if confirm("    Telegram?", false) {
            channels.telegram = setup_channel_telegram(&mut env_vars);
        }
        if confirm("    Discord?", false) {
            channels.discord = setup_channel_discord(&mut env_vars);
        }
        if confirm("    Slack?", false) {
            channels.slack = setup_channel_slack(&mut env_vars);
        }
        if confirm("    WhatsApp?", false) {
            channels.whatsapp = setup_channel_whatsapp(&mut env_vars);
        }
        if confirm("    Signal?", false) {
            channels.signal = setup_channel_signal();
        }
        if confirm("    Email?", false) {
            channels.email = setup_channel_email(&mut env_vars);
        }
        if confirm("    Home Assistant?", false) {
            channels.homeassistant = setup_channel_ha(&mut env_vars);
        }
    } else {
        println!("  Skipped. Add channels anytime in config.toml.\n");
    }

    // ── Step 6: MCP Servers ─────────────────────────────────────────────
    section_header("6/7", "MCP Servers");
    println!("  MCP servers extend Ferroclaw with external tools.");
    println!("  Common: filesystem, GitHub, Brave Search, Postgres.\n");

    let mut mcp_servers: Vec<McpEntry> = Vec::new();

    if confirm("  Add a filesystem MCP server? (recommended)", true) {
        let path = prompt_string("    Directory to expose", "/tmp");
        mcp_servers.push(McpEntry {
            name: "filesystem".into(),
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
                path,
            ],
            env: HashMap::new(),
        });
        println!();
    }

    if confirm("  Add a GitHub MCP server?", false) {
        let key = prompt_secret("    Paste your GitHub token");
        if !key.is_empty() {
            env_vars.push(("GITHUB_TOKEN".into(), key));
        }
        mcp_servers.push(McpEntry {
            name: "github".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
            env: HashMap::from([("GITHUB_TOKEN".into(), "${GITHUB_TOKEN}".into())]),
        });
        println!();
    }

    // ── Step 7: Summary & Write ─────────────────────────────────────────
    section_header("7/7", "Review & Install");

    let gateway_port = 8420u16;

    println!("  Provider(s):  {}", providers.summary());
    println!("  Model:        {default_model}");
    println!(
        "  Security:     {profile_name} ({} capabilities)",
        capabilities.len()
    );
    println!(
        "  Skills:       {}",
        if load_bundled {
            match &enabled_categories {
                Some(cats) => format!("{} categories", cats.len()),
                None => "all 84 (16 categories)".into(),
            }
        } else {
            "disabled".into()
        }
    );
    println!(
        "  Channels:     {}",
        if channels.any() {
            channels.summary()
        } else {
            "CLI + HTTP gateway".into()
        }
    );
    println!(
        "  MCP servers:  {}",
        if mcp_servers.is_empty() {
            "none".into()
        } else {
            mcp_servers
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!("  Gateway:      127.0.0.1:{gateway_port}");
    println!("  Audit log:    enabled");
    println!("  Secrets:      {} keys stored in .env", env_vars.len());
    println!();

    if !confirm("  Write config and .env?", true) {
        println!("\n  Aborted. No changes made.");
        return Ok(());
    }

    // ── Write files ─────────────────────────────────────────────────────
    std::fs::create_dir_all(config::config_dir())?;

    // Write .env
    if !env_vars.is_empty() {
        let env_content = generate_env_file(&env_vars);
        std::fs::write(&env_path, &env_content)?;
        // Restrict permissions: owner read/write only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600))?;
        }
    }

    // Write config.toml
    let toml = generate_config_toml(SetupTomlParams {
        providers: &providers,
        default_model: &default_model,
        capabilities: &capabilities,
        load_bundled,
        enabled_categories: &enabled_categories,
        channels: &channels,
        mcp_servers: &mcp_servers,
        gateway_port,
    });
    std::fs::write(&config_path, &toml)?;

    // ── Verify connection ───────────────────────────────────────────────
    println!();

    // Load the env vars we just wrote so the verification can use them
    for (key, value) in &env_vars {
        // SAFETY: called at startup before any threads are spawned.
        unsafe { std::env::set_var(key, value) };
    }

    print!("  Verifying API connection... ");
    let _ = io::stdout().flush();

    let verified = verify_api_key(&providers);
    if verified {
        println!("connected!");
    } else {
        println!("could not verify (check key later).");
    }

    // ── Done ────────────────────────────────────────────────────────────
    println!();
    divider();
    println!();
    println!("  Setup complete.");
    println!();
    println!("    {}", config_path.display());
    if !env_vars.is_empty() {
        println!("    {}  (chmod 600)", env_path.display());
    }
    println!();
    println!("  Start Ferroclaw:");
    println!();
    println!("    ferroclaw run");
    println!();
    println!("  One-shot mode:");
    println!();
    println!("    ferroclaw exec \"What files are in /tmp?\"");
    println!();
    divider();
    println!();

    Ok(())
}

// ── UI helpers ──────────────────────────────────────────────────────────────

fn clear_screen() {
    if atty_is_terminal() {
        print!("\x1B[2J\x1B[H");
        let _ = io::stdout().flush();
    }
}

fn atty_is_terminal() -> bool {
    unsafe { libc_isatty(1) != 0 }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> i32 {
    unsafe extern "C" {
        safe fn isatty(fd: i32) -> i32;
    }
    isatty(fd)
}

#[cfg(not(unix))]
unsafe fn libc_isatty(_fd: i32) -> i32 {
    1
}

fn print_banner() {
    println!();
    println!("  ███████╗███████╗██████╗ ██████╗  ██████╗  ██████╗██╗      █████╗ ██╗    ██╗");
    println!("  ██╔════╝██╔════╝██╔══██╗██╔══██╗██╔═══██╗██╔════╝██║     ██╔══██╗██║    ██║");
    println!("  █████╗  █████╗  ██████╔╝██████╔╝██║   ██║██║     ██║     ███████║██║ █╗ ██║");
    println!("  ██╔══╝  ██╔══╝  ██╔══██╗██╔══██╗██║   ██║██║     ██║     ██╔══██║██║███╗██║");
    println!("  ██║     ███████╗██║  ██║██║  ██║╚██████╔╝╚██████╗███████╗██║  ██║╚███╔███╔╝");
    println!("  ╚═╝     ╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝  ╚═════╝╚══════╝╚═╝  ╚═╝ ╚══╝╚══╝");
    println!();
    println!(
        "  v{}  —  Security-first AI agent",
        env!("CARGO_PKG_VERSION")
    );
    println!("  84 skills | 7 channels | 14 providers | DietMCP");
    println!();
    divider();
    println!();
}

fn divider() {
    println!("  ─────────────────────────────────────────────────");
}

fn section_header(step: &str, title: &str) {
    divider();
    println!("  [{step}]  {title}");
    divider();
    println!();
}

fn prompt_string(label: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    let _ = io::stdout().flush();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or_default();
    let input = input.trim();
    if input.is_empty() {
        default.to_string()
    } else {
        input.to_string()
    }
}

/// Prompt for a secret value (API key, token). Shows masked hint.
fn prompt_secret(label: &str) -> String {
    print!("{label}: ");
    let _ = io::stdout().flush();

    // Try to disable echo on unix
    #[cfg(unix)]
    let _guard = disable_echo();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or_default();

    #[cfg(unix)]
    {
        // Re-enable echo and print newline since enter wasn't echoed
        drop(_guard);
        println!();
    }

    let input = input.trim().to_string();

    if !input.is_empty() {
        let masked = if input.len() > 8 {
            format!("{}...{}", &input[..4], &input[input.len() - 4..])
        } else {
            "****".into()
        };
        println!("    Saved: {masked}");
    }

    input
}

/// RAII guard to disable/restore terminal echo on unix.
#[cfg(unix)]
struct EchoGuard {
    original: libc_termios,
}

#[cfg(unix)]
impl Drop for EchoGuard {
    fn drop(&mut self) {
        tcsetattr(0, 0, &self.original);
    }
}

#[cfg(unix)]
fn disable_echo() -> Option<EchoGuard> {
    unsafe {
        let mut term: libc_termios = std::mem::zeroed();
        if tcgetattr(0, &mut term) != 0 {
            return None;
        }
        let guard = EchoGuard { original: term };
        term.c_lflag &= !ECHO_FLAG;
        tcsetattr(0, 0, &term);
        Some(guard)
    }
}

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
struct libc_termios {
    c_iflag: u64,
    c_oflag: u64,
    c_cflag: u64,
    c_lflag: u64,
    c_cc: [u8; 20],
    c_ispeed: u64,
    c_ospeed: u64,
}

#[cfg(unix)]
const ECHO_FLAG: u64 = 0x00000008;

#[cfg(unix)]
unsafe extern "C" {
    safe fn tcgetattr(fd: i32, termios_p: *mut libc_termios) -> i32;
    safe fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const libc_termios) -> i32;
}

fn confirm(label: &str, default: bool) -> bool {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    print!("{label} {hint} ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or_default();
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        default
    } else {
        input.starts_with('y')
    }
}

fn prompt_choice(label: &str, min: u32, max: u32, default: u32) -> u32 {
    loop {
        print!("{label} [default: {default}]: ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or_default();
        let input = input.trim();
        if input.is_empty() {
            return default;
        }
        if let Ok(n) = input.parse::<u32>()
            && n >= min
            && n <= max
        {
            return n;
        }
        println!("  Please enter a number between {min} and {max}.");
    }
}

// ── Provider setup ──────────────────────────────────────────────────────────

#[derive(Default)]
struct ProviderSetup {
    anthropic: Option<ProviderEntry>,
    openai: Option<ProviderEntry>,
    openai_codex: Option<ProviderEntry>,
    google: Option<ProviderEntry>,
    xai: Option<ProviderEntry>,
    nvidia: Option<ProviderEntry>,
    zai: Option<ProviderEntry>,
    llamacpp: Option<ProviderEntry>,
    mistral: Option<ProviderEntry>,
    azure_openai: Option<ProviderEntry>,
    github_copilot: Option<ProviderEntry>,
    google_vertex: Option<ProviderEntry>,
    bedrock: Option<ProviderEntry>,
    openrouter: Option<ProviderEntry>,
}

struct ProviderEntry {
    env_var: String,
    base_url: Option<String>,
    models: Vec<String>,
}

impl ProviderSetup {
    fn summary(&self) -> String {
        let mut names = Vec::new();
        if self.anthropic.is_some() {
            names.push("Anthropic");
        }
        if self.openai.is_some() {
            names.push("OpenAI");
        }
        if self.openai_codex.is_some() {
            names.push("OpenAI Codex");
        }
        if self.google.is_some() {
            names.push("Google");
        }
        if self.xai.is_some() {
            names.push("xAI");
        }
        if self.nvidia.is_some() {
            names.push("NVIDIA");
        }
        if self.zai.is_some() {
            names.push("Zai GLM");
        }
        if self.llamacpp.is_some() {
            names.push("llama.cpp");
        }
        if self.mistral.is_some() {
            names.push("Mistral");
        }
        if self.azure_openai.is_some() {
            names.push("Azure OpenAI");
        }
        if self.github_copilot.is_some() {
            names.push("GitHub Copilot");
        }
        if self.google_vertex.is_some() {
            names.push("Google Vertex");
        }
        if self.bedrock.is_some() {
            names.push("Bedrock");
        }
        if self.openrouter.is_some() {
            names.push("OpenRouter");
        }
        if names.is_empty() {
            "none".into()
        } else {
            names.join(", ")
        }
    }
}

fn setup_provider(
    name: &str,
    env_var: &str,
    env_vars: &mut Vec<(String, String)>,
) -> Option<ProviderEntry> {
    println!();
    let key = prompt_secret(&format!("    Paste your {name} API key"));
    if key.is_empty() {
        println!("    Skipped (no key provided).\n");
        return None;
    }
    env_vars.push((env_var.into(), key));

    let models = match name {
        "Anthropic" => vec![
            "claude-sonnet-4-20250514".into(),
            "claude-opus-4-20250514".into(),
        ],
        "Zai GLM" => vec!["glm-5".into(), "glm-4.6".into()],
        "OpenRouter" => vec![
            "anthropic/claude-sonnet-4".into(),
            "openai/gpt-4o".into(),
            "x-ai/grok-4.20-multi-agent-beta".into(),
            "nvidia/nemotron-3-super-120b-a12b:free".into(),
            "minimax/minimax-m2.5:free".into(),
            "minimax/minimax-m2.5".into(),
            "moonshotai/kimi-k2.5".into(),
        ],
        _ => vec![],
    };

    Some(ProviderEntry {
        env_var: env_var.into(),
        base_url: None,
        models,
    })
}

fn setup_provider_with_url(
    name: &str,
    env_var: &str,
    default_url: &str,
    env_vars: &mut Vec<(String, String)>,
) -> Option<ProviderEntry> {
    println!();
    let key = prompt_secret(&format!("    Paste your {name} API key"));
    if key.is_empty() {
        println!("    Skipped (no key provided).\n");
        return None;
    }
    env_vars.push((env_var.into(), key));

    let base = prompt_string("    Base URL", default_url);
    // Always persist the chosen base URL, even when it's the provider default.
    // OpenAI-compatible provider configs deserialize with OpenAI defaults when
    // base_url is omitted, which can silently misroute non-OpenAI providers
    // (e.g. NVIDIA, Google, xAI) to api.openai.com.
    let base_url = Some(base);

    let models = match name {
        "OpenAI" => vec!["gpt-4o".into(), "gpt-4o-mini".into()],
        "OpenAI Codex" => vec![
            "gpt-5.4-mini".into(),
            "gpt-5.4".into(),
            "gpt-5.3-codex".into(),
            "gpt-5.2-codex".into(),
        ],
        "Google" => vec![
            "google:gemini-2.5-pro".into(),
            "google:gemini-2.5-flash".into(),
        ],
        "xAI" => vec!["xai:grok-4".into(), "xai:grok-3-mini".into()],
        "NVIDIA" => vec!["z-ai/glm4.7".into(), "meta/llama-3.1-70b-instruct".into()],
        "llama.cpp" => vec!["llamacpp:local-model".into()],
        "Mistral" => vec![
            "mistral:mistral-large-latest".into(),
            "mistral:mistral-small-latest".into(),
        ],
        "Azure OpenAI" => vec!["azure:gpt-4o".into()],
        "GitHub Copilot" => vec!["copilot:gpt-4o".into()],
        "Google Vertex" => vec!["vertex:gemini-2.5-pro".into()],
        "Bedrock" => vec!["bedrock:anthropic.claude-3-7-sonnet".into()],
        _ => vec!["gpt-4o".into()],
    };
    Some(ProviderEntry {
        env_var: env_var.into(),
        base_url,
        models,
    })
}

fn pick_default_model(providers: &ProviderSetup) -> String {
    let mut options: Vec<(String, String)> = Vec::new();
    if let Some(ref p) = providers.anthropic {
        for m in &p.models {
            options.push((m.clone(), "Anthropic".into()));
        }
    }
    if let Some(ref p) = providers.openai {
        for m in &p.models {
            options.push((m.clone(), "OpenAI".into()));
        }
    }
    if let Some(ref p) = providers.openai_codex {
        for m in &p.models {
            options.push((m.clone(), "OpenAI Codex".into()));
        }
    }
    if let Some(ref p) = providers.google {
        for m in &p.models {
            options.push((m.clone(), "Google".into()));
        }
    }
    if let Some(ref p) = providers.xai {
        for m in &p.models {
            options.push((m.clone(), "xAI".into()));
        }
    }
    if let Some(ref p) = providers.nvidia {
        for m in &p.models {
            options.push((m.clone(), "NVIDIA".into()));
        }
    }
    if let Some(ref p) = providers.zai {
        for m in &p.models {
            options.push((m.clone(), "Zai".into()));
        }
    }
    if let Some(ref p) = providers.llamacpp {
        for m in &p.models {
            options.push((m.clone(), "llama.cpp".into()));
        }
    }
    if let Some(ref p) = providers.mistral {
        for m in &p.models {
            options.push((m.clone(), "Mistral".into()));
        }
    }
    if let Some(ref p) = providers.azure_openai {
        for m in &p.models {
            options.push((m.clone(), "Azure OpenAI".into()));
        }
    }
    if let Some(ref p) = providers.github_copilot {
        for m in &p.models {
            options.push((m.clone(), "GitHub Copilot".into()));
        }
    }
    if let Some(ref p) = providers.google_vertex {
        for m in &p.models {
            options.push((m.clone(), "Google Vertex".into()));
        }
    }
    if let Some(ref p) = providers.bedrock {
        for m in &p.models {
            options.push((m.clone(), "Bedrock".into()));
        }
    }
    if let Some(ref p) = providers.openrouter {
        for m in &p.models {
            options.push((m.clone(), "OpenRouter".into()));
        }
    }
    if options.is_empty() {
        return "claude-sonnet-4-20250514".into();
    }
    if options.len() == 1 {
        println!("  Using: {} ({})", options[0].0, options[0].1);
        return options[0].0.clone();
    }
    println!("  Available models:\n");
    for (i, (model, provider)) in options.iter().enumerate() {
        println!("    {}) {model}  ({provider})", i + 1);
    }
    println!();
    let choice = prompt_choice("  Select default model", 1, options.len() as u32, 1);
    options[choice as usize - 1].0.clone()
}

// ── Channel setup ───────────────────────────────────────────────────────────

#[derive(Default)]
struct ChannelSetup {
    telegram: Option<TelegramEntry>,
    discord: Option<DiscordEntry>,
    slack: Option<SlackEntry>,
    whatsapp: Option<WhatsAppEntry>,
    signal: Option<SignalEntry>,
    email: Option<EmailEntry>,
    homeassistant: Option<HomeAssistantEntry>,
}

impl ChannelSetup {
    fn any(&self) -> bool {
        self.telegram.is_some()
            || self.discord.is_some()
            || self.slack.is_some()
            || self.whatsapp.is_some()
            || self.signal.is_some()
            || self.email.is_some()
            || self.homeassistant.is_some()
    }
    fn summary(&self) -> String {
        let mut n = Vec::new();
        if self.telegram.is_some() {
            n.push("Telegram");
        }
        if self.discord.is_some() {
            n.push("Discord");
        }
        if self.slack.is_some() {
            n.push("Slack");
        }
        if self.whatsapp.is_some() {
            n.push("WhatsApp");
        }
        if self.signal.is_some() {
            n.push("Signal");
        }
        if self.email.is_some() {
            n.push("Email");
        }
        if self.homeassistant.is_some() {
            n.push("Home Assistant");
        }
        n.join(", ")
    }
}

struct TelegramEntry;
struct DiscordEntry {
    prefix: String,
}
struct SlackEntry;
struct WhatsAppEntry {
    phone_number_id: String,
}
struct SignalEntry {
    api_url: String,
    phone_number: String,
}
struct EmailEntry {
    smtp_host: String,
    smtp_port: u16,
    from_address: String,
}
struct HomeAssistantEntry {
    api_url: String,
}

fn setup_channel_telegram(env_vars: &mut Vec<(String, String)>) -> Option<TelegramEntry> {
    println!();
    let key = prompt_secret("      Paste your Telegram bot token");
    if key.is_empty() {
        return None;
    }
    env_vars.push(("TELEGRAM_BOT_TOKEN".into(), key));
    Some(TelegramEntry)
}

fn setup_channel_discord(env_vars: &mut Vec<(String, String)>) -> Option<DiscordEntry> {
    println!();
    let key = prompt_secret("      Paste your Discord bot token");
    if key.is_empty() {
        return None;
    }
    env_vars.push(("DISCORD_BOT_TOKEN".into(), key));
    let prefix = prompt_string("      Command prefix", "!fc ");
    Some(DiscordEntry { prefix })
}

fn setup_channel_slack(env_vars: &mut Vec<(String, String)>) -> Option<SlackEntry> {
    println!();
    let bot = prompt_secret("      Paste your Slack bot token (xoxb-...)");
    if bot.is_empty() {
        return None;
    }
    env_vars.push(("SLACK_BOT_TOKEN".into(), bot));
    let app = prompt_secret("      Paste your Slack app token (xapp-...)");
    if !app.is_empty() {
        env_vars.push(("SLACK_APP_TOKEN".into(), app));
    }
    Some(SlackEntry)
}

fn setup_channel_whatsapp(env_vars: &mut Vec<(String, String)>) -> Option<WhatsAppEntry> {
    println!();
    let key = prompt_secret("      Paste your WhatsApp API token");
    if key.is_empty() {
        return None;
    }
    env_vars.push(("WHATSAPP_API_TOKEN".into(), key));
    let phone_id = prompt_string("      Phone number ID", "");
    if phone_id.is_empty() {
        println!("      Skipped (phone number ID required).");
        return None;
    }
    Some(WhatsAppEntry {
        phone_number_id: phone_id,
    })
}

fn setup_channel_signal() -> Option<SignalEntry> {
    println!();
    let url = prompt_string("      signal-cli REST API URL", "http://localhost:8080");
    let phone = prompt_string("      Registered phone number", "");
    if phone.is_empty() {
        println!("      Skipped (phone number required).");
        return None;
    }
    Some(SignalEntry {
        api_url: url,
        phone_number: phone,
    })
}

fn setup_channel_email(env_vars: &mut Vec<(String, String)>) -> Option<EmailEntry> {
    println!();
    let host = prompt_string("      SMTP host", "smtp.gmail.com");
    let port: u16 = prompt_string("      SMTP port", "587")
        .parse()
        .unwrap_or(587);
    let user = prompt_string("      SMTP username", "");
    if user.is_empty() {
        return None;
    }
    env_vars.push(("EMAIL_USERNAME".into(), user));
    let pass = prompt_secret("      SMTP password");
    if !pass.is_empty() {
        env_vars.push(("EMAIL_PASSWORD".into(), pass));
    }
    let from = prompt_string("      From address", "");
    if from.is_empty() {
        println!("      Skipped (from address required).");
        return None;
    }
    Some(EmailEntry {
        smtp_host: host,
        smtp_port: port,
        from_address: from,
    })
}

fn setup_channel_ha(env_vars: &mut Vec<(String, String)>) -> Option<HomeAssistantEntry> {
    println!();
    let url = prompt_string(
        "      Home Assistant URL",
        "http://homeassistant.local:8123",
    );
    let token = prompt_secret("      Paste your HA long-lived access token");
    if token.is_empty() {
        return None;
    }
    env_vars.push(("HA_TOKEN".into(), token));
    Some(HomeAssistantEntry { api_url: url })
}

// ── Skills picker ───────────────────────────────────────────────────────────

fn pick_skill_categories() -> Vec<String> {
    let all_cats = SkillCategory::all();
    println!();
    for (i, cat) in all_cats.iter().enumerate() {
        println!("    {:2}) {}", i + 1, cat.display_name());
    }
    println!();
    println!("  Enter numbers separated by commas (e.g. 1,2,3,8):");
    let input = prompt_string("  Categories", "");
    let selected: Vec<String> = input
        .split(',')
        .filter_map(|s| {
            let n = s.trim().parse::<usize>().ok()?;
            if n >= 1 && n <= all_cats.len() {
                Some(format!("{:?}", all_cats[n - 1]).to_lowercase())
            } else {
                None
            }
        })
        .collect();
    if selected.is_empty() {
        println!("  No valid selection. Enabling all categories.\n");
    } else {
        println!("  Enabled {} categories.\n", selected.len());
    }
    selected
}

// ── MCP ─────────────────────────────────────────────────────────────────────

struct McpEntry {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

// ── API verification ────────────────────────────────────────────────────────

fn verify_api_key(providers: &ProviderSetup) -> bool {
    // Try a lightweight API call to verify connectivity.
    // We use a blocking reqwest client since we're not in an async context.
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref p) = providers.anthropic {
        let key = std::env::var(&p.env_var).unwrap_or_default();
        if key.is_empty() {
            return false;
        }
        let base = p.base_url.as_deref().unwrap_or("https://api.anthropic.com");
        let resp = client
            .post(format!("{base}/v1/messages"))
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .body(r#"{"model":"claude-haiku-3-20240307","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#)
            .send();
        return match resp {
            Ok(r) => r.status().is_success() || r.status().as_u16() == 400,
            Err(_) => false,
        };
    }

    if let Some(ref p) = providers.openai {
        let key = std::env::var(&p.env_var).unwrap_or_default();
        if key.is_empty() {
            return false;
        }
        let base = p.base_url.as_deref().unwrap_or("https://api.openai.com/v1");
        let resp = client
            .get(format!("{base}/models"))
            .header("Authorization", format!("Bearer {key}"))
            .send();
        return match resp {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        };
    }

    if let Some(ref p) = providers.nvidia {
        let key = std::env::var(&p.env_var).unwrap_or_default();
        if key.is_empty() {
            return false;
        }
        let base = p
            .base_url
            .as_deref()
            .unwrap_or("https://integrate.api.nvidia.com/v1");
        let resp = client
            .get(format!("{base}/models"))
            .header("Authorization", format!("Bearer {key}"))
            .send();
        return match resp {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        };
    }

    if let Some(ref p) = providers.zai {
        let key = std::env::var(&p.env_var).unwrap_or_default();
        return !key.is_empty(); // Zai doesn't have a simple health endpoint
    }

    if let Some(ref p) = providers.openrouter {
        let key = std::env::var(&p.env_var).unwrap_or_default();
        if key.is_empty() {
            return false;
        }
        let resp = client
            .get("https://openrouter.ai/api/v1/models")
            .header("Authorization", format!("Bearer {key}"))
            .send();
        return match resp {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        };
    }

    false
}

// ── .env file ───────────────────────────────────────────────────────────────

fn generate_env_file(env_vars: &[(String, String)]) -> String {
    let mut out = String::from("# Ferroclaw secrets — generated by `ferroclaw setup`\n");
    out.push_str("# chmod 600 — do NOT commit this file\n\n");
    for (key, value) in env_vars {
        out.push_str(&format!("{key}={value}\n"));
    }
    out
}

/// Load .env files into the process environment.
/// Searches in order: CWD/.env, then config dir/.env.
/// Shell env takes precedence over file values, and CWD takes precedence over config dir.
/// Called at startup before config loading.
pub fn load_env_file() {
    // Load from config dir first (lower priority)
    load_env_from_path(&config::config_dir().join(".env"));
    // Load from CWD (higher priority — overwrites config dir values)
    load_env_from_path(&std::path::PathBuf::from(".env"));
}

fn load_env_from_path(env_path: &std::path::Path) {
    if !env_path.exists() {
        return;
    }
    let content = match std::fs::read_to_string(env_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[ferroclaw] warning: could not read {}: {e}",
                env_path.display()
            );
            return;
        }
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if key.is_empty() {
                continue;
            }
            // Only set if not already present (shell env takes precedence)
            if std::env::var(key).is_err() {
                // SAFETY: called at startup before any threads are spawned.
                unsafe { std::env::set_var(key, value) };
            }
        }
    }
}

// ── TOML generator ──────────────────────────────────────────────────────────

fn push_openai_compatible_provider_section(
    t: &mut String,
    section: &str,
    p: &ProviderEntry,
    oauth: bool,
) {
    t.push_str(&format!("[providers.{section}]\n"));
    t.push_str(&format!("api_key_env = \"{}\"\n", p.env_var));
    if oauth {
        t.push_str("auth_mode = \"oauth\"\n");
        t.push_str(&format!("oauth_token_env = \"{}\"\n", p.env_var));
    }
    if let Some(ref url) = p.base_url {
        t.push_str(&format!("base_url = \"{url}\"\n"));
    }
    t.push_str(
        "max_tokens = 8192\nrequest_timeout_ms = 15000\nmax_retries = 2\nno_retry_max_tokens_threshold = 128\n",
    );
    t.push('\n');
}

struct SetupTomlParams<'a> {
    providers: &'a ProviderSetup,
    default_model: &'a str,
    capabilities: &'a [&'a str],
    load_bundled: bool,
    enabled_categories: &'a Option<Vec<String>>,
    channels: &'a ChannelSetup,
    mcp_servers: &'a [McpEntry],
    gateway_port: u16,
}

fn generate_config_toml(params: SetupTomlParams<'_>) -> String {
    let SetupTomlParams {
        providers,
        default_model,
        capabilities,
        load_bundled,
        enabled_categories,
        channels,
        mcp_servers,
        gateway_port,
    } = params;

    let mut t = String::with_capacity(2048);

    t.push_str("# Ferroclaw Configuration\n# Generated by `ferroclaw setup`\n\n");

    // Agent
    t.push_str("[agent]\n");
    t.push_str(&format!("default_model = \"{default_model}\"\n"));
    t.push_str("max_iterations = 150\ntoken_budget = 200000\nmax_tool_calls_per_iteration = 8\nmax_tool_calls_total = 64\nmax_wall_clock_ms = 0\ndeadline_aware_completion = true\ndeadline_tight_ms = 1200\ndeadline_tight_max_tokens = 96\n\n");

    // Providers — env var names only, actual keys are in .env
    if let Some(ref p) = providers.anthropic {
        t.push_str("[providers.anthropic]\n");
        t.push_str(&format!("api_key_env = \"{}\"\n", p.env_var));
        if let Some(ref url) = p.base_url {
            t.push_str(&format!("base_url = \"{url}\"\n"));
        }
        t.push_str("max_tokens = 8192\nrequest_timeout_ms = 15000\nmax_retries = 2\nno_retry_max_tokens_threshold = 128\n");
        t.push('\n');
    }

    if let Some(ref p) = providers.openai {
        push_openai_compatible_provider_section(&mut t, "openai", p, false);
    }
    if let Some(ref p) = providers.openai_codex {
        push_openai_compatible_provider_section(&mut t, "openai_codex", p, true);
    }
    if let Some(ref p) = providers.google {
        push_openai_compatible_provider_section(&mut t, "google", p, false);
    }
    if let Some(ref p) = providers.xai {
        push_openai_compatible_provider_section(&mut t, "xai", p, false);
    }
    if let Some(ref p) = providers.nvidia {
        push_openai_compatible_provider_section(&mut t, "nvidia", p, false);
    }
    if let Some(ref p) = providers.zai {
        t.push_str("[providers.zai]\n");
        t.push_str(&format!("api_key_env = \"{}\"\n", p.env_var));
        if let Some(ref url) = p.base_url {
            t.push_str(&format!("base_url = \"{url}\"\n"));
        }
        t.push_str("max_tokens = 8192\nrequest_timeout_ms = 15000\nmax_retries = 2\nno_retry_max_tokens_threshold = 128\n");
        t.push('\n');
    }
    if let Some(ref p) = providers.llamacpp {
        push_openai_compatible_provider_section(&mut t, "llamacpp", p, false);
    }
    if let Some(ref p) = providers.mistral {
        push_openai_compatible_provider_section(&mut t, "mistral", p, false);
    }
    if let Some(ref p) = providers.azure_openai {
        push_openai_compatible_provider_section(&mut t, "azure_openai", p, false);
    }
    if let Some(ref p) = providers.github_copilot {
        push_openai_compatible_provider_section(&mut t, "github_copilot", p, false);
    }
    if let Some(ref p) = providers.google_vertex {
        push_openai_compatible_provider_section(&mut t, "google_vertex", p, false);
    }
    if let Some(ref p) = providers.bedrock {
        push_openai_compatible_provider_section(&mut t, "bedrock", p, false);
    }
    if let Some(ref p) = providers.openrouter {
        t.push_str("[providers.openrouter]\n");
        t.push_str(&format!("api_key_env = \"{}\"\n", p.env_var));
        if let Some(ref url) = p.base_url {
            t.push_str(&format!("base_url = \"{url}\"\n"));
        }
        t.push_str("max_tokens = 8192\nrequest_timeout_ms = 15000\nmax_retries = 2\nno_retry_max_tokens_threshold = 128\n");
        t.push('\n');
    }

    // Security
    t.push_str("[security]\n");
    let caps: Vec<String> = capabilities.iter().map(|c| format!("\"{c}\"")).collect();
    t.push_str(&format!("default_capabilities = [{}]\n", caps.join(", ")));
    t.push_str("require_skill_signatures = true\naudit_enabled = true\n\n");

    // Gateway
    t.push_str(&format!(
        "[gateway]\nbind = \"127.0.0.1\"\nport = {gateway_port}\n\n"
    ));

    // Skills
    t.push_str("[skills]\n");
    t.push_str(&format!("load_bundled = {load_bundled}\n"));
    if let Some(cats) = enabled_categories
        && !cats.is_empty()
    {
        let c: Vec<String> = cats.iter().map(|c| format!("\"{c}\"")).collect();
        t.push_str(&format!("enabled_categories = [{}]\n", c.join(", ")));
    }
    t.push('\n');

    // Channels — tokens reference env vars, actual values in .env
    if channels.telegram.is_some() {
        t.push_str("[telegram]\nbot_token_env = \"TELEGRAM_BOT_TOKEN\"\nallowed_chat_ids = []\n\n");
    }
    if let Some(ref d) = channels.discord {
        t.push_str("[channels.discord]\nbot_token_env = \"DISCORD_BOT_TOKEN\"\n");
        t.push_str(&format!("command_prefix = \"{}\"\n", d.prefix));
        t.push_str("allowed_guild_ids = []\n\n");
    }
    if channels.slack.is_some() {
        t.push_str("[channels.slack]\nbot_token_env = \"SLACK_BOT_TOKEN\"\n");
        t.push_str("app_token_env = \"SLACK_APP_TOKEN\"\nallowed_channels = []\n\n");
    }
    if let Some(ref w) = channels.whatsapp {
        t.push_str("[channels.whatsapp]\napi_token_env = \"WHATSAPP_API_TOKEN\"\n");
        t.push_str(&format!("phone_number_id = \"{}\"\n", w.phone_number_id));
        t.push_str("allowed_numbers = []\n\n");
    }
    if let Some(ref s) = channels.signal {
        t.push_str(&format!("[channels.signal]\napi_url = \"{}\"\n", s.api_url));
        t.push_str(&format!("phone_number = \"{}\"\n", s.phone_number));
        t.push_str("allowed_numbers = []\n\n");
    }
    if let Some(ref e) = channels.email {
        t.push_str(&format!(
            "[channels.email]\nsmtp_host = \"{}\"\n",
            e.smtp_host
        ));
        t.push_str(&format!("smtp_port = {}\n", e.smtp_port));
        t.push_str("username_env = \"EMAIL_USERNAME\"\npassword_env = \"EMAIL_PASSWORD\"\n");
        t.push_str(&format!("from_address = \"{}\"\n", e.from_address));
        t.push_str("allowed_addresses = []\n\n");
    }
    if let Some(ref ha) = channels.homeassistant {
        t.push_str(&format!(
            "[channels.homeassistant]\napi_url = \"{}\"\n",
            ha.api_url
        ));
        t.push_str("token_env = \"HA_TOKEN\"\n\n");
    }

    // MCP Servers
    for s in mcp_servers {
        t.push_str(&format!("[mcp_servers.{}]\n", s.name));
        t.push_str(&format!("command = \"{}\"\n", s.command));
        let a: Vec<String> = s.args.iter().map(|a| format!("\"{a}\"")).collect();
        t.push_str(&format!("args = [{}]\n", a.join(", ")));
        if !s.env.is_empty() {
            let e: Vec<String> = s
                .env
                .iter()
                .map(|(k, v)| format!("{k} = \"{v}\""))
                .collect();
            t.push_str(&format!("env = {{ {} }}\n", e.join(", ")));
        }
        t.push('\n');
    }

    t
}
