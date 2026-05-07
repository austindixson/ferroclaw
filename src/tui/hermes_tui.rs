//! Hermes-style TUI module for Ferroclaw.
//!
//! Provides a chat interface similar to the Hermes agent TUI with:
//! - Dark theme
//! - Message bubbles (assistant: "Ferroclaw" header + text; user: orange dot + text)
//! - Bottom status bar with model/process info
//! - Left sidebar with task management

use super::app::{App, ChatEntry};
use super::events::{Event, EventHandler};
use super::hermes_ui::draw as draw_hermes;

use crate::agent::AgentLoop;
use crate::agent::r#loop::AgentEvent;
use crate::config::{self, Config};
use crate::tui::glitter_verbs::{
    elapsed_ms_since, glitter_verb_for_llm_pending, glitter_verb_for_tool_call,
};
use crate::types::{Message, RunStopReason};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct ExternalSkill {
    name: String,
    path: PathBuf,
    content: String,
}

type SkillCatalog = BTreeMap<String, ExternalSkill>;

enum SlashAction {
    Continue,
    Send(String),
}

const BASE_SLASH_COMMANDS: [&str; 7] = [
    "/help",
    "/model",
    "/skills",
    "/skills rescan",
    "/active-skills",
    "/use",
    "/unuse",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ModelMenuMode {
    #[default]
    None,
    ProviderSelect,
    OpenRouterModels,
    OpenAiModels,
    NvidiaModels,
}

#[derive(Default)]
struct ModelCommandState {
    mode: ModelMenuMode,
    query: String,
    openrouter_models: Vec<String>,
    openai_models: Vec<String>,
    nvidia_models: Vec<String>,
}

fn current_provider_name(config: &Config) -> String {
    let model = config.agent.default_model.to_lowercase();
    if config.providers.nvidia.is_some()
        && (model.starts_with("z-ai/")
            || model.starts_with("nvidia/")
            || model.starts_with("nvidia:"))
    {
        return "nvidia".to_string();
    }
    if config.providers.openrouter.is_some() && model.contains('/') {
        return "openrouter".to_string();
    }
    if config.providers.openai.is_some() && (model.starts_with("gpt-") || model.starts_with("o")) {
        return "openai".to_string();
    }
    if config.providers.anthropic.is_some() && model.starts_with("claude") {
        return "anthropic".to_string();
    }
    if config.providers.zai.is_some() && model.starts_with("glm") {
        return "zai".to_string();
    }

    if config.providers.openrouter.is_some() {
        return "openrouter".to_string();
    }
    if config.providers.anthropic.is_some() {
        return "anthropic".to_string();
    }
    if config.providers.openai.is_some() {
        return "openai".to_string();
    }
    if config.providers.nvidia.is_some() {
        return "nvidia".to_string();
    }
    if config.providers.zai.is_some() {
        return "zai".to_string();
    }

    "openrouter".to_string()
}

fn configured_provider_menu_items(config: &Config) -> Vec<String> {
    let mut providers = Vec::new();
    if config.providers.openrouter.is_some() {
        providers.push("openrouter".to_string());
    }
    if config.providers.anthropic.is_some() {
        providers.push("anthropic".to_string());
    }
    if config.providers.openai.is_some() || config.providers.openai_codex.is_some() {
        providers.push("openai".to_string());
    }
    if config.providers.nvidia.is_some() {
        providers.push("nvidia".to_string());
    }
    if config.providers.zai.is_some() {
        providers.push("zai".to_string());
    }

    if providers.is_empty() {
        providers.push("openrouter".to_string());
    }

    let current = current_provider_name(config);
    if let Some(idx) = providers.iter().position(|p| p == &current) {
        providers.swap(0, idx);
    }

    providers
}

fn model_menu_items_for_input(config: &Config, state: &ModelCommandState) -> Vec<String> {
    match state.mode {
        ModelMenuMode::None => Vec::new(),
        ModelMenuMode::ProviderSelect => {
            let query = state.query.trim().to_lowercase();
            configured_provider_menu_items(config)
                .into_iter()
                .filter(|provider| query.is_empty() || provider.contains(&query))
                .collect()
        }
        ModelMenuMode::OpenRouterModels => {
            let query = state.query.trim().to_lowercase();
            state
                .openrouter_models
                .iter()
                .filter(|model| query.is_empty() || model.to_lowercase().contains(&query))
                .take(500)
                .cloned()
                .collect()
        }
        ModelMenuMode::OpenAiModels => {
            let query = state.query.trim().to_lowercase();
            state
                .openai_models
                .iter()
                .filter(|model| query.is_empty() || model.to_lowercase().contains(&query))
                .take(500)
                .cloned()
                .collect()
        }
        ModelMenuMode::NvidiaModels => {
            let query = state.query.trim().to_lowercase();
            state
                .nvidia_models
                .iter()
                .filter(|model| query.is_empty() || model.to_lowercase().contains(&query))
                .take(500)
                .cloned()
                .collect()
        }
    }
}

fn resolve_openai_credential_for_tui(config: &Config) -> anyhow::Result<String> {
    let openai_cfg = config
        .providers
        .openai_codex
        .as_ref()
        .or(config.providers.openai.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!("providers.openai (or providers.openai_codex) is not configured")
        })?;

    let token_env = if openai_cfg.auth_mode.eq_ignore_ascii_case("oauth") {
        &openai_cfg.oauth_token_env
    } else {
        &openai_cfg.api_key_env
    };

    std::env::var(token_env).map_err(|_| anyhow::anyhow!("{} is not set", token_env))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1] as char;
            let h2 = bytes[i + 2] as char;
            if let (Some(a), Some(b)) = (h1.to_digit(16), h2.to_digit(16)) {
                out.push(((a << 4) + b) as u8);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn normalize_pasted_payload(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("file://localhost") {
                percent_decode(rest)
            } else if let Some(rest) = trimmed.strip_prefix("file://") {
                percent_decode(rest)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape_shell_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn normalize_path_candidate(raw: &str) -> String {
    let stripped = raw.trim().trim_matches('"').trim_matches('\'');
    let normalized = normalize_pasted_payload(stripped);
    unescape_shell_path(&normalized)
}

fn looks_like_filesystem_path_command(cmd: &str) -> bool {
    if !cmd.starts_with('/') {
        return false;
    }
    if BASE_SLASH_COMMANDS.contains(&cmd) {
        return false;
    }

    let normalized = normalize_path_candidate(cmd);
    let p = Path::new(&normalized);
    if p.is_absolute() && p.exists() {
        return true;
    }

    // Heuristic for absolute file-like paths that may not exist yet.
    p.is_absolute()
        && normalized.contains('/')
        && normalized
            .rsplit('/')
            .next()
            .is_some_and(|name| name.contains('.'))
}

fn local_image_paths_in_text(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .filter_map(|token| {
            let normalized = normalize_path_candidate(token);
            let p = Path::new(&normalized);
            if !p.is_absolute() {
                return None;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())?;
            let is_image = matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "heic"
            );
            if is_image { Some(normalized) } else { None }
        })
        .collect()
}

fn fetch_openrouter_models(config: &Config) -> anyhow::Result<Vec<String>> {
    let provider = config
        .providers
        .openrouter
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("providers.openrouter is not configured"))?;

    let api_key = std::env::var(&provider.api_key_env)
        .map_err(|_| anyhow::anyhow!("{} is not set", provider.api_key_env))?;

    let base = provider.base_url.trim_end_matches('/');
    let url = format!("{base}/models");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "OpenRouter /models returned {}",
            resp.status()
        ));
    }

    let json: serde_json::Value = resp.json()?;
    let mut models: Vec<String> = json
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| it.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    models.sort();
    models.dedup();

    if models.is_empty() {
        return Err(anyhow::anyhow!("OpenRouter returned zero models"));
    }

    Ok(models)
}

