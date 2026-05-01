pub mod anthropic;
pub mod openai;
pub mod openrouter;
pub mod streaming;
pub mod zai;

use crate::config::{Config, OpenAiConfig, resolve_env_var};
use crate::error::{FerroError, Result};
use crate::provider::LlmProvider;

fn resolve_openai_credential(cfg: &OpenAiConfig) -> Result<String> {
    if cfg.auth_mode.eq_ignore_ascii_case("oauth") {
        return resolve_env_var(&cfg.oauth_token_env);
    }

    match resolve_env_var(&cfg.api_key_env) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Graceful fallback: if OAuth token exists, use it.
            resolve_env_var(&cfg.oauth_token_env)
        }
    }
}

fn is_bare_codex_model(model_l: &str) -> bool {
    !model_l.contains('/')
        && (model_l.starts_with("codex-")
            || model_l.contains("-codex")
            || model_l.starts_with("gpt-5."))
}

fn openai_compatible_provider(
    cfg: &OpenAiConfig,
    prefer_codex_mode: bool,
) -> Result<Box<dyn LlmProvider>> {
    let token = resolve_openai_credential(cfg)?;
    let api_mode =
        if prefer_codex_mode || openai::OpenAiProvider::is_codex_backend_url(&cfg.base_url) {
            openai::OpenAiApiMode::CodexResponses
        } else {
            openai::OpenAiApiMode::ChatCompletions
        };

    Ok(Box::new(openai::OpenAiProvider::new(
        token,
        cfg.base_url.clone(),
        api_mode,
        cfg.max_tokens,
        cfg.request_timeout_ms,
        cfg.max_retries,
        cfg.no_retry_max_tokens_threshold,
    )))
}

/// Select the appropriate provider for a model string.
///
/// Routing order:
/// 1. Explicit prefixes for Orca-parity provider lanes (xai:, mistral:, azure:, etc)
/// 2. Zai GLM models (`glm-*`)
/// 3. OpenRouter models (`provider/model` format with `/`)
/// 4. Anthropic models (`claude-*`)
/// 5. OpenAI-compatible fallback
pub fn resolve_provider(model: &str, config: &Config) -> Result<Box<dyn LlmProvider>> {
    let model_l = model.to_ascii_lowercase();

    if (model_l.starts_with("openaicodex:")
        || model_l.starts_with("codex-")
        || is_bare_codex_model(&model_l))
        && let Some(cfg) = &config.providers.openai_codex
    {
        return openai_compatible_provider(cfg, true);
    }
    if (model_l.starts_with("google:") || model_l.starts_with("gemini-"))
        && let Some(cfg) = &config.providers.google
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("xai:") || model_l.starts_with("grok-"))
        && let Some(cfg) = &config.providers.xai
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("nvidia:")
        || model_l.starts_with("z-ai/")
        || model_l.starts_with("nvidia/"))
        && let Some(cfg) = &config.providers.nvidia
    {
        return openai_compatible_provider(cfg, false);
    }
    if model_l.starts_with("llamacpp:")
        && let Some(cfg) = &config.providers.llamacpp
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("mistral:") || model_l.starts_with("mistral-"))
        && let Some(cfg) = &config.providers.mistral
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("azure:") || model_l.starts_with("azure-openai:"))
        && let Some(cfg) = &config.providers.azure_openai
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("copilot:") || model_l.starts_with("githubcopilot:"))
        && let Some(cfg) = &config.providers.github_copilot
    {
        return openai_compatible_provider(cfg, false);
    }
    if (model_l.starts_with("vertex:") || model_l.starts_with("googlevertex:"))
        && let Some(cfg) = &config.providers.google_vertex
    {
        return openai_compatible_provider(cfg, false);
    }
    if model_l.starts_with("bedrock:")
        && let Some(cfg) = &config.providers.bedrock
    {
        return openai_compatible_provider(cfg, false);
    }

    // Zai GLM models
    if zai::is_zai_model(model) {
        let zai_cfg = config
            .providers
            .zai
            .as_ref()
            .ok_or_else(|| FerroError::Config("Zai provider not configured".into()))?;
        let api_key = resolve_env_var(&zai_cfg.api_key_env)?;
        return Ok(Box::new(zai::ZaiProvider::new(
            api_key,
            zai_cfg.base_url.clone(),
            zai_cfg.request_timeout_ms,
            zai_cfg.max_retries,
            zai_cfg.no_retry_max_tokens_threshold,
        )));
    }

    // OpenRouter models (provider/model format)
    if openrouter::is_openrouter_model(model) {
        let or_cfg = config
            .providers
            .openrouter
            .as_ref()
            .ok_or_else(|| FerroError::Config("OpenRouter provider not configured".into()))?;
        let api_key = resolve_env_var(&or_cfg.api_key_env)?;
        return Ok(Box::new(openrouter::OpenRouterProvider::new(
            api_key,
            or_cfg.base_url.clone(),
            or_cfg.site_url.clone(),
            or_cfg.site_name.clone(),
            or_cfg.request_timeout_ms,
            or_cfg.max_retries,
            or_cfg.no_retry_max_tokens_threshold,
        )));
    }

    // Anthropic models
    if model.starts_with("claude-") {
        let anthropic_cfg = config
            .providers
            .anthropic
            .as_ref()
            .ok_or_else(|| FerroError::Config("Anthropic provider not configured".into()))?;
        let api_key = resolve_env_var(&anthropic_cfg.api_key_env)?;
        return Ok(Box::new(anthropic::AnthropicProvider::new(
            api_key,
            anthropic_cfg.base_url.clone(),
            anthropic_cfg.max_tokens,
            anthropic_cfg.request_timeout_ms,
            anthropic_cfg.max_retries,
            anthropic_cfg.no_retry_max_tokens_threshold,
        )));
    }

    // OpenAI-compatible fallback
    if let Some(openai_cfg) = &config.providers.openai {
        return openai_compatible_provider(openai_cfg, false);
    }

    // If only OpenAI Codex lane is configured, allow bare OpenAI/Codex model names.
    if let Some(codex_cfg) = &config.providers.openai_codex {
        return openai_compatible_provider(codex_cfg, true);
    }

    Err(FerroError::Provider(format!(
        "No provider configured for model '{model}'"
    )))
}
