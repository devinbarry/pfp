use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::PfpError;

const DEFAULT_MAX_LOG_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_ROTATED_LOG_FILES: usize = 10;
const LOG_MAX_BYTES_ENV: &str = "PFP_LOG_MAX_BYTES";

#[derive(Debug, Serialize)]
pub struct InvocationRecord {
    pub version: &'static str,
    pub ts: String,
    pub command: String,
    pub args: Value,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
}

fn default_log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".pfp")
}

fn log_file_path() -> PathBuf {
    default_log_dir().join("pfp.jsonl")
}

/// Build an invocation record from command results.
pub fn make_entry(
    command: &str,
    args: Value,
    result: &Result<(), PfpError>,
    duration_ms: u64,
) -> InvocationRecord {
    InvocationRecord {
        version: env!("CARGO_PKG_VERSION"),
        ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        command: command.to_string(),
        args,
        outcome: if result.is_ok() { "ok" } else { "error" },
        error: result.as_ref().err().map(|e| e.to_string()),
        duration_ms,
    }
}

/// Log a CLI invocation. Errors are printed to stderr but never fail the process.
pub fn log_invocation(command: &str, args: Value, result: &Result<(), PfpError>, duration_ms: u64) {
    let entry = make_entry(command, args, result, duration_ms);
    log_entry_to(&entry, &log_file_path());
}

/// Write an invocation record to a specific path (for testing).
fn log_entry_to(entry: &InvocationRecord, path: &Path) {
    log_entry_to_with_rotation(
        entry,
        path,
        configured_max_log_file_bytes(),
        MAX_ROTATED_LOG_FILES,
    );
}

fn configured_max_log_file_bytes() -> u64 {
    std::env::var(LOG_MAX_BYTES_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|bytes| *bytes > 0)
        .unwrap_or(DEFAULT_MAX_LOG_FILE_BYTES)
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn rotate_logs(path: &Path, keep_files: usize) -> io::Result<()> {
    if keep_files == 0 {
        return Ok(());
    }

    let oldest = rotated_log_path(path, keep_files);
    if oldest.exists() {
        fs::remove_file(oldest)?;
    }

    for index in (1..keep_files).rev() {
        let src = rotated_log_path(path, index);
        if src.exists() {
            let dst = rotated_log_path(path, index + 1);
            fs::rename(src, dst)?;
        }
    }

    if path.exists() {
        fs::rename(path, rotated_log_path(path, 1))?;
    }

    Ok(())
}

fn maybe_rotate_before_append(
    path: &Path,
    next_entry_len: usize,
    max_bytes: u64,
    keep_files: usize,
) -> io::Result<()> {
    if max_bytes == 0 {
        return Ok(());
    }

    let current_bytes = match fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };

    let projected = current_bytes
        .saturating_add(next_entry_len as u64)
        .saturating_add(1);
    if projected > max_bytes {
        rotate_logs(path, keep_files)?;
    }

    Ok(())
}