fn default_openai_model_catalog() -> Vec<String> {
    vec![
        "gpt-5.4-mini".into(),
        "gpt-5.4".into(),
        "gpt-5.3-codex".into(),
        "gpt-5.2-codex".into(),
        "gpt-5.1-codex-max".into(),
        "gpt-5.1-codex-mini".into(),
    ]
}

fn fetch_openai_models(config: &Config) -> anyhow::Result<Vec<String>> {
    let openai_cfg = config
        .providers
        .openai_codex
        .as_ref()
        .or(config.providers.openai.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!("providers.openai (or providers.openai_codex) is not configured")
        })?;

    let token = resolve_openai_credential_for_tui(config)?;
    let base = openai_cfg.base_url.trim_end_matches('/');
    let codex_backend = base
        .to_ascii_lowercase()
        .contains("chatgpt.com/backend-api/codex");
    let url = if codex_backend {
        format!("{base}/models?client_version=1.0.0")
    } else {
        format!("{base}/models")
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()?;

    if !resp.status().is_success() {
        if matches!(resp.status().as_u16(), 401 | 403) {
            return Ok(default_openai_model_catalog());
        }
        return Err(anyhow::anyhow!("OpenAI /models returned {}", resp.status()));
    }

    let json: serde_json::Value = resp.json()?;
    let mut models: Vec<String> = if codex_backend {
        json.get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                let mut sortable: Vec<(i64, String)> = arr
                    .iter()
                    .filter_map(|it| {
                        let slug = it.get("slug").and_then(|v| v.as_str())?.to_string();
                        if it
                            .get("supported_in_api")
                            .and_then(|v| v.as_bool())
                            .is_some_and(|ok| !ok)
                        {
                            return None;
                        }
                        let hidden = it
                            .get("visibility")
                            .and_then(|v| v.as_str())
                            .map(|s| {
                                matches!(s.trim().to_ascii_lowercase().as_str(), "hide" | "hidden")
                            })
                            .unwrap_or(false);
                        if hidden {
                            return None;
                        }
                        let priority = it
                            .get("priority")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(10_000);
                        Some((priority, slug))
                    })
                    .collect();
                sortable.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                sortable
                    .into_iter()
                    .map(|(_, slug)| slug)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        json.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|it| it.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    models.sort();
    models.dedup();

    if models.is_empty() {
        return Ok(default_openai_model_catalog());
    }

    Ok(models)
}

fn fetch_nvidia_models(config: &Config) -> anyhow::Result<Vec<String>> {
    let provider = config
        .providers
        .nvidia
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("providers.nvidia is not configured"))?;

    let api_key = std::env::var(&provider.api_key_env)
        .map_err(|_| anyhow::anyhow!("{} is not set", provider.api_key_env))?;

    let base = provider.base_url.trim_end_matches('/');
    let url = format!("{base}/models");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("NVIDIA /models returned {}", resp.status()));
    }

    let json: serde_json::Value = resp.json()?;
    let mut models: Vec<String> = json
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| it.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    models.sort();
    models.dedup();

    if models.is_empty() {
        return Err(anyhow::anyhow!("NVIDIA returned zero models"));
    }

    Ok(models)
}

fn persist_default_model(config: &Config, model: &str) -> anyhow::Result<PathBuf> {
    let path = config::config_dir().join("config.toml");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut root = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str::<toml::Value>(&content)
            .unwrap_or_else(|_| toml::Value::Table(Default::default()))
    } else {
        toml::Value::Table(Default::default())
    };

    let root_table = root
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("config root is not a table"))?;
    let agent = root_table
        .entry("agent")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[agent] is not a table"))?;

    agent.insert(
        "default_model".into(),
        toml::Value::String(model.to_string()),
    );
    agent
        .entry("max_iterations")
        .or_insert_with(|| toml::Value::Integer(config.agent.max_iterations as i64));
    agent
        .entry("token_budget")
        .or_insert_with(|| toml::Value::Integer(config.agent.token_budget as i64));
    agent
        .entry("max_tool_calls_per_iteration")
        .or_insert_with(|| toml::Value::Integer(config.agent.max_tool_calls_per_iteration as i64));
    agent
        .entry("max_tool_calls_total")
        .or_insert_with(|| toml::Value::Integer(config.agent.max_tool_calls_total as i64));

    std::fs::write(&path, toml::to_string_pretty(&root)?)?;
    Ok(path)
}

fn try_restart_gateway() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;

    // Use the dedicated CLI helper so all restart logic is centralized.
    let output = Command::new(&exe)
        .arg("gateway")
        .arg("restart")
        .arg("--force")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow::anyhow!(
                "Ferroclaw Gateway helper restart exited with {}",
                output.status
            ));
        }
        return Err(anyhow::anyhow!(
            "Ferroclaw Gateway helper restart exited with {}: {}",
            output.status,
            stderr
        ));
    }

    Ok(format!(
        "Ferroclaw Gateway helper restarted via '{} gateway restart --force'",
        exe.display()
    ))
}

fn discover_external_skills() -> SkillCatalog {
    let mut out = BTreeMap::new();
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let cwd = std::env::current_dir().ok();

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = &home {
        roots.push(home.join(".hermes/skills"));
        roots.push(home.join(".claude/workspace/skills"));
        roots.push(home.join(".claude/skills"));
        roots.push(home.join(".claude/plugins/cache"));
        roots.push(home.join(".cursor/skills"));
        roots.push(home.join(".cursor/skills-cursor"));
        roots.push(home.join(".cursor/plugins/cache"));
        roots.push(home.join(".openclaw"));
        roots.push(home.join(".openclaw/skills"));
    }
    if let Some(cwd) = &cwd {
        roots.push(cwd.join(".claude/workspace/skills"));
        roots.push(cwd.join(".claude/skills"));
        roots.push(cwd.join(".claude/plugins/cache"));
        roots.push(cwd.join(".cursor/skills"));
        roots.push(cwd.join(".cursor/skills-cursor"));
        roots.push(cwd.join(".cursor/plugins/cache"));
        roots.push(cwd.join(".openclaw"));
        roots.push(cwd.join(".openclaw/skills"));
        roots.push(cwd.join("skills"));
    }

    for root in roots {
        scan_skill_md(&root, &mut out);
    }
    out
}

