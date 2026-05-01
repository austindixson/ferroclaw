//! Skill loader — discovers and registers skills from bundled defaults and disk.

use crate::config::SkillsConfig;
use crate::error::{FerroError, Result};
use crate::skills::bundled::bundled_skills;
use crate::skills::executor::BashSkillHandler;
use crate::skills::manifest::{SkillCategory, SkillManifest, SkillType};
use crate::tool::ToolRegistry;
use crate::types::{ToolDefinition, ToolMeta, ToolSource};
use std::collections::HashMap;
use std::path::Path;

/// Load all skills (bundled + custom) and register them into the tool registry.
pub fn load_and_register_skills(
    registry: &mut ToolRegistry,
    config: &SkillsConfig,
) -> Result<SkillStats> {
    let mut stats = SkillStats::default();

    // Load bundled skills
    let bundled = bundled_skills();
    stats.bundled_total = bundled.len();

    // Load custom skills from disk
    let custom = if let Some(ref dir) = config.custom_skills_dir {
        load_custom_skills(dir)?
    } else {
        let default_dir = crate::config::config_dir().join("skills");
        if default_dir.exists() {
            load_custom_skills(&default_dir)?
        } else {
            Vec::new()
        }
    };
    stats.custom_total = custom.len();

    // Merge: custom skills override bundled skills with the same name
    let mut all_skills: HashMap<String, SkillManifest> = HashMap::new();
    for skill in bundled {
        all_skills.insert(skill.name.clone(), skill);
    }
    for skill in custom {
        all_skills.insert(skill.name.clone(), skill);
    }

    // Apply category filter
    let enabled_categories: Option<Vec<SkillCategory>> =
        config.enabled_categories.as_ref().map(|cats| {
            cats.iter()
                .filter_map(|c| serde_json::from_value(serde_json::Value::String(c.clone())).ok())
                .collect()
        });

    // Apply disabled skills filter
    let disabled: Vec<String> = config.disabled_skills.clone().unwrap_or_default();

    // Register each skill as a tool
    for (name, skill) in &all_skills {
        if !skill.enabled {
            stats.skipped += 1;
            continue;
        }

        if disabled.contains(name) {
            stats.skipped += 1;
            continue;
        }

        if let Some(ref cats) = enabled_categories
            && !cats.contains(&skill.category)
        {
            stats.skipped += 1;
            continue;
        }

        match register_skill(registry, skill) {
            Ok(_) => {
                stats.registered += 1;
                *stats.by_category.entry(skill.category).or_insert(0) += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to register skill '{}': {}", name, e);
                stats.failed += 1;
            }
        }
    }

    Ok(stats)
}

/// Register a single skill as a tool in the registry.
fn register_skill(registry: &mut ToolRegistry, skill: &SkillManifest) -> Result<()> {
    let meta = ToolMeta {
        definition: ToolDefinition {
            name: skill.name.clone(),
            description: skill.description.clone(),
            input_schema: skill.input_schema.clone(),
            server_name: None,
        },
        required_capabilities: skill.required_capabilities.clone(),
        source: ToolSource::Skill {
            path: format!("bundled:{}", skill.category.display_name()),
        },
    };

    let handler: Box<dyn crate::tool::ToolHandler> = match &skill.skill_type {
        SkillType::Bash { command_template } => {
            Box::new(BashSkillHandler::new(command_template.clone()))
        }
        SkillType::Native => {
            return Err(FerroError::Tool(format!(
                "Native skill '{}' must be registered with its own handler",
                skill.name
            )));
        }
        SkillType::McpWrapper { server, tool: _ } => {
            // MCP wrapper skills are registered as MCP tools
            let def = ToolDefinition {
                name: skill.name.clone(),
                description: skill.description.clone(),
                input_schema: skill.input_schema.clone(),
                server_name: Some(server.clone()),
            };
            registry.register_mcp_tool(def, server.clone());
            return Ok(());
        }
    };

    registry.register(meta, handler);
    Ok(())
}

/// Load custom skill manifests from a directory of TOML files.
pub fn load_custom_skills(dir: &Path) -> Result<Vec<SkillManifest>> {
    let mut skills = Vec::new();

    if !dir.exists() {
        return Ok(skills);
    }

    let entries = std::fs::read_dir(dir).map_err(|e| {
        FerroError::Config(format!(
            "Failed to read skills directory {}: {e}",
            dir.display()
        ))
    })?;

    for entry in entries {
        let entry = entry
            .map_err(|e| FerroError::Config(format!("Failed to read directory entry: {e}")))?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "toml") {
            match load_skill_file(&path) {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    tracing::warn!("Failed to load skill {}: {e}", path.display());
                }
            }
        }
    }

    Ok(skills)
}

/// Load a single skill manifest from a TOML file.
fn load_skill_file(path: &Path) -> Result<SkillManifest> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| FerroError::Config(format!("Failed to read {}: {e}", path.display())))?;

    let manifest: SkillManifest = toml::from_str(&content)
        .map_err(|e| FerroError::Config(format!("Failed to parse {}: {e}", path.display())))?;

    Ok(manifest)
}

/// Statistics from skill loading.
#[derive(Debug, Default)]
pub struct SkillStats {
    pub bundled_total: usize,
    pub custom_total: usize,
    pub registered: usize,
    pub skipped: usize,
    pub failed: usize,
    pub by_category: HashMap<SkillCategory, usize>,
}

impl std::fmt::Display for SkillStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} skills registered ({} bundled, {} custom, {} skipped, {} failed) across {} categories",
            self.registered,
            self.bundled_total,
            self.custom_total,
            self.skipped,
            self.failed,
            self.by_category.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::bundled::bundled_skills;

    #[test]
    fn test_bundled_skills_count() {
        let skills = bundled_skills();
        assert!(
            skills.len() >= 70,
            "Expected 70+ bundled skills, got {}",
            skills.len()
        );
    }

    #[test]
    fn test_bundled_skills_categories() {
        let skills = bundled_skills();
        let categories: std::collections::HashSet<SkillCategory> =
            skills.iter().map(|s| s.category).collect();
        assert!(
            categories.len() >= 16,
            "Expected 16+ categories, got {}",
            categories.len()
        );
    }

    #[test]
    fn test_skill_registration() {
        let mut registry = ToolRegistry::new();
        let config = SkillsConfig::default();
        let stats = load_and_register_skills(&mut registry, &config).unwrap();
        assert!(stats.registered >= 70);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_load_custom_nonexistent_dir() {
        let skills = load_custom_skills(Path::new("/nonexistent/path")).unwrap();
        assert!(skills.is_empty());
    }
}