fn log_entry_to_with_rotation(
    entry: &InvocationRecord,
    path: &Path,
    max_bytes: u64,
    keep_files: usize,
) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("pfp: failed to create log directory: {e}");
            return;
        }
    }

    let json = match serde_json::to_string(entry) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("pfp: failed to serialize log entry: {e}");
            return;
        }
    };

    if let Err(e) = maybe_rotate_before_append(path, json.len(), max_bytes, keep_files) {
        eprintln!("pfp: failed to rotate log files: {e}");
    }

    let mut file = match OpenOptions::new().create(true).append(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("pfp: failed to open log file: {e}");
            return;
        }
    };

    if let Err(e) = writeln!(file, "{json}") {
        eprintln!("pfp: failed to write log entry: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("test-logs-{label}-{nanos}"))
    }

    fn sample_entry(command: &str, outcome: &'static str) -> InvocationRecord {
        InvocationRecord {
            version: env!("CARGO_PKG_VERSION"),
            ts: "2026-04-02T10:00:00.000Z".to_string(),
            command: command.to_string(),
            args: serde_json::json!({"query": "my-deploy"}),
            outcome,
            error: if outcome == "error" {
                Some("something broke".to_string())
            } else {
                None
            },
            duration_ms: 42,
        }
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = sample_entry("ls", "ok");
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"version\":\""));
        assert!(json.contains("\"command\":\"ls\""));
        assert!(json.contains("\"outcome\":\"ok\""));
        assert!(json.contains("\"duration_ms\":42"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_error_entry_includes_error_field() {
        let entry = sample_entry("run", "error");
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"outcome\":\"error\""));
        assert!(json.contains("\"error\":\"something broke\""));
    }

    #[test]
    fn test_log_entry_writes_to_file() {
        let dir = unique_test_dir("basic-write");
        let path = dir.join("test.jsonl");

        let entry = sample_entry("ls", "ok");
        log_entry_to(&entry, &path);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"command\":\"ls\""));
        assert!(content.contains("\"outcome\":\"ok\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rotation_when_projected_size_exceeds_max() {
        let dir = unique_test_dir("rotation-threshold");
        let path = dir.join("test.jsonl");

        let first = sample_entry("ls", "ok");
        let second = sample_entry("run", "ok");

        let first_len = serde_json::to_string(&first).unwrap().len() as u64;
        let max_bytes = first_len + 5;

        log_entry_to_with_rotation(&first, &path, max_bytes, 10);
        log_entry_to_with_rotation(&second, &path, max_bytes, 10);

        let current = fs::read_to_string(&path).unwrap();
        let rotated = fs::read_to_string(rotated_log_path(&path, 1)).unwrap();
        assert!(current.contains("\"command\":\"run\""));
        assert!(rotated.contains("\"command\":\"ls\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rotation_keeps_most_recent_10_files() {
        let dir = unique_test_dir("rotation-retention");
        let path = dir.join("test.jsonl");

        for i in 1..=12 {
            let mut entry = sample_entry(&format!("cmd-{i}"), "ok");
            entry.args = serde_json::json!({});
            log_entry_to_with_rotation(&entry, &path, 1, 10);
        }

        for index in 1..=10 {
            assert!(rotated_log_path(&path, index).exists());
        }
        assert!(!rotated_log_path(&path, 11).exists());

        let current = fs::read_to_string(&path).unwrap();
        let newest_rotated = fs::read_to_string(rotated_log_path(&path, 1)).unwrap();
        let oldest_rotated = fs::read_to_string(rotated_log_path(&path, 10)).unwrap();
        assert!(current.contains("\"command\":\"cmd-12\""));
        assert!(newest_rotated.contains("\"command\":\"cmd-11\""));
        assert!(oldest_rotated.contains("\"command\":\"cmd-2\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_make_entry_ok_result() {
        let result: Result<(), PfpError> = Ok(());
        let entry = make_entry("ls", serde_json::json!({"json": false}), &result, 100);
        assert_eq!(entry.version, env!("CARGO_PKG_VERSION"));
        assert!(!entry.ts.is_empty());
        assert_eq!(entry.command, "ls");
        assert_eq!(entry.outcome, "ok");
        assert!(entry.error.is_none());
        assert_eq!(entry.duration_ms, 100);
    }

    #[test]
    fn test_make_entry_error_result() {
        let result: Result<(), PfpError> =
            Err(PfpError::NoMatch("no deployment matching 'foo'".into()));
        let entry = make_entry("run", serde_json::json!({"query": "foo"}), &result, 55);
        assert_eq!(entry.outcome, "error");
        assert_eq!(
            entry.error.as_deref(),
            Some("No match: no deployment matching 'foo'")
        );
    }

    #[test]
    fn test_configured_max_bytes_default() {
        // With no env var set, should return the default.
        std::env::remove_var(LOG_MAX_BYTES_ENV);
        assert_eq!(configured_max_log_file_bytes(), DEFAULT_MAX_LOG_FILE_BYTES);
    }

    #[test]
    fn test_configured_max_bytes_env_override() {
        std::env::set_var(LOG_MAX_BYTES_ENV, "5000");
        assert_eq!(configured_max_log_file_bytes(), 5000);
        std::env::remove_var(LOG_MAX_BYTES_ENV);
    }

    #[test]
    fn test_configured_max_bytes_zero_uses_default() {
        std::env::set_var(LOG_MAX_BYTES_ENV, "0");
        assert_eq!(configured_max_log_file_bytes(), DEFAULT_MAX_LOG_FILE_BYTES);
        std::env::remove_var(LOG_MAX_BYTES_ENV);
    }

    #[test]
    fn test_configured_max_bytes_non_numeric_uses_default() {
        std::env::set_var(LOG_MAX_BYTES_ENV, "not-a-number");
        assert_eq!(configured_max_log_file_bytes(), DEFAULT_MAX_LOG_FILE_BYTES);
        std::env::remove_var(LOG_MAX_BYTES_ENV);
    }
}