fn scan_skill_md(root: &Path, out: &mut SkillCatalog) {
    if !root.exists() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("SKILL.md"))
                .unwrap_or(false)
            {
                continue;
            }
            if let Some((content, resolved_path)) = load_skill_content(&path) {
                let name = skill_name_from_path_or_frontmatter(&resolved_path, &content);
                out.insert(
                    name.clone(),
                    ExternalSkill {
                        name,
                        path: resolved_path,
                        content,
                    },
                );
            }
        }
    }
}

fn load_skill_content(path: &Path) -> Option<(String, PathBuf)> {
    if let Ok(content) = fs::read_to_string(path) {
        return Some((content, path.to_path_buf()));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(resolved) = resolve_macos_alias_path(path)
            && let Ok(content) = fs::read_to_string(&resolved)
        {
            return Some((content, resolved));
        }
    }

    if let Ok(resolved) = fs::canonicalize(path)
        && resolved != path
        && let Ok(content) = fs::read_to_string(&resolved)
    {
        return Some((content, resolved));
    }

    None
}

#[cfg(target_os = "macos")]
fn resolve_macos_alias_path(path: &Path) -> Option<PathBuf> {
    use std::process::Command;

    let path_str = path.to_str()?;
    let escaped = path_str.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "POSIX path of (original item of (POSIX file \"{}\" as alias))",
        escaped
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if resolved.is_empty() {
        None
    } else {
        Some(PathBuf::from(resolved))
    }
}

fn skill_name_from_path_or_frontmatter(path: &Path, content: &str) -> String {
    for line in content.lines().take(40) {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("name:") {
            let candidate = v.trim().trim_matches('"').trim_matches('\'');
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
    }
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("skill")
        .to_string()
}

fn slash_menu_items_for_input(
    input: &str,
    catalog: &SkillCatalog,
    active_skills: &BTreeMap<String, ExternalSkill>,
) -> Vec<String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return Vec::new();
    }

    let lower = trimmed.to_lowercase();

    // Context-sensitive completions for /use and /unuse
    if lower.starts_with("/use ") {
        let query = trimmed[5..].trim().to_lowercase();
        let mut items = Vec::new();
        for skill in catalog.values() {
            if query.is_empty() || skill.name.to_lowercase().contains(&query) {
                items.push(format!("/use {}", skill.name));
            }
        }
        return items;
    }

    if lower.starts_with("/unuse ") {
        let query = trimmed[7..].trim().to_lowercase();
        let mut items = vec!["/unuse all".to_string()];
        for skill in active_skills.values() {
            if query.is_empty() || skill.name.to_lowercase().contains(&query) {
                items.push(format!("/unuse {}", skill.name));
            }
        }
        return items;
    }

    // Orca-style slash palette: show built-in commands + direct /<skill-slug> entries.
    let mut items: Vec<String> = BASE_SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.starts_with(&lower))
        .map(|cmd| cmd.to_string())
        .collect();

    for skill in catalog.values() {
        let candidate = format!("/{}", skill.name);
        if candidate.to_lowercase().starts_with(&lower) {
            items.push(candidate);
        }
    }

    items.truncate(180);
    items
}

fn refresh_slash_menu(
    app: &mut App,
    config: &Config,
    model_state: &ModelCommandState,
    catalog: &SkillCatalog,
    active_skills: &BTreeMap<String, ExternalSkill>,
) {
    let items = if model_state.mode != ModelMenuMode::None {
        model_menu_items_for_input(config, model_state)
    } else {
        let input = app.input_text();
        slash_menu_items_for_input(&input, catalog, active_skills)
    };
    app.slash_menu_items = items;
    app.slash_menu_visible = !app.slash_menu_items.is_empty();
    if app.slash_menu_items.is_empty() {
        app.slash_menu_selected = 0;
        app.slash_menu_scroll = 0;
        return;
    }
    if app.slash_menu_selected >= app.slash_menu_items.len() {
        app.slash_menu_selected = app.slash_menu_items.len() - 1;
    }
    let window = 8usize;
    if app.slash_menu_selected < app.slash_menu_scroll {
        app.slash_menu_scroll = app.slash_menu_selected;
    } else if app.slash_menu_selected >= app.slash_menu_scroll + window {
        app.slash_menu_scroll = app.slash_menu_selected.saturating_sub(window - 1);
    }
}

fn accept_selected_slash_menu_item(app: &mut App) -> bool {
    if !app.slash_menu_visible || app.slash_menu_items.is_empty() {
        return false;
    }
    let picked = app.slash_menu_items[app.slash_menu_selected].clone();
    let with_space = if picked.starts_with("/use") || picked.starts_with("/unuse") {
        picked
    } else {
        format!("{picked} ")
    };
    app.set_input_text(with_space);
    true
}

