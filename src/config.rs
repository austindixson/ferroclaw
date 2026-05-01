use crate::error::{FerroError, Result};
use crate::types::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_model")]
    pub default_model: String,
    /// Fallback models tried in order when the primary model fails.
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_token_budget")]
    pub token_budget: u64,
    #[serde(default = "default_max_tool_calls_per_iteration")]
    pub max_tool_calls_per_iteration: u32,
    #[serde(default = "default_max_tool_calls_total")]
    pub max_tool_calls_total: u32,
    /// Optional hard wall-clock budget for a single run (0 disables).
    #[serde(default = "default_max_wall_clock_ms")]
    pub max_wall_clock_ms: u64,
    /// Enable adaptive token/retry behavior when close to the wall-clock deadline.
    #[serde(default = "default_true")]
    pub deadline_aware_completion: bool,
    /// Remaining-ms threshold where we switch to tight completion mode.
    #[serde(default = "default_deadline_tight_ms")]
    pub deadline_tight_ms: u64,
    /// Max tokens used while in tight completion mode.
    #[serde(default = "default_deadline_tight_max_tokens")]
    pub deadline_tight_max_tokens: u32,
    #[serde(default = "default_max_response_size")]
    pub max_response_size: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            fallback_models: Vec::new(),
            max_iterations: default_max_iterations(),
            token_budget: default_token_budget(),
            max_tool_calls_per_iteration: default_max_tool_calls_per_iteration(),
            max_tool_calls_total: default_max_tool_calls_total(),
            max_wall_clock_ms: default_max_wall_clock_ms(),
            deadline_aware_completion: default_true(),
            deadline_tight_ms: default_deadline_tight_ms(),
            deadline_tight_max_tokens: default_deadline_tight_max_tokens(),
            max_response_size: default_max_response_size(),
            system_prompt: default_system_prompt(),
        }
    }
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".into()
}
fn default_max_iterations() -> u32 {
    150
}
fn default_token_budget() -> u64 {
    200_000
}
fn default_max_tool_calls_per_iteration() -> u32 {
    8
}
fn default_max_tool_calls_total() -> u32 {
    64
}
fn default_max_wall_clock_ms() -> u64 {
    0
}
fn default_deadline_tight_ms() -> u64 {
    1200
}
fn default_deadline_tight_max_tokens() -> u32 {
    96
}
fn default_max_response_size() -> usize {
    50_000
}
fn default_system_prompt() -> String {
    format!(
        "You are Ferroclaw, a capable AI assistant with full access to the user's system. \
         You can read/write files anywhere, execute any bash command, and access the network. \
         Use the `bash` tool for system operations (creating folders, running commands, installing software). \
         Use `read_file` and `write_file` for file operations. \
         Prefer the built-in tools (bash, read_file, write_file, list_directory) over MCP tools. \
         Be concise and direct. When using tools, explain what you're doing briefly.\n\n\
         User's home directory: {}\n\
         Operating system: {}\n\
         Current working directory: {}",
        dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".into()),
        std::env::consts::OS,
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into()),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anthropic: Option<AnthropicConfig>,
    #[serde(default)]
    pub openai: Option<OpenAiConfig>,
    #[serde(default)]
    pub openai_codex: Option<OpenAiConfig>,
    #[serde(default)]
    pub google: Option<OpenAiConfig>,
    #[serde(default)]
    pub xai: Option<OpenAiConfig>,
    #[serde(default)]
    pub nvidia: Option<OpenAiConfig>,
    #[serde(default)]
    pub zai: Option<ZaiConfig>,
    #[serde(default)]
    pub llamacpp: Option<OpenAiConfig>,
    #[serde(default)]
    pub mistral: Option<OpenAiConfig>,
    #[serde(default)]
    pub azure_openai: Option<OpenAiConfig>,
    #[serde(default)]
    pub github_copilot: Option<OpenAiConfig>,
    #[serde(default)]
    pub google_vertex: Option<OpenAiConfig>,
    #[serde(default)]
    pub bedrock: Option<OpenAiConfig>,
    #[serde(default)]
    pub openrouter: Option<OpenRouterConfig>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            anthropic: Some(AnthropicConfig::default()),
            openai: None,
            openai_codex: None,
            google: None,
            xai: None,
            nvidia: None,
            zai: None,
            llamacpp: None,
            mistral: None,
            azure_openai: None,
            github_copilot: None,
            google_vertex: None,
            bedrock: None,
            openrouter: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    #[serde(default = "default_anthropic_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_anthropic_base_url")]
    pub base_url: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_provider_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_provider_no_retry_max_tokens_threshold")]
    pub no_retry_max_tokens_threshold: u32,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_anthropic_api_key_env(),
            base_url: default_anthropic_base_url(),
            max_tokens: default_max_tokens(),
            request_timeout_ms: default_provider_request_timeout_ms(),
            max_retries: default_provider_max_retries(),
            no_retry_max_tokens_threshold: default_provider_no_retry_max_tokens_threshold(),
        }
    }
}

