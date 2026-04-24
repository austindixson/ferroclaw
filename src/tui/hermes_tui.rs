//! Hermes-style TUI module for Ferroclaw.
//!
//! Provides a chat interface similar to the Hermes agent TUI with:
//! - Dark theme
//! - Message bubbles (assistant: "Ferroclaw" header + text; user: orange dot + text)
//! - Bottom status bar with model/process info
//! - Left sidebar with task management

#[path = "hermes_ui.rs"]
mod hermes_ui;

use super::app::{App, ChatEntry};
use super::events::{Event, EventHandler};
use hermes_ui::draw as draw_hermes;

use crate::agent::AgentLoop;
use crate::agent::r#loop::AgentEvent;
use crate::config::{self, Config};
use crate::tui::glitter_verbs::{
    elapsed_ms_since, glitter_verb_for_llm_pending, glitter_verb_for_tool_call,
};
use crate::types::Message;

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

#[derive(Default)]
struct ModelCommandState {
    openrouter_models: Vec<String>,
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
        return Err(anyhow::anyhow!("OpenRouter /models returned {}", resp.status()));
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

fn persist_default_model(config: &Config, model: &str) -> anyhow::Result<PathBuf> {
    let path = config::config_dir().join("config.toml");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut root = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str::<toml::Value>(&content).unwrap_or_else(|_| toml::Value::Table(Default::default()))
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

    agent.insert("default_model".into(), toml::Value::String(model.to_string()));
    agent
        .entry("max_iterations")
        .or_insert_with(|| toml::Value::Integer(config.agent.max_iterations as i64));
    agent
        .entry("token_budget")
        .or_insert_with(|| toml::Value::Integer(config.agent.token_budget as i64));
    agent.entry("max_tool_calls_per_iteration").or_insert_with(|| {
        toml::Value::Integer(config.agent.max_tool_calls_per_iteration as i64)
    });
    agent
        .entry("max_tool_calls_total")
        .or_insert_with(|| toml::Value::Integer(config.agent.max_tool_calls_total as i64));

    std::fs::write(&path, toml::to_string_pretty(&root)?)?;
    Ok(path)
}

fn try_restart_gateway() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.display().to_string();
    let log_path = config::data_dir().join("gateway.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let script = format!(
        "pkill -f '{} serve' >/dev/null 2>&1 || true; nohup '{}' serve >> '{}' 2>&1 &",
        exe_str,
        exe_str,
        log_path.display()
    );