fn handle_model_menu_enter(
    app: &mut App,
    config: &Config,
    model_state: &mut ModelCommandState,
    pending_gateway_restart_confirm: &mut bool,
) -> bool {
    if model_state.mode == ModelMenuMode::None {
        return false;
    }
    if app.slash_menu_items.is_empty() {
        app.chat_history.push(ChatEntry::Error(
            "No selectable items in model menu.".into(),
        ));
        model_state.mode = ModelMenuMode::None;
        app.slash_menu_visible = false;
        return true;
    }

    let picked = app.slash_menu_items[app.slash_menu_selected].clone();
    match model_state.mode {
        ModelMenuMode::ProviderSelect => {
            match picked.as_str() {
                "openrouter" => match fetch_openrouter_models(config) {
                    Ok(models) => {
                        model_state.openrouter_models = models;
                        model_state.mode = ModelMenuMode::OpenRouterModels;
                        model_state.query.clear();
                        app.set_input_text(String::new());
                        app.slash_menu_selected = 0;
                        app.slash_menu_scroll = 0;
                        app.chat_history.push(ChatEntry::SystemInfo(
                            "Provider selected: openrouter. Type to search models, use ↑/↓ to navigate, Enter to select."
                                .into(),
                        ));
                    }
                    Err(e) => {
                        app.chat_history.push(ChatEntry::Error(format!(
                            "Failed to load OpenRouter models: {e}"
                        )));
                        model_state.mode = ModelMenuMode::None;
                        app.slash_menu_visible = false;
                    }
                },
                "openai" => match fetch_openai_models(config) {
                    Ok(models) => {
                        model_state.openai_models = models;
                        model_state.mode = ModelMenuMode::OpenAiModels;
                        model_state.query.clear();
                        app.set_input_text(String::new());
                        app.slash_menu_selected = 0;
                        app.slash_menu_scroll = 0;
                        app.chat_history.push(ChatEntry::SystemInfo(
                            "Provider selected: openai. Type to search models, use ↑/↓ to navigate, Enter to select."
                                .into(),
                        ));
                    }
                    Err(e) => {
                        app.chat_history.push(ChatEntry::Error(format!(
                            "Failed to load OpenAI models: {e}"
                        )));
                        model_state.mode = ModelMenuMode::None;
                        app.slash_menu_visible = false;
                    }
                },
                "nvidia" => match fetch_nvidia_models(config) {
                    Ok(models) => {
                        model_state.nvidia_models = models;
                        model_state.mode = ModelMenuMode::NvidiaModels;
                        model_state.query.clear();
                        app.set_input_text(String::new());
                        app.slash_menu_selected = 0;
                        app.slash_menu_scroll = 0;
                        app.chat_history.push(ChatEntry::SystemInfo(
                            "Provider selected: nvidia. Type to search models, use ↑/↓ to navigate, Enter to select."
                                .into(),
                        ));
                    }
                    Err(e) => {
                        app.chat_history.push(ChatEntry::Error(format!(
                            "Failed to load NVIDIA models: {e}"
                        )));
                        model_state.mode = ModelMenuMode::None;
                        app.slash_menu_visible = false;
                    }
                },
                _ => {
                    model_state.mode = ModelMenuMode::None;
                    app.slash_menu_visible = false;
                    app.chat_history.push(ChatEntry::SystemInfo(format!(
                        "Provider '{}' selected. Interactive model picker is currently implemented for openrouter/openai/nvidia.",
                        picked
                    )));
                }
            }
            app.scroll_to_bottom();
            true
        }
        ModelMenuMode::OpenRouterModels
        | ModelMenuMode::OpenAiModels
        | ModelMenuMode::NvidiaModels => {
            let selected = picked;
            match persist_default_model(config, &selected) {
                Ok(path) => {
                    app.model_name = selected.clone();
                    app.chat_history.push(ChatEntry::SystemInfo(format!(
                        "Model set to {} and saved to {}",
                        selected,
                        path.display()
                    )));
                    match try_restart_gateway() {
                        Ok(msg) => app.chat_history.push(ChatEntry::SystemInfo(format!(
                            "Gateway restarted automatically after model change. {msg}"
                        ))),
                        Err(e) => app.chat_history.push(ChatEntry::Error(format!(
                            "Model saved, but automatic gateway restart failed: {e}"
                        ))),
                    }
                    *pending_gateway_restart_confirm = false;
                }
                Err(e) => {
                    app.chat_history.push(ChatEntry::Error(format!(
                        "Failed to persist model change: {e}"
                    )));
                }
            }
            model_state.mode = ModelMenuMode::None;
            model_state.query.clear();
            app.slash_menu_visible = false;
            app.slash_menu_items.clear();
            app.slash_menu_selected = 0;
            app.slash_menu_scroll = 0;
            app.set_input_text(String::new());
            app.scroll_to_bottom();
            true
        }
        ModelMenuMode::None => false,
    }
}