fn default_anthropic_api_key_env() -> String {
    "ANTHROPIC_API_KEY".into()
}
fn default_anthropic_base_url() -> String {
    "https://api.anthropic.com".into()
}
fn default_max_tokens() -> u32 {
    8192
}
fn default_provider_request_timeout_ms() -> u64 {
    15_000
}
fn default_provider_max_retries() -> u32 {
    2
}
fn default_provider_no_retry_max_tokens_threshold() -> u32 {
    128
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    #[serde(default = "default_openai_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default = "default_openai_auth_mode")]
    pub auth_mode: String,
    #[serde(default = "default_openai_oauth_token_env")]
    pub oauth_token_env: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_provider_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_provider_no_retry_max_tokens_threshold")]
    pub no_retry_max_tokens_threshold: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_openai_api_key_env(),
            base_url: default_openai_base_url(),
            auth_mode: default_openai_auth_mode(),
            oauth_token_env: default_openai_oauth_token_env(),
            max_tokens: default_max_tokens(),
            request_timeout_ms: default_provider_request_timeout_ms(),
            max_retries: default_provider_max_retries(),
            no_retry_max_tokens_threshold: default_provider_no_retry_max_tokens_threshold(),
        }
    }
}

fn default_openai_api_key_env() -> String {
    "OPENAI_API_KEY".into()
}
fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".into()
}
fn default_openai_auth_mode() -> String {
    "api_key".into()
}
fn default_openai_oauth_token_env() -> String {
    "OPENAI_OAUTH_TOKEN".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZaiConfig {
    #[serde(default = "default_zai_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_zai_base_url")]
    pub base_url: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_provider_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_provider_no_retry_max_tokens_threshold")]
    pub no_retry_max_tokens_threshold: u32,
}

impl Default for ZaiConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_zai_api_key_env(),
            base_url: default_zai_base_url(),
            max_tokens: default_max_tokens(),
            request_timeout_ms: default_provider_request_timeout_ms(),
            max_retries: default_provider_max_retries(),
            no_retry_max_tokens_threshold: default_provider_no_retry_max_tokens_threshold(),
        }
    }
}

fn default_zai_api_key_env() -> String {
    "ZAI_API_KEY".into()
}
fn default_zai_base_url() -> String {
    "https://api.z.ai/api/paas/v4".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    #[serde(default = "default_openrouter_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_openrouter_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub site_url: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_provider_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_provider_no_retry_max_tokens_threshold")]
    pub no_retry_max_tokens_threshold: u32,
}

fn default_openrouter_api_key_env() -> String {
    "OPENROUTER_API_KEY".into()
}
fn default_openrouter_base_url() -> String {
    "https://openrouter.ai/api/v1".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
}

fn default_cache_ttl() -> u64 {
    3600
}

impl McpServerConfig {
    pub fn is_stdio(&self) -> bool {
        self.command.is_some()
    }

    pub fn is_sse(&self) -> bool {
        self.url.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_capabilities")]
    pub default_capabilities: Vec<Capability>,
    #[serde(default = "default_true")]
    pub require_skill_signatures: bool,
    #[serde(default = "default_true")]
    pub audit_enabled: bool,
    #[serde(default)]
    pub audit_path: Option<PathBuf>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            default_capabilities: default_capabilities(),
            require_skill_signatures: true,
            audit_enabled: true,
            audit_path: None,
        }
    }
}

fn default_capabilities() -> Vec<Capability> {
    vec![
        Capability::FsRead,
        Capability::NetOutbound,
        Capability::MemoryRead,
        Capability::MemoryWrite,
    ]
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub bearer_token_env: Option<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            bearer_token: None,
            bearer_token_env: None,
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    8420
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token_env: String,
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}


/// Skills configuration — controls which skill categories and individual skills are loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Custom skills directory (default: ~/.config/ferroclaw/skills/).
    #[serde(default)]
    pub custom_skills_dir: Option<PathBuf>,
    /// If set, only load skills from these categories. If None, load all.
    #[serde(default)]
    pub enabled_categories: Option<Vec<String>>,
    /// Specific skill names to disable.
    #[serde(default)]
    pub disabled_skills: Option<Vec<String>>,
    /// Whether to load bundled skills (default: true).
    #[serde(default = "default_true")]
    pub load_bundled: bool,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            custom_skills_dir: None,
            enabled_categories: None,
            disabled_skills: None,
            load_bundled: true,
        }
    }
}