    let status = Command::new("sh").arg("-lc").arg(script).status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("gateway restart command exited with {}", status));
    }
    Ok(format!("Gateway restarted using {} serve", exe.display()))
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
        if let Some(resolved) = resolve_macos_alias_path(path) {
            if let Ok(content) = fs::read_to_string(&resolved) {
                return Some((content, resolved));
            }
        }
    }

    if let Ok(resolved) = fs::canonicalize(path) {
        if resolved != path {
            if let Ok(content) = fs::read_to_string(&resolved) {
                return Some((content, resolved));
            }
        }
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
    catalog: &SkillCatalog,
    active_skills: &BTreeMap<String, ExternalSkill>,
) {
    let input = app.input_text();
    let items = slash_menu_items_for_input(&input, catalog, active_skills);
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
                match fetch_openrouter_models(config) {
                    Ok(models) => {
                        model_state.openrouter_models = models;
                        let total = model_state.openrouter_models.len();
                        let mut s = format!(
                            "OpenRouter models loaded: {}\nSelect with /model <number> or /model <provider/model-id>\n",
                            total
                        );
                        for (idx, m) in model_state.openrouter_models.iter().take(40).enumerate() {
                            s.push_str(&format!("{:>2}. {}\n", idx + 1, m));
                        }
                        if total > 40 {
                            s.push_str(&format!("... and {} more\n", total - 40));
                        }
                        app.chat_history.push(ChatEntry::SystemInfo(s));
                    }
                    Err(e) => {
                        app.chat_history.push(ChatEntry::Error(format!(
                            "Failed to load OpenRouter models: {e}"
                        )));
                    }
                }
                return SlashAction::Continue;
            }

            let selected = if let Ok(n) = target.parse::<usize>() {
                if n == 0 || n > model_state.openrouter_models.len() {
                    app.chat_history.push(ChatEntry::Error(format!(
                        "Model index out of range: {} (run /model to list)",
                        n
                    )));
                    return SlashAction::Continue;
                }
                model_state.openrouter_models[n - 1].clone()
            } else {
                target
            };

            match persist_default_model(config, &selected) {
                Ok(path) => {
                    app.model_name = selected.clone();
                    *pending_gateway_restart_confirm = true;
                    app.chat_history.push(ChatEntry::SystemInfo(format!(
                        "Model set to {} and saved to {}\nRestart gateway to apply live runtime change? (y/n)",
                        selected,
                        path.display()
                    )));
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
                preview.push_str(&format!("{}. {} ({})\n", i + 1, skill.name, skill.path.display()));
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
                app.chat_history
                    .push(ChatEntry::SystemInfo(format!("Deactivated skill: {target}")));
            } else {
                app.chat_history
                    .push(ChatEntry::Error(format!("Skill not active: {target}")));
            }
            SlashAction::Continue
        }
        _ if cmd.starts_with('/') => {
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
                let mut preface = String::from(
                    "Active skill context (follow as guidance):\n",
                );
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

    let result = run_loop(
        &mut terminal,
        &mut app,
        &event_handler,
        &mut agent_loop,
        config,
        &mut history,
        &mut skill_catalog,
        &mut active_skills,
        &mut model_state,
        &mut pending_gateway_restart_confirm,
    )
    .await;

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

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    agent_loop: &mut AgentLoop,
    config: &Config,
    history: &mut Vec<Message>,
    skill_catalog: &mut SkillCatalog,
    active_skills: &mut BTreeMap<String, ExternalSkill>,
    model_state: &mut ModelCommandState,
    pending_gateway_restart_confirm: &mut bool,
) -> anyhow::Result<()> {
    loop {
        refresh_slash_menu(app, skill_catalog, active_skills);

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

                // Esc: close slash command menu popup
                if code == KeyCode::Esc {
                    app.slash_menu_visible = false;
                    app.slash_menu_items.clear();
                    app.slash_menu_selected = 0;
                    app.slash_menu_scroll = 0;
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
                    let input = app.take_input();
                    if input.is_empty() {
                        continue;
                    }

                    if *pending_gateway_restart_confirm {
                        let answer = input.trim().to_ascii_lowercase();
                        app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                        match answer.as_str() {
                            "y" | "yes" => match try_restart_gateway() {
                                Ok(msg) => app.chat_history.push(ChatEntry::SystemInfo(msg)),
                                Err(e) => app.chat_history.push(ChatEntry::Error(format!(
                                    "Failed to restart gateway: {e}"
                                ))),
                            },
                            "n" | "no" => app.chat_history.push(ChatEntry::SystemInfo(
                                "Gateway restart skipped. Run `/Users/ghost/.local/bin/ferroclaw serve` in another terminal when ready.".into(),
                            )),
                            _ => app.chat_history.push(ChatEntry::SystemInfo(
                                "Please answer y or n for gateway restart.".into(),
                            )),
                        }
                        if matches!(answer.as_str(), "y" | "yes" | "n" | "no") {
                            *pending_gateway_restart_confirm = false;
                        }
                        app.scroll_to_bottom();
                        continue;
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

                            // Stream agent events in real time via callback.
                            let run_result = agent_loop
                                .run_with_callback(&effective_input, history, |ev| {
                                    apply_agent_event(app, ev);
                                    let _ = terminal.draw(|frame| draw_hermes(frame, app));
                                })
                                .await;

                            app.is_running = false;
                            app.active_tools.clear();
                            app.run_started_at = None;

                            match run_result {
                                Ok(outcome) => {
                                    app.chat_history
                                        .push(ChatEntry::AssistantMessage(outcome.text));
                                    app.set_status("Ready");
                                    app.verb = "Ready".to_string();
                                }
                                Err(e) => {
                                    app.chat_history.push(ChatEntry::Error(format!("{e}")));
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
                        refresh_slash_menu(app, skill_catalog, active_skills);
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
                        refresh_slash_menu(app, skill_catalog, active_skills);
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
                        let _ = accept_selected_slash_menu_item(app);
                        refresh_slash_menu(app, skill_catalog, active_skills);
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
            app.iteration += 1;
            app.tool_call_count = app.tool_call_count.saturating_add(1);
            app.add_active_tool(name.clone());
            app.verb = glitter_verb_for_tool_call(name, app.tool_call_count, app.shimmer_phase);
        }
        AgentEvent::LlmRound { .. }
        | AgentEvent::ModelToolChoice { .. }
        | AgentEvent::ParallelToolBatch { .. } => {}
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
}