fn handle_slash_command(
    raw: &str,
    app: &mut App,
    config: &Config,
    model_state: &mut ModelCommandState,
    pending_gateway_restart_confirm: &mut bool,
    catalog: &mut SkillCatalog,
    active_skills: &mut BTreeMap<String, ExternalSkill>,
) -> SlashAction {
    let trimmed = raw.trim();
    let mut parts = trimmed.split_whitespace();
    let cmd = parts.next().unwrap_or("");

    match cmd {
        "/help" | "/?" => {
            app.chat_history.push(ChatEntry::SystemInfo(
                "Slash commands: /model, /skills, /skills rescan, /use <skill>, /unuse <skill|all>, /active-skills".into(),
            ));
            SlashAction::Continue
        }
        "/model" => {
            let target = parts.collect::<Vec<_>>().join(" ").trim().to_string();
            if target.is_empty() {
                model_state.mode = ModelMenuMode::ProviderSelect;
                model_state.query.clear();
                app.set_input_text(String::new());
                app.chat_history.push(ChatEntry::SystemInfo(
                    "Model picker: choose provider (↑/↓ + Enter).".into(),
                ));
                return SlashAction::Continue;
            }

            let selected = if let Ok(n) = target.parse::<usize>() {
                let catalog = match model_state.mode {
                    ModelMenuMode::OpenRouterModels => &model_state.openrouter_models,
                    ModelMenuMode::OpenAiModels => &model_state.openai_models,
                    ModelMenuMode::NvidiaModels => &model_state.nvidia_models,
                    _ => &model_state.openrouter_models,
                };
                if n == 0 || n > catalog.len() {
                    app.chat_history.push(ChatEntry::Error(format!(
                        "Model index out of range: {} (run /model to list)",
                        n
                    )));
                    return SlashAction::Continue;
                }
                catalog[n - 1].clone()
            } else {
                target
            };

            match persist_default_model(config, &selected) {
                Ok(path) => {
                    app.model_name = selected.clone();
                    app.chat_history.push(ChatEntry::SystemInfo(format!(
                        "Model set to {} and saved to {}",
                        selected,
                        path.display()
                    )));
                    match try_restart_gateway() {
                        Ok(msg) => app.chat_history.push(ChatEntry::SystemInfo(format!(
                            "Gateway restarted automatically after model change. {msg}"
                        ))),
                        Err(e) => app.chat_history.push(ChatEntry::Error(format!(
                            "Model saved, but automatic gateway restart failed: {e}"
                        ))),
                    }
                    *pending_gateway_restart_confirm = false;
                }
                Err(e) => {
                    app.chat_history.push(ChatEntry::Error(format!(
                        "Failed to persist model change: {e}"
                    )));
                }
            }
            SlashAction::Continue
        }
        "/skills" => {
            if matches!(parts.next(), Some("rescan")) {
                *catalog = discover_external_skills();
                app.discovered_skills_count = catalog.len();
                app.chat_history.push(ChatEntry::SystemInfo(format!(
                    "Rescanned skills: found {} SKILL.md files.",
                    catalog.len()
                )));
                return SlashAction::Continue;
            }
            if catalog.is_empty() {
                app.chat_history.push(ChatEntry::SystemInfo(
                    "No SKILL.md files found in known locations.".into(),
                ));
                return SlashAction::Continue;
            }
            let mut preview = String::from("Discovered skills:\n");
            for (i, skill) in catalog.values().take(60).enumerate() {
                preview.push_str(&format!(
                    "{}. {} ({})\n",
                    i + 1,
                    skill.name,
                    skill.path.display()
                ));
            }
            if catalog.len() > 60 {
                preview.push_str(&format!("... and {} more", catalog.len() - 60));
            }
            app.chat_history.push(ChatEntry::SystemInfo(preview));
            SlashAction::Continue
        }
        "/active-skills" => {
            if active_skills.is_empty() {
                app.chat_history
                    .push(ChatEntry::SystemInfo("No active skills.".into()));
            } else {
                let mut s = String::from("Active skills:\n");
                for skill in active_skills.values() {
                    s.push_str(&format!("- {}\n", skill.name));
                }
                app.chat_history.push(ChatEntry::SystemInfo(s));
            }
            SlashAction::Continue
        }
        "/use" => {
            let target = parts.collect::<Vec<_>>().join(" ");
            if target.is_empty() {
                app.chat_history.push(ChatEntry::Error(
                    "Usage: /use <skill name> (run /skills to list)".into(),
                ));
                return SlashAction::Continue;
            }
            if let Some(skill) = catalog.get(&target).cloned() {
                active_skills.insert(skill.name.clone(), skill.clone());
                app.chat_history.push(ChatEntry::SystemInfo(format!(
                    "Activated skill: {}",
                    skill.name
                )));
            } else {
                app.chat_history.push(ChatEntry::Error(format!(
                    "Skill '{}' not found. Use /skills or /skills rescan.",
                    target
                )));
            }
            SlashAction::Continue
        }
        "/unuse" => {
            let target = parts.collect::<Vec<_>>().join(" ");
            if target.eq_ignore_ascii_case("all") {
                active_skills.clear();
                app.chat_history
                    .push(ChatEntry::SystemInfo("Cleared all active skills.".into()));
            } else if target.is_empty() {
                app.chat_history
                    .push(ChatEntry::Error("Usage: /unuse <skill|all>".into()));
            } else if active_skills.remove(&target).is_some() {
                app.chat_history.push(ChatEntry::SystemInfo(format!(
                    "Deactivated skill: {target}"
                )));
            } else {
                app.chat_history
                    .push(ChatEntry::Error(format!("Skill not active: {target}")));
            }
            SlashAction::Continue
        }
        _ if cmd.starts_with('/') => {
            // Absolute/local filesystem paths must be treated as message input, not slash commands.
            if looks_like_filesystem_path_command(cmd) {
                return SlashAction::Send(raw.to_string());
            }

            // Orca-style direct skill slash activation: /<skill> [optional prompt]
            let skill_slug = cmd.trim_start_matches('/');
            if let Some(skill) = catalog.get(skill_slug).cloned() {
                active_skills.insert(skill.name.clone(), skill.clone());
                app.chat_history.push(ChatEntry::SystemInfo(format!(
                    "Activated skill: {} ({})",
                    skill.name,
                    skill.path.display()
                )));
                let remainder = parts.collect::<Vec<_>>().join(" ");
                if remainder.trim().is_empty() {
                    SlashAction::Continue
                } else {
                    SlashAction::Send(remainder)
                }
            } else {
                app.chat_history.push(ChatEntry::Error(format!(
                    "Unknown slash command or skill: {}. Use /skills to list discovered skills.",
                    cmd
                )));
                SlashAction::Continue
            }
        }
        _ => {
            let mut final_input = raw.to_string();
            if !active_skills.is_empty() {
                let mut preface = String::from("Active skill context (follow as guidance):\n");
                for skill in active_skills.values() {
                    preface.push_str(&format!("\n### SKILL: {}\n", skill.name));
                    // guard against runaway prompt bloat
                    let clipped: String = skill.content.chars().take(5000).collect();
                    preface.push_str(&clipped);
                    preface.push('\n');
                }
                preface.push_str("\n### USER REQUEST\n");
                preface.push_str(raw);
                final_input = preface;
            }
            SlashAction::Send(final_input)
        }
    }
}

