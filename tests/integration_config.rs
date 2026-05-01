//! Integration tests for configuration loading, validation, and provider routing.

use ferroclaw::config::{self, Config, OpenRouterConfig, ZaiConfig};
use ferroclaw::providers;

#[test]
fn test_default_config_is_valid() {
    let config = Config::default();
    assert_eq!(config.agent.default_model, "claude-sonnet-4-20250514");
    assert_eq!(config.agent.max_iterations, 150);
    assert_eq!(config.agent.token_budget, 200_000);
    assert_eq!(config.gateway.bind, "127.0.0.1");
    assert_eq!(config.gateway.port, 8420);
    assert!(config.security.audit_enabled);
    assert!(config.security.require_skill_signatures);
    assert!(config.providers.anthropic.is_some());
    assert!(config.providers.openai.is_none());
    assert!(config.providers.zai.is_none());
    assert!(config.providers.openrouter.is_none());
}

#[test]
fn test_example_config_roundtrip() {
    let example = config::generate_example_config();
    let parsed: Config = toml::from_str(&example).expect("Example config must parse");
    let reserialized = toml::to_string_pretty(&parsed).expect("Config must serialize");
    let reparsed: Config = toml::from_str(&reserialized).expect("Re-serialized config must parse");
    assert_eq!(parsed.agent.default_model, reparsed.agent.default_model);
    assert_eq!(parsed.gateway.port, reparsed.gateway.port);
}

#[test]
fn test_config_with_all_providers() {
    let toml_str = r#"
[agent]
default_model = "glm-5"

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"

[providers.openai]
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"

[providers.zai]
api_key_env = "ZAI_API_KEY"
base_url = "https://api.z.ai/api/paas/v4"

[providers.openrouter]
api_key_env = "OPENROUTER_API_KEY"
base_url = "https://openrouter.ai/api/v1"
site_url = "https://example.com"
site_name = "Test App"
"#;
    let config: Config = toml::from_str(toml_str).expect("Multi-provider config must parse");
    assert!(config.providers.anthropic.is_some());
    assert!(config.providers.openai.is_some());
    assert!(config.providers.zai.is_some());
    assert!(config.providers.openrouter.is_some());

    let or = config.providers.openrouter.unwrap();
    assert_eq!(or.site_url.as_deref(), Some("https://example.com"));
    assert_eq!(or.site_name.as_deref(), Some("Test App"));
}

#[test]
fn test_provider_routing_anthropic() {
    let config = Config::default();
    let result = providers::resolve_provider("claude-sonnet-4-20250514", &config);
    // If ANTHROPIC_API_KEY is set in env, provider resolves successfully.
    // If not, we get a config error. Either outcome validates correct routing.
    match result {
        Ok(provider) => {
            assert_eq!(provider.name(), "anthropic");
        }
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("ANTHROPIC_API_KEY") || err.contains("not set"),
                "Expected API key error, got: {err}"
            );
        }
    }
}

#[test]
fn test_provider_routing_zai_requires_config() {
    let config = Config::default();
    match providers::resolve_provider("glm-5", &config) {
        Ok(_) => panic!("Expected Zai config error"),
        Err(e) => {
            let err = e.to_string();
            assert!(err.contains("Zai"), "Expected Zai config error, got: {err}");
        }
    }
}

#[test]
fn test_provider_routing_openrouter_requires_config() {
    let config = Config::default();
    match providers::resolve_provider("openai/gpt-4o", &config) {
        Ok(_) => panic!("Expected OpenRouter config error"),
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("OpenRouter"),
                "Expected OpenRouter error, got: {err}"
            );
        }
    }
}

#[test]
fn test_provider_routing_unknown_model_falls_through() {
    let config = Config::default();
    match providers::resolve_provider("some-random-model", &config) {
        Ok(_) => panic!("Expected fallthrough error"),
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("No provider configured"),
                "Expected fallthrough, got: {err}"
            );
        }
    }
}

#[test]
fn test_gateway_security_defaults() {
    let config = Config::default();
    assert_eq!(config.gateway.bind, "127.0.0.1");
    assert!(config.gateway.bearer_token.is_none());
    // Default config is safe: binds localhost only
    let gw = ferroclaw::gateway::Gateway::from_config(&config);
    assert!(gw.validate_bind_safety().is_ok());
}

#[test]
fn test_gateway_blocks_open_bind_without_token() {
    let mut config = Config::default();
    config.gateway.bind = "0.0.0.0".into();
    config.gateway.bearer_token = None;
    config.gateway.bearer_token_env = None;
    let gw = ferroclaw::gateway::Gateway::from_config(&config);
    assert!(gw.validate_bind_safety().is_err());
}

#[test]
fn test_gateway_allows_open_bind_with_token() {
    let mut config = Config::default();
    config.gateway.bind = "0.0.0.0".into();
    config.gateway.bearer_token = Some("my-secret-token".into());
    let gw = ferroclaw::gateway::Gateway::from_config(&config);
    assert!(gw.validate_bind_safety().is_ok());
}

#[test]
fn test_default_capabilities_are_safe() {
    let config = Config::default();
    let caps = &config.security.default_capabilities;
    // Default should NOT include process_exec, fs_write, net_listen, or browser
    use ferroclaw::types::Capability;
    assert!(caps.contains(&Capability::FsRead));
    assert!(caps.contains(&Capability::NetOutbound));
    assert!(caps.contains(&Capability::MemoryRead));
    assert!(caps.contains(&Capability::MemoryWrite));
    assert!(!caps.contains(&Capability::FsWrite));
    assert!(!caps.contains(&Capability::ProcessExec));
    assert!(!caps.contains(&Capability::NetListen));
    assert!(!caps.contains(&Capability::BrowserControl));
}

#[test]
fn test_zai_config_defaults() {
    let toml_str = r#"
[providers.zai]
api_key_env = "ZAI_API_KEY"
"#;

    #[derive(serde::Deserialize)]
    struct Wrapper {
        providers: ProvWrapper,
    }
    #[derive(serde::Deserialize)]
    struct ProvWrapper {
        zai: ZaiConfig,
    }

    let w: Wrapper = toml::from_str(toml_str).unwrap();
    assert_eq!(w.providers.zai.base_url, "https://api.z.ai/api/paas/v4");
}

#[test]
fn test_openrouter_config_defaults() {
    let toml_str = r#"
[providers.openrouter]
api_key_env = "OPENROUTER_API_KEY"
"#;

    #[derive(serde::Deserialize)]
    struct Wrapper {
        providers: ProvWrapper,
    }
    #[derive(serde::Deserialize)]
    struct ProvWrapper {
        openrouter: OpenRouterConfig,
    }

    let w: Wrapper = toml::from_str(toml_str).unwrap();
    assert_eq!(
        w.providers.openrouter.base_url,
        "https://openrouter.ai/api/v1"
    );
    assert!(w.providers.openrouter.site_url.is_none());
    assert!(w.providers.openrouter.site_name.is_none());
}