/// Channels configuration — multi-platform messaging adapters.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub discord: Option<DiscordConfig>,
    #[serde(default)]
    pub slack: Option<SlackConfig>,
    #[serde(default)]
    pub whatsapp: Option<WhatsAppConfig>,
    #[serde(default)]
    pub signal: Option<SignalConfig>,
    #[serde(default)]
    pub email: Option<EmailConfig>,
    #[serde(default)]
    pub homeassistant: Option<HomeAssistantConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Env var name containing the Discord bot token.
    pub bot_token_env: String,
    /// Guild/channel allowlist (empty = allow all).
    #[serde(default)]
    pub allowed_guild_ids: Vec<u64>,
    /// Command prefix for the bot.
    #[serde(default = "default_discord_prefix")]
    pub command_prefix: String,
}

fn default_discord_prefix() -> String {
    "!fc ".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Env var name containing the Slack bot token (xoxb-...).
    pub bot_token_env: String,
    /// Env var name containing the Slack app token (xapp-...) for Socket Mode.
    #[serde(default)]
    pub app_token_env: Option<String>,
    /// Channel allowlist (empty = allow all).
    #[serde(default)]
    pub allowed_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Env var name for WhatsApp Business Cloud API token.
    pub api_token_env: String,
    /// Phone number ID for sending messages.
    pub phone_number_id: String,
    /// Webhook verify token.
    #[serde(default)]
    pub webhook_verify_token: Option<String>,
    /// Phone number allowlist (empty = allow all).
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// URL of signal-cli REST API (e.g. http://localhost:8080).
    pub api_url: String,
    /// Registered phone number (e.g. +1234567890).
    pub phone_number: String,
    /// Phone number allowlist (empty = allow all).
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// SMTP server host.
    pub smtp_host: String,
    /// SMTP port (default: 587).
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    /// Env var for SMTP username.
    pub username_env: String,
    /// Env var for SMTP password.
    pub password_env: String,
    /// Sender email address.
    pub from_address: String,
    /// Email allowlist (empty = allow all).
    #[serde(default)]
    pub allowed_addresses: Vec<String>,
    /// IMAP server for receiving (optional).
    #[serde(default)]
    pub imap_host: Option<String>,
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
}

fn default_smtp_port() -> u16 {
    587
}
fn default_imap_port() -> u16 {
    993
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeAssistantConfig {
    /// Home Assistant API URL (e.g. http://homeassistant.local:8123).
    pub api_url: String,
    /// Env var for long-lived access token.
    pub token_env: String,
    /// Entity ID for the conversation agent (e.g. conversation.ferroclaw).
    #[serde(default)]
    pub entity_id: Option<String>,
}

/// Resolve `${VAR_NAME}` placeholders from environment variables.
/// Resolve an environment variable reference to its value.
///
/// Supports two formats:
/// - `${VAR_NAME}` — explicit template syntax
/// - `VAR_NAME`    — plain env var name (used by `api_key_env` fields)
///
/// In both cases, looks up the env var and returns its value.
pub fn resolve_env_var(template: &str) -> Result<String> {
    let var_name = if template.starts_with("${") && template.ends_with('}') {
        &template[2..template.len() - 1]
    } else {
        template
    };

    std::env::var(var_name).map_err(|_| {
        FerroError::Config(format!(
            "Environment variable '{var_name}' not set (referenced as '{template}')"
        ))
    })
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ferroclaw")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ferroclaw")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ferroclaw")
}

pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let config_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir().join("config.toml"));

    if !config_path.exists() {
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        FerroError::Config(format!(
            "Failed to read config at {}: {e}",
            config_path.display()
        ))
    })?;

    toml::from_str(&content).map_err(|e| {
        FerroError::Config(format!(
            "Failed to parse config at {}: {e}",
            config_path.display()
        ))
    })
}