/// Run the Hermes-style TUI REPL. Takes ownership of the agent loop and config.
pub async fn run_hermes_tui(mut agent_loop: AgentLoop, config: &Config) -> anyhow::Result<()> {
    // Setup terminal in alternate screen so shell scrollback/output cannot corrupt the TUI frame.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model_name = config.agent.default_model.clone();
    let token_budget = config.agent.token_budget;

    let mut app = App::new(model_name, token_budget);
    let event_handler = EventHandler::new(250);
    let mut history: Vec<Message> = Vec::new();
    let mut skill_catalog = discover_external_skills();
    app.discovered_skills_count = skill_catalog.len();
    let mut active_skills: BTreeMap<String, ExternalSkill> = BTreeMap::new();

    // Add Ferroclaw greeting
    app.chat_history.push(ChatEntry::AssistantMessage(
        "Hello! I'm Ferroclaw, your security-first AI assistant. How can I help you today?".into(),
    ));

    // Main loop
    let mut model_state = ModelCommandState::default();
    let mut pending_gateway_restart_confirm = false;

    let mut loop_ctx = RunLoopCtx {
        agent_loop: &mut agent_loop,
        config,
        history: &mut history,
        skill_catalog: &mut skill_catalog,
        active_skills: &mut active_skills,
        model_state: &mut model_state,
        pending_gateway_restart_confirm: &mut pending_gateway_restart_confirm,
    };

    let result = run_loop(&mut terminal, &mut app, &event_handler, &mut loop_ctx).await;

    // Restore terminal (always, even on error)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

struct RunLoopCtx<'a> {
    agent_loop: &'a mut AgentLoop,
    config: &'a Config,
    history: &'a mut Vec<Message>,
    skill_catalog: &'a mut SkillCatalog,
    active_skills: &'a mut BTreeMap<String, ExternalSkill>,
    model_state: &'a mut ModelCommandState,
    pending_gateway_restart_confirm: &'a mut bool,
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    ctx: &mut RunLoopCtx<'_>,
) -> anyhow::Result<()> {
    let agent_loop = &mut *ctx.agent_loop;
    let config = ctx.config;
    let history = &mut *ctx.history;
    let skill_catalog = &mut *ctx.skill_catalog;
    let active_skills = &mut *ctx.active_skills;
    let model_state = &mut *ctx.model_state;
    let pending_gateway_restart_confirm = &mut *ctx.pending_gateway_restart_confirm;

    loop {
        refresh_slash_menu(app, config, model_state, skill_catalog, active_skills);

        // Draw UI
        terminal.draw(|frame| draw_hermes(frame, app))?;

        // Handle events
        match event_handler.next()? {
            Event::Tick => {
                app.advance_shimmer();
                if app.is_running {
                    let elapsed = elapsed_ms_since(app.run_started_at);
                    app.verb = glitter_verb_for_llm_pending(elapsed, app.iteration);
                }
            }
            Event::MouseScrollUp => {
                app.scroll_up(3);
            }
            Event::MouseScrollDown => {
                app.scroll_down(3);
            }
            Event::Paste(raw) => {
                let mut pasted = normalize_pasted_payload(&raw);
                if pasted.trim().is_empty() {
                    continue;
                }
                // For drag/drop paths and URI pastes, separate from existing text with one space.
                if !app.input_text().is_empty()
                    && !app.input_text().ends_with(' ')
                    && !pasted.starts_with('\n')
                {
                    pasted = format!(" {pasted}");
                }
                app.input_insert_text(&pasted);
                continue;
            }
            Event::Key(key_event) => {
                use crossterm::event::KeyCode;
                use crossterm::event::KeyModifiers;

                let code = key_event.code;
                let modifiers = key_event.modifiers;

                // Task management disabled - shortcuts removed
                // if let Some(task_cmd) = Event::Key(key_event).as_task_command() {
                //     handle_task_command(app, task_cmd);
                //     continue;
                // }

                // Ctrl+C: quit
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                // Ctrl+L: clear chat
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('l') {
                    app.clear_chat();
                    continue;
                }

                // Esc: close slash/model menu popup
                if code == KeyCode::Esc {
                    model_state.mode = ModelMenuMode::None;
                    model_state.query.clear();
                    app.slash_menu_visible = false;
                    app.slash_menu_items.clear();
                    app.slash_menu_selected = 0;
                    app.slash_menu_scroll = 0;
                    app.set_input_text(String::new());
                    continue;
                }

                // PageUp / PageDown: scroll chat
                if code == KeyCode::PageUp {
                    app.scroll_up(10);
                    continue;
                }
                if code == KeyCode::PageDown {
                    app.scroll_down(10);
                    continue;
                }

                // Ctrl+Home / Ctrl+End: jump to top/bottom
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Home {
                    app.scroll_to_top();
                    continue;
                }
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::End {
                    app.scroll_to_bottom();
                    continue;
                }

                // Shift+Up / Shift+Down: scroll by 1
                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Up {
                    app.scroll_up(1);
                    continue;
                }
                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Down {
                    app.scroll_down(1);
                    continue;
                }

                // Enter: send message (Tab accepts slash suggestion)
                if code == KeyCode::Enter && !modifiers.contains(KeyModifiers::SHIFT) {
                    if handle_model_menu_enter(
                        app,
                        config,
                        model_state,
                        pending_gateway_restart_confirm,
                    ) {
                        continue;
                    }

                    let input = app.take_input();
                    if input.is_empty() {
                        continue;
                    }

                    if *pending_gateway_restart_confirm {
                        let answer = input.trim().to_ascii_lowercase();
                        match answer.as_str() {
                            "y" | "yes" => {
                                app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                                match try_restart_gateway() {
                                    Ok(msg) => app.chat_history.push(ChatEntry::SystemInfo(msg)),
                                    Err(e) => app.chat_history.push(ChatEntry::Error(format!(
                                        "Failed to restart gateway: {e}"
                                    ))),
                                }
                                *pending_gateway_restart_confirm = false;
                                app.scroll_to_bottom();
                                continue;
                            }
                            "n" | "no" => {
                                app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                                app.chat_history.push(ChatEntry::SystemInfo(
                                    "Ferroclaw Gateway restart skipped. Run `ferroclaw gateway restart --force` in another terminal when ready.".into(),
                                ));
                                *pending_gateway_restart_confirm = false;
                                app.scroll_to_bottom();
                                continue;
                            }
                            _ => {
                                app.chat_history.push(ChatEntry::SystemInfo(
                                    "No y/n received; skipping gateway restart and sending your message.".into(),
                                ));
                                *pending_gateway_restart_confirm = false;
                            }
                        }
                    }

                    let image_paths = local_image_paths_in_text(&input);
                    if !image_paths.is_empty() {
                        for path in image_paths {
                            let exists = Path::new(&path).is_file();
                            if !exists {
                                app.chat_history.push(ChatEntry::Error(format!(
                                    "Image path not found or not readable: {path}"
                                )));
                            } else {
                                app.chat_history.push(ChatEntry::SystemInfo(format!(
                                    "Detected local image path: {path}. Note: binary image upload from TUI path input is not yet supported; sending path text to the model."
                                )));
                            }
                        }
                    }

                    app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                    app.scroll_to_bottom();

                    match handle_slash_command(
                        &input,
                        app,
                        config,
                        model_state,
                        pending_gateway_restart_confirm,
                        skill_catalog,
                        active_skills,
                    ) {
                        SlashAction::Continue => {
                            app.set_status("Ready");
                            app.scroll_to_bottom();
                            continue;
                        }
                        SlashAction::Send(effective_input) => {
                            app.set_status("Thinking...");
                            app.iteration = 0;
                            app.is_running = true;
                            app.is_error = false;
                            app.run_started_at = Some(Instant::now());
                            app.verb = glitter_verb_for_llm_pending(0, app.iteration);

                            // Redraw with the user message visible
                            terminal.draw(|frame| draw_hermes(frame, app))?;

                            // Stream agent events in real time via callback and keep UI ticking.
                            let (event_tx, mut event_rx) =
                                tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
                            let run_fut = agent_loop.run_with_callback(
                                &effective_input,
                                history,
                                move |ev| {
                                    let _ = event_tx.send(ev.clone());
                                },
                            );
                            tokio::pin!(run_fut);

                            let run_result = loop {
                                tokio::select! {
                                    res = &mut run_fut => {
                                        while let Ok(ev) = event_rx.try_recv() {
                                            apply_agent_event(app, &ev);
                                        }
                                        break res;
                                    }
                                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                                        while let Ok(ev) = event_rx.try_recv() {
                                            apply_agent_event(app, &ev);
                                        }
                                        if app.is_running {
                                            let elapsed = elapsed_ms_since(app.run_started_at);
                                            if app.active_tools.is_empty() {
                                                app.verb = glitter_verb_for_llm_pending(elapsed, app.iteration);
                                            }
                                        }
                                        let _ = terminal.draw(|frame| draw_hermes(frame, app));
                                    }
                                }
                            };

                            app.is_running = false;
                            app.active_tools.clear();
                            app.run_started_at = None;

                            match run_result {
                                Ok(outcome) => {
                                    if !matches!(outcome.stop.reason, RunStopReason::AssistantFinal)
                                    {
                                        let mut msg = format!(
                                            "Run stop: {:?} · iterations={} · tools={} · elapsed={}ms",
                                            outcome.stop.reason,
                                            outcome.stop.iterations,
                                            outcome.stop.tool_calls_total,
                                            outcome.stop.elapsed_ms
                                        );
                                        if let Some(note) = &outcome.stop.notes {
                                            msg.push_str(&format!(" · note: {note}"));
                                        }
                                        app.chat_history.push(ChatEntry::SystemInfo(msg));
                                    }

                                    if outcome.text.trim().is_empty() {
                                        app.chat_history.push(ChatEntry::Error(
                                            "Assistant produced no visible text. This can happen when a stream is interrupted or response is truncated.".into(),
                                        ));
                                    } else {
                                        app.chat_history
                                            .push(ChatEntry::AssistantMessage(outcome.text));
                                    }

                                    app.set_status("Ready");
                                    app.verb = "Ready".to_string();
                                }
                                Err(e) => {
                                    let err_text = format!("{e}");
                                    app.chat_history.push(ChatEntry::Error(err_text.clone()));
                                    let lowered = err_text.to_ascii_lowercase();
                                    if lowered.contains("generatorexit")
                                        || lowered.contains("disconnect")
                                        || lowered.contains("connection closed")
                                        || lowered.contains("stream")
                                    {
                                        app.chat_history.push(ChatEntry::SystemInfo(
                                            "Stream appears to have been interrupted by client disconnect/cancellation. Check recent request logs for request_id correlation.".into(),
                                        ));
                                    }
                                    app.set_status("Error");
                                    app.is_error = true;
                                    app.verb = "Error".to_string();
                                }
                            }

                            app.scroll_to_bottom();
                            continue;
                        }
                    }
                }

                if model_state.mode != ModelMenuMode::None {
                    match code {
                        KeyCode::Char(c)
                            if !modifiers.contains(KeyModifiers::CONTROL)
                                && !modifiers.contains(KeyModifiers::ALT) =>
                        {
                            model_state.query.push(c);
                            continue;
                        }
                        KeyCode::Backspace => {
                            model_state.query.pop();
                            continue;
                        }
                        KeyCode::Delete => {
                            model_state.query.clear();
                            continue;
                        }
                        KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {
                            continue;
                        }
                        _ => {}
                    }
                }

                // Shift+Enter or Alt+Enter: newline in input
                if code == KeyCode::Enter && modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_newline();
                    continue;
                }

                // Backspace
                if code == KeyCode::Backspace {
                    app.input_backspace();
                    continue;
                }

                // Delete
                if code == KeyCode::Delete {
                    app.input_delete();
                    continue;
                }

                // Arrow keys for cursor movement in input (only if not task navigation)
                if code == KeyCode::Left {
                    app.input_move_left();
                    continue;
                }
                if code == KeyCode::Right {
                    app.input_move_right();
                    continue;
                }
                if code == KeyCode::Up && !modifiers.contains(KeyModifiers::SHIFT) {
                    if app.slash_menu_visible {
                        if app.slash_menu_selected > 0 {
                            app.slash_menu_selected -= 1;
                        }
                        refresh_slash_menu(app, config, model_state, skill_catalog, active_skills);
                    } else if app.input_is_blank() {
                        app.scroll_up(1);
                    } else {
                        app.input_move_up();
                    }
                    continue;
                }
                if code == KeyCode::Down && !modifiers.contains(KeyModifiers::SHIFT) {
                    if app.slash_menu_visible {
                        if app.slash_menu_selected + 1 < app.slash_menu_items.len() {
                            app.slash_menu_selected += 1;
                        }
                        refresh_slash_menu(app, config, model_state, skill_catalog, active_skills);
                    } else if app.input_is_blank() {
                        app.scroll_down(1);
                    } else {
                        app.input_move_down();
                    }
                    continue;
                }

                // Home / End
                if code == KeyCode::Home {
                    app.input_home();
                    continue;
                }
                if code == KeyCode::End {
                    app.input_end();
                    continue;
                }

                // Character input
                if let KeyCode::Char(c) = code {
                    app.input_char(c);
                }

                // Tab: accept slash suggestion, else insert 4 spaces
                if code == KeyCode::Tab {
                    if app.slash_menu_visible {
                        if model_state.mode == ModelMenuMode::None {
                            let _ = accept_selected_slash_menu_item(app);
                        }
                        refresh_slash_menu(app, config, model_state, skill_catalog, active_skills);
                    } else {
                        for _ in 0..4 {
                            app.input_char(' ');
                        }
                    }
                }
            }
            Event::Resize(_, _) => {
                // Terminal will redraw on next iteration
            }
        }
    }
}

