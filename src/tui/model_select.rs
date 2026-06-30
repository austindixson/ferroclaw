//! Live model catalog helpers — pick Nemotron (or a sensible fallback) from provider APIs.

use crate::config::Config;

/// Prefer the newest Nemotron slug from a provider `/models` list.
pub fn pick_best_nemotron(models: &[String]) -> Option<String> {
    let mut candidates: Vec<&String> = models
        .iter()
        .filter(|m| m.to_ascii_lowercase().contains("nemotron"))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| rank_model_slug(b).cmp(&rank_model_slug(a)));
    candidates.first().map(|s| (*s).clone())
}

/// Prefer a free/fast slug when Nemotron is unavailable.
pub fn pick_fast_fallback(models: &[String]) -> Option<String> {
    if models.is_empty() {
        return None;
    }
    let mut sorted: Vec<&String> = models.iter().collect();
    sorted.sort_by(|a, b| rank_model_slug(b).cmp(&rank_model_slug(a)));
    sorted.first().map(|s| (*s).clone())
}

/// Choose the best available model from a fetched catalog (Nemotron first).
pub fn pick_recommended_from_catalog(models: &[String]) -> Option<String> {
    pick_best_nemotron(models).or_else(|| pick_fast_fallback(models))
}

fn rank_model_slug(id: &str) -> i64 {
    let l = id.to_ascii_lowercase();
    let mut score: i64 = 0;

    if l.contains("nemotron") {
        score += 10_000;
    }
    if l.contains("super") {
        score += 3_000;
    } else if l.contains("ultra") {
        score += 2_500;
    } else if l.contains("nano") {
        score += 500;
    }

    // nemotron-3-* beats nemotron-2-*
    if let Some(ver) = extract_major_version(&l) {
        score += ver * 500;
    }

    if l.contains(":free") || l.contains("-free") {
        score += 200;
    }

    // Longer slugs often encode newer variants (e.g. 120b-a12b).
    score += (l.len() as i64).min(120);

    score
}

fn extract_major_version(lower: &str) -> Option<i64> {
    // Match nemotron-3, nemotron-4, llama-3.1, etc.
    let mut digits = String::new();
    let mut after_dash = false;
    for ch in lower.chars() {
        if ch == '-' {
            after_dash = true;
            digits.clear();
            continue;
        }
        if after_dash {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else if !digits.is_empty() {
                break;
            }
        }
    }
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Which provider to query first for auto-pick (matches runtime routing).
pub fn auto_pick_provider(config: &Config) -> Option<&'static str> {
    let model = config.agent.default_model.to_ascii_lowercase();
    if config.providers.nvidia.is_some()
        && (model.starts_with("nvidia/")
            || model.starts_with("z-ai/")
            || model.starts_with("google/")
            || (model.contains('/') && config.providers.openrouter.is_none()))
    {
        return Some("nvidia");
    }
    if config.providers.openrouter.is_some() {
        return Some("openrouter");
    }
    if config.providers.nvidia.is_some() {
        return Some("nvidia");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newest_nemotron() {
        let models = vec![
            "meta/llama-3.1-8b".into(),
            "nvidia/nemotron-3-nano-4b-v1:free".into(),
            "nvidia/nemotron-3-super-120b-a12b:free".into(),
            "google/gemma-4-31b-it".into(),
        ];
        assert_eq!(
            pick_best_nemotron(&models).as_deref(),
            Some("nvidia/nemotron-3-super-120b-a12b:free")
        );
    }

    #[test]
    fn prefers_free_when_ranking() {
        let models = vec![
            "nvidia/nemotron-3-super-120b-a12b".into(),
            "nvidia/nemotron-3-super-120b-a12b:free".into(),
        ];
        assert_eq!(
            pick_best_nemotron(&models).as_deref(),
            Some("nvidia/nemotron-3-super-120b-a12b:free")
        );
    }

    #[test]
    fn fallback_when_no_nemotron() {
        let models = vec!["google/gemma-4-31b-it".into(), "meta/llama-3.1-70b".into()];
        let picked = pick_recommended_from_catalog(&models).unwrap();
        assert!(!picked.to_ascii_lowercase().contains("nemotron"));
    }
}