pub fn generate_example_config() -> String {
    r#"# Ferroclaw Configuration

[agent]
default_model = "claude-sonnet-4-20250514"
max_iterations = 150
token_budget = 200000
max_tool_calls_per_iteration = 8
max_tool_calls_total = 64
max_wall_clock_ms = 0
deadline_aware_completion = true
deadline_tight_ms = 1200
deadline_tight_max_tokens = 96

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"

# [providers.openai]
# api_key_env = "OPENAI_API_KEY"
# base_url = "https://api.openai.com/v1"
# auth_mode = "api_key" # or "oauth"
# oauth_token_env = "OPENAI_OAUTH_TOKEN"

# [providers.openai_codex]
# api_key_env = "OPENAI_API_KEY"
# base_url = "https://chatgpt.com/backend-api"
# auth_mode = "oauth"
# oauth_token_env = "OPENAI_OAUTH_TOKEN"

# [providers.google]
# api_key_env = "GEMINI_API_KEY"
# base_url = "https://generativelanguage.googleapis.com/v1beta/openai"

# [providers.xai]
# api_key_env = "XAI_API_KEY"
# base_url = "https://api.x.ai/v1"

# [providers.zai]
# api_key_env = "ZAI_API_KEY"
# base_url = "https://api.z.ai/api/paas/v4"

# [providers.llamacpp]
# api_key_env = "LLAMACPP_API_KEY"
# base_url = "http://127.0.0.1:8000/v1"

# [providers.mistral]
# api_key_env = "MISTRAL_API_KEY"
# base_url = "https://api.mistral.ai/v1"

# [providers.azure_openai]
# api_key_env = "AZURE_OPENAI_API_KEY"
# base_url = "https://<resource>.openai.azure.com/openai/v1"

# [providers.github_copilot]
# api_key_env = "GITHUB_COPILOT_API_KEY"
# base_url = "https://api.githubcopilot.com"

# [providers.google_vertex]
# api_key_env = "GOOGLE_VERTEX_API_KEY"
# base_url = "https://aiplatform.googleapis.com/v1/projects/<project>/locations/<location>/endpoints/openapi"

# [providers.bedrock]
# api_key_env = "AWS_BEARER_TOKEN_BEDROCK"
# base_url = "https://bedrock-runtime.<region>.amazonaws.com/openai/v1"

# [providers.openrouter]
# api_key_env = "OPENROUTER_API_KEY"
# base_url = "https://openrouter.ai/api/v1"
# site_url = "https://your-app.com"
# site_name = "Your App"

[security]
default_capabilities = ["fs_read", "net_outbound", "memory_read", "memory_write"]
require_skill_signatures = true
audit_enabled = true

[gateway]
bind = "127.0.0.1"
port = 8420
# bearer_token_env = "FERROCLAW_TOKEN"

# ── Skills ────────────────────────────────────────────────────────────────
# 84 bundled skills across 16 categories, loaded by default.
# Custom skills: add TOML files to ~/.config/ferroclaw/skills/
[skills]
load_bundled = true
# enabled_categories = ["filesystem", "version_control", "code_analysis"]
# disabled_skills = ["ssh_command", "docker_exec"]
# custom_skills_dir = "~/.config/ferroclaw/skills"

# ── Messaging Channels ───────────────────────────────────────────────────

# [telegram]
# bot_token_env = "TELEGRAM_BOT_TOKEN"
# allowed_chat_ids = []

# [channels.discord]
# bot_token_env = "DISCORD_BOT_TOKEN"
# allowed_guild_ids = []
# command_prefix = "!fc "

# [channels.slack]
# bot_token_env = "SLACK_BOT_TOKEN"
# app_token_env = "SLACK_APP_TOKEN"
# allowed_channels = []

# [channels.whatsapp]
# api_token_env = "WHATSAPP_API_TOKEN"
# phone_number_id = "your_phone_number_id"
# webhook_verify_token = "your_verify_token"
# allowed_numbers = []

# [channels.signal]
# api_url = "http://localhost:8080"
# phone_number = "+1234567890"
# allowed_numbers = []

# [channels.email]
# smtp_host = "smtp.gmail.com"
# smtp_port = 587
# username_env = "EMAIL_USERNAME"
# password_env = "EMAIL_PASSWORD"
# from_address = "ferroclaw@example.com"
# allowed_addresses = []

# [channels.homeassistant]
# api_url = "http://homeassistant.local:8123"
# token_env = "HA_TOKEN"
# entity_id = "conversation.ferroclaw"

# ── MCP Servers ───────────────────────────────────────────────────────────

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

# [mcp_servers.github]
# command = "npx"
# args = ["-y", "@modelcontextprotocol/server-github"]
# env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.agent.max_iterations, 150);
        assert_eq!(config.agent.max_tool_calls_per_iteration, 8);
        assert_eq!(config.agent.max_tool_calls_total, 64);
        assert_eq!(config.gateway.bind, "127.0.0.1");
        assert_eq!(config.gateway.port, 8420);
    }

    #[test]
    fn test_resolve_env_var() {
        // SAFETY: test runs in a single thread, no concurrent env access
        unsafe { std::env::set_var("FERRO_TEST_VAR", "test_value") };
        // Both ${VAR} and plain VAR should resolve
        assert_eq!(resolve_env_var("${FERRO_TEST_VAR}").unwrap(), "test_value");
        assert_eq!(resolve_env_var("FERRO_TEST_VAR").unwrap(), "test_value");
        // Nonexistent vars should error in both formats
        assert!(resolve_env_var("${NONEXISTENT_VAR_12345}").is_err());
        assert!(resolve_env_var("NONEXISTENT_VAR_12345").is_err());
        unsafe { std::env::remove_var("FERRO_TEST_VAR") };
    }

    #[test]
    fn test_parse_example_config() {
        let example = generate_example_config();
        let _config: Config = toml::from_str(&example).expect("Example config should parse");
    }
}
