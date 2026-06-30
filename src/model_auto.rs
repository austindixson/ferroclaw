//! CLI + shared helpers: fetch provider catalogs and auto-pick the best Nemotron slug.

use crate::config::{self, Config};
use crate::tui::model_select::{auto_pick_provider, pick_recommended_from_catalog};
use std::path::PathBuf;
use std::time::Duration;

pub fn run_auto_pick(config: &Config) -> anyhow::Result<()> {
    let (provider, model) = fetch_recommended_model_slug(config)?;
    let path = persist_default_model(config, &model)?;
    println!("Auto-selected {model} from {provider} catalog.");
    println!("Saved to {}", path.display());
    Ok(())
}

pub fn fetch_recommended_model_slug(config: &Config) -> anyhow::Result<(String, String)> {
    let provider = auto_pick_provider(config)
        .ok_or_else(|| anyhow::anyhow!("No OpenRouter or NVIDIA provider configured"))?;

    let models = match provider {
        "nvidia" => fetch_nvidia_models(config)?,
        "openrouter" => fetch_openrouter_models(config)?,
        other => anyhow::bail!("Unsupported provider for auto-pick: {other}"),
    };

    let model = pick_recommended_from_catalog(&models)
        .ok_or_else(|| anyhow::anyhow!("Provider {provider} returned an empty model list"))?;

    Ok((provider.to_string(), model))
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

    std::fs::write(&path, toml::to_string_pretty(&root)?)?;
    Ok(path)
}