/// Apply a single AgentEvent into ChatEntry/metrics state.
fn apply_agent_event(app: &mut App, event: &AgentEvent) {
    match event {
        AgentEvent::ToolCallStart { name, .. } => {
            app.chat_history.push(ChatEntry::ToolCall {
                name: name.clone(),
                args: String::new(),
            });
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "◦ reason: running {name}"
            )));
            app.iteration += 1;
            app.tool_call_count = app.tool_call_count.saturating_add(1);
            app.add_active_tool(name.clone());
            app.verb = glitter_verb_for_tool_call(name, app.tool_call_count, app.shimmer_phase);
        }
        AgentEvent::LlmRound { iteration } => {
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "⋯ thinking (round {iteration})"
            )));
            app.chat_history
                .push(ChatEntry::TranscriptLine("◦ reason: evaluating next step".to_string()));
            let elapsed = elapsed_ms_since(app.run_started_at);
            app.verb = glitter_verb_for_llm_pending(elapsed, *iteration);
        }
        AgentEvent::ModelToolChoice { names, .. } => {
            if !names.is_empty() {
                app.chat_history.push(ChatEntry::TranscriptLine(format!(
                    "◆ model selected tools: {}",
                    names.join(", ")
                )));
                let brief = if names.len() > 3 {
                    format!("{}, +{}", names[..3].join(", "), names.len() - 3)
                } else {
                    names.join(", ")
                };
                app.chat_history.push(ChatEntry::TranscriptLine(format!(
                    "◦ reason: selected tools for required action ({brief})"
                )));
            }
        }
        AgentEvent::ParallelToolBatch { count } => {
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "◆ parallel tool batch: {count}"
            )));
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "◦ reason: parallelized {count} tool call(s)"
            )));
        }
        AgentEvent::ToolResult {
            name,
            content,
            is_error,
            ..
        } => {
            app.chat_history.push(ChatEntry::ToolResult {
                name: name.clone(),
                content: content.clone(),
                is_error: *is_error,
            });
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "◦ reason: {} returned {}; deciding next step",
                name,
                if *is_error { "error" } else { "result" }
            )));
            app.remove_active_tool(name);
            let elapsed = elapsed_ms_since(app.run_started_at);
            app.verb = glitter_verb_for_llm_pending(elapsed, app.iteration);
        }
        AgentEvent::TokenUsage {
            input,
            output,
            total_used,
        } => {
            app.tokens_used = *total_used;
            app.last_input_tokens = *input;
            app.last_output_tokens = *output;
        }
        AgentEvent::Error(msg) => {
            app.chat_history.push(ChatEntry::Error(msg.clone()));
            app.is_error = true;
            app.verb = "Error".to_string();
        }
        AgentEvent::TextDelta(_) | AgentEvent::Done { .. } => {
            // Text deltas are already captured in the final response
        }
    }
}

