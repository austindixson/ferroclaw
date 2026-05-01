//! File-based schema cache with TTL.
//!
//! Mirrors dietmcp's cache strategy: SHA256 key from (server_name + config),
//! atomic writes to prevent corruption, configurable TTL per server.

use crate::config::cache_dir;
use crate::error::Result;
use crate::types::ToolDefinition;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    server_name: String,
    config_key: String,
    cached_at: DateTime<Utc>,
    ttl_seconds: u64,
    tools: Vec<ToolDefinition>,
}

pub struct SchemaCache {
    cache_dir: PathBuf,
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaCache {
    pub fn new() -> Self {
        let dir = cache_dir().join("schemas");
        Self { cache_dir: dir }
    }

    pub fn with_dir(dir: PathBuf) -> Self {
        Self { cache_dir: dir }
    }

    /// Get cached tools for a server, if present and not expired.
    pub fn get(
        &self,
        server_name: &str,
        config_fingerprint: &str,
        ttl: u64,
    ) -> Option<Vec<ToolDefinition>> {
        let key = self.make_key(server_name, config_fingerprint);
        let path = self.cache_dir.join(format!("{key}.json"));

        let content = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&content).ok()?;

        // Check TTL (>= means TTL=0 always expires)
        let elapsed = Utc::now()
            .signed_duration_since(entry.cached_at)
            .num_seconds() as u64;
        if elapsed >= ttl {
            return None;
        }

        Some(entry.tools)
    }

    /// Store tools in cache with atomic write.
    pub fn put(
        &self,
        server_name: &str,
        config_fingerprint: &str,
        ttl: u64,
        tools: &[ToolDefinition],
    ) -> Result<()> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&self.cache_dir)?;

        let key = self.make_key(server_name, config_fingerprint);
        let path = self.cache_dir.join(format!("{key}.json"));

        let entry = CacheEntry {
            server_name: server_name.to_string(),
            config_key: key.clone(),
            cached_at: Utc::now(),
            ttl_seconds: ttl,
            tools: tools.to_vec(),
        };

        let content = serde_json::to_string_pretty(&entry)?;

        // Atomic write: write to temp file, then rename
        let tmp_path = self.cache_dir.join(format!("{key}.tmp"));
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Invalidate cache for a specific server.
    pub fn invalidate(&self, server_name: &str, config_fingerprint: &str) {
        let key = self.make_key(server_name, config_fingerprint);
        let path = self.cache_dir.join(format!("{key}.json"));
        let _ = std::fs::remove_file(path);
    }

    /// Clear all cached schemas.
    pub fn clear_all(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    fn make_key(&self, server_name: &str, config_fingerprint: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(server_name.as_bytes());
        hasher.update(config_fingerprint.as_bytes());
        let hash = hasher.finalize();
        hex::encode(&hash[..8]) // 16-char hex prefix
    }
}

// We need hex encoding but don't want another dependency.
// Minimal inline hex encoder.
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Generate a fingerprint for an MCP server config (for cache invalidation).
pub fn config_fingerprint(command: Option<&str>, args: &[String], url: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    if let Some(cmd) = command {
        hasher.update(cmd.as_bytes());
    }
    for arg in args {
        hasher.update(arg.as_bytes());
    }
    if let Some(u) = url {
        hasher.update(u.as_bytes());
    }
    let hash = hasher.finalize();
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn make_test_tools() -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            input_schema: json!({"type": "object"}),
            server_name: Some("test".into()),
        }]
    }

    #[test]
    fn test_cache_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let cache = SchemaCache::with_dir(tmp.path().to_path_buf());
        let tools = make_test_tools();

        cache.put("test", "fp123", 3600, &tools).unwrap();

        let cached = cache.get("test", "fp123", 3600);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_cache_expired() {
        let tmp = TempDir::new().unwrap();
        let cache = SchemaCache::with_dir(tmp.path().to_path_buf());
        let tools = make_test_tools();

        cache.put("test", "fp123", 0, &tools).unwrap(); // TTL = 0

        // Wait briefly to ensure elapsed > 0
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Should be expired
        let cached = cache.get("test", "fp123", 0);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_invalidate() {
        let tmp = TempDir::new().unwrap();
        let cache = SchemaCache::with_dir(tmp.path().to_path_buf());
        let tools = make_test_tools();

        cache.put("test", "fp123", 3600, &tools).unwrap();
        cache.invalidate("test", "fp123");

        let cached = cache.get("test", "fp123", 3600);
        assert!(cached.is_none());
    }

    #[test]
    fn test_config_fingerprint() {
        let fp1 = config_fingerprint(Some("npx"), &["arg1".into()], None);
        let fp2 = config_fingerprint(Some("npx"), &["arg2".into()], None);
        assert_ne!(fp1, fp2);

        let fp3 = config_fingerprint(Some("npx"), &["arg1".into()], None);
        assert_eq!(fp1, fp3);
    }
}
