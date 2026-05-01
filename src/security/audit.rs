//! Append-only, hash-chained audit log.
//!
//! Every tool call is logged with timestamp, args hash, and result hash.
//! Each entry includes a hash of the previous entry, creating a tamper-evident chain.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub arguments_hash: String,
    pub result_hash: String,
    pub is_error: bool,
    pub previous_hash: String,
    pub entry_hash: String,
}

pub struct AuditLog {
    path: PathBuf,
    last_hash: String,
    enabled: bool,
}

impl AuditLog {
    pub fn new(path: PathBuf, enabled: bool) -> Self {
        let last_hash = Self::read_last_hash(&path);
        Self {
            path,
            last_hash,
            enabled,
        }
    }

    pub fn disabled() -> Self {
        Self {
            path: PathBuf::new(),
            last_hash: String::new(),
            enabled: false,
        }
    }

    /// Log a tool call. Arguments and results are hashed, not stored in full.
    pub fn log_tool_call(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        is_error: bool,
    ) {
        if !self.enabled {
            return;
        }

        let arguments_hash = hash_content(arguments);
        let result_hash = hash_content(result);

        let entry_content = format!(
            "{}|{}|{}|{}|{}",
            tool_name, arguments_hash, result_hash, is_error, self.last_hash
        );
        let entry_hash = hash_content(&entry_content);

        let entry = AuditEntry {
            timestamp: Utc::now(),
            tool_name: tool_name.to_string(),
            arguments_hash,
            result_hash,
            is_error,
            previous_hash: self.last_hash.clone(),
            entry_hash: entry_hash.clone(),
        };

        self.last_hash = entry_hash;

        // Append to file
        if let Err(e) = self.append_entry(&entry) {
            tracing::warn!("Failed to write audit log: {e}");
        }
    }

    /// Verify the integrity of the audit log chain.
    pub fn verify(&self) -> Result<VerifyResult, std::io::Error> {
        if !self.enabled || !self.path.exists() {
            return Ok(VerifyResult {
                entries: 0,
                valid: true,
                first_invalid: None,
            });
        }

        let content = std::fs::read_to_string(&self.path)?;
        let mut previous_hash = String::new();
        let mut count = 0;

        for (i, line) in content.lines().enumerate() {
            if line.is_empty() {
                continue;
            }

            let entry: AuditEntry = match serde_json::from_str(line) {
                Ok(e) => e,
                Err(_) => {
                    return Ok(VerifyResult {
                        entries: count,
                        valid: false,
                        first_invalid: Some(i),
                    });
                }
            };

            if entry.previous_hash != previous_hash {
                return Ok(VerifyResult {
                    entries: count,
                    valid: false,
                    first_invalid: Some(i),
                });
            }

            let expected_content = format!(
                "{}|{}|{}|{}|{}",
                entry.tool_name,
                entry.arguments_hash,
                entry.result_hash,
                entry.is_error,
                entry.previous_hash
            );
            let expected_hash = hash_content(&expected_content);

            if entry.entry_hash != expected_hash {
                return Ok(VerifyResult {
                    entries: count,
                    valid: false,
                    first_invalid: Some(i),
                });
            }

            previous_hash = entry.entry_hash;
            count += 1;
        }

        Ok(VerifyResult {
            entries: count,
            valid: true,
            first_invalid: None,
        })
    }

    fn append_entry(&self, entry: &AuditEntry) -> Result<(), std::io::Error> {
        use std::fs::OpenOptions;
        use std::io::Write;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let json = serde_json::to_string(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{json}")?;
        Ok(())
    }

    fn read_last_hash(path: &Path) -> String {
        if !path.exists() {
            return String::new();
        }
        if let Ok(content) = std::fs::read_to_string(path)
            && let Some(last_line) = content.lines().rev().find(|l| !l.is_empty())
            && let Ok(entry) = serde_json::from_str::<AuditEntry>(last_line)
        {
            return entry.entry_hash;
        }
        String::new()
    }
}

pub struct VerifyResult {
    pub entries: usize,
    pub valid: bool,
    pub first_invalid: Option<usize>,
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_content() {
        let h1 = hash_content("hello");
        let h2 = hash_content("hello");
        let h3 = hash_content("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64); // SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_audit_log_write_and_verify() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("audit.jsonl");

        let mut log = AuditLog::new(path.clone(), true);
        log.log_tool_call("read_file", "{\"path\":\"/tmp\"}", "file contents", false);
        log.log_tool_call("write_file", "{\"path\":\"/tmp/out\"}", "ok", false);
        log.log_tool_call("bash", "{\"cmd\":\"ls\"}", "error: denied", true);

        let result = log.verify().unwrap();
        assert!(result.valid);
        assert_eq!(result.entries, 3);
    }

    #[test]
    fn test_audit_log_tamper_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("audit.jsonl");

        let mut log = AuditLog::new(path.clone(), true);
        log.log_tool_call("read_file", "{}", "ok", false);
        log.log_tool_call("write_file", "{}", "ok", false);

        // Tamper with the file
        let content = std::fs::read_to_string(&path).unwrap();
        let tampered = content.replace("read_file", "evil_tool");
        std::fs::write(&path, tampered).unwrap();

        let log2 = AuditLog::new(path, true);
        let result = log2.verify().unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_disabled_log() {
        let mut log = AuditLog::disabled();
        log.log_tool_call("test", "{}", "ok", false); // Should not panic
        let result = log.verify().unwrap();
        assert!(result.valid);
        assert_eq!(result.entries, 0);
    }
}