/// Process AgentEvents into ChatEntry items for the TUI.
#[allow(dead_code)]
fn process_agent_events(app: &mut App, events: &[AgentEvent]) {
    for event in events {
        apply_agent_event(app, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_name_prefers_frontmatter_name() {
        let p = PathBuf::from("/tmp/some/path/SKILL.md");
        let c = "---\nname: my-skill\n---\nbody";
        assert_eq!(skill_name_from_path_or_frontmatter(&p, c), "my-skill");
    }

    #[test]
    fn skill_name_falls_back_to_parent_dir() {
        let p = PathBuf::from("/tmp/demo-skill/SKILL.md");
        let c = "# title only";
        assert_eq!(skill_name_from_path_or_frontmatter(&p, c), "demo-skill");
    }

    #[test]
    fn slash_menu_shows_base_commands_for_prefix() {
        let catalog = BTreeMap::new();
        let active = BTreeMap::new();
        let items = slash_menu_items_for_input("/s", &catalog, &active);
        assert!(items.contains(&"/skills".to_string()));
        assert!(items.contains(&"/skills rescan".to_string()));
    }

    #[test]
    fn slash_menu_expands_use_with_discovered_skills() {
        let mut catalog = BTreeMap::new();
        catalog.insert(
            "demo-skill".into(),
            ExternalSkill {
                name: "demo-skill".into(),
                path: PathBuf::from("/tmp/demo-skill/SKILL.md"),
                content: "---\nname: demo-skill\n---".into(),
            },
        );
        let active = BTreeMap::new();
        let items = slash_menu_items_for_input("/use dem", &catalog, &active);
        assert_eq!(items, vec!["/use demo-skill".to_string()]);
    }

    #[test]
    fn slash_menu_lists_direct_skill_slugs() {
        let mut catalog = BTreeMap::new();
        catalog.insert(
            "benchmark".into(),
            ExternalSkill {
                name: "benchmark".into(),
                path: PathBuf::from("/Users/ghost/.claude/skills/benchmark/SKILL.md"),
                content: "---\nname: benchmark\n---".into(),
            },
        );
        let active = BTreeMap::new();
        let items = slash_menu_items_for_input("/b", &catalog, &active);
        assert!(items.contains(&"/benchmark".to_string()));
    }

    #[test]
    fn normalize_pasted_file_uri_to_path() {
        let raw = "file:///Users/ghost/Desktop/My%20Image.png";
        assert_eq!(
            normalize_pasted_payload(raw),
            "/Users/ghost/Desktop/My Image.png"
        );
    }

    #[test]
    fn path_like_absolute_input_is_not_slash_command() {
        assert!(looks_like_filesystem_path_command(
            "/Users/ghost/Downloads/IMG_5236.PNG"
        ));
        assert!(!looks_like_filesystem_path_command("/skills"));
        assert!(!looks_like_filesystem_path_command("/use"));
    }

    #[test]
    fn local_image_paths_detect_shell_escaped_candidates() {
        let input = "/Users/ghost/Downloads/st\\,small\\,507x507-pad\\,600x600\\,f8f8f8.jpg";
        let paths = local_image_paths_in_text(input);
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0],
            "/Users/ghost/Downloads/st,small,507x507-pad,600x600,f8f8f8.jpg"
        );
    }

    #[test]
    fn model_provider_menu_starts_with_current_provider() {
        let mut config = Config::default();
        config.agent.default_model = "openai/gpt-5.3-codex".into();
        config.providers.openrouter = Some(crate::config::OpenRouterConfig {
            api_key_env: "OPENROUTER_API_KEY".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            site_url: None,
            site_name: None,
            max_tokens: 8192,
            request_timeout_ms: 15_000,
            max_retries: 2,
            no_retry_max_tokens_threshold: 128,
        });

        let items = configured_provider_menu_items(&config);
        assert!(!items.is_empty());
        assert_eq!(items[0], "openrouter");
    }

    #[test]
    fn model_provider_menu_includes_openai_openrouter_nvidia_when_configured() {
        let mut config = Config::default();
        config.providers.openrouter = Some(crate::config::OpenRouterConfig {
            api_key_env: "OPENROUTER_API_KEY".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            site_url: None,
            site_name: None,
            max_tokens: 8192,
            request_timeout_ms: 15_000,
            max_retries: 2,
            no_retry_max_tokens_threshold: 128,
        });
        config.providers.openai = Some(crate::config::OpenAiConfig::default());
        config.providers.nvidia = Some(crate::config::OpenAiConfig {
            api_key_env: "NVIDIA_API_KEY".into(),
            base_url: "https://integrate.api.nvidia.com/v1".into(),
            ..crate::config::OpenAiConfig::default()
        });

        let items = configured_provider_menu_items(&config);
        assert!(items.contains(&"openai".to_string()));
        assert!(items.contains(&"openrouter".to_string()));
        assert!(items.contains(&"nvidia".to_string()));
    }

    #[test]
    fn openrouter_model_menu_filters_by_search_query() {
        let state = ModelCommandState {
            mode: ModelMenuMode::OpenRouterModels,
            openrouter_models: vec![
                "openai/gpt-4o".into(),
                "openai/gpt-4o-mini".into(),
                "anthropic/claude-sonnet-4".into(),
            ],
            query: "mini".into(),
            ..ModelCommandState::default()
        };

        let items = model_menu_items_for_input(&Config::default(), &state);
        assert_eq!(items, vec!["openai/gpt-4o-mini".to_string()]);
    }

    #[test]
    fn openai_model_menu_filters_by_search_query() {
        let state = ModelCommandState {
            mode: ModelMenuMode::OpenAiModels,
            openai_models: vec!["gpt-4o-mini".into(), "gpt-5.3-codex".into(), "o3".into()],
            query: "5.3".into(),
            ..ModelCommandState::default()
        };

        let items = model_menu_items_for_input(&Config::default(), &state);
        assert_eq!(items, vec!["gpt-5.3-codex".to_string()]);
    }

    #[test]
    fn nvidia_model_menu_filters_by_search_query() {
        let state = ModelCommandState {
            mode: ModelMenuMode::NvidiaModels,
            nvidia_models: vec!["z-ai/glm4.7".into(), "meta/llama-3.1-70b-instruct".into()],
            query: "glm".into(),
            ..ModelCommandState::default()
        };

        let items = model_menu_items_for_input(&Config::default(), &state);
        assert_eq!(items, vec!["z-ai/glm4.7".to_string()]);
    }
}
