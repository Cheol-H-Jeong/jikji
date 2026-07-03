use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use jikji_core::{Result, io_error, json_error};
use serde_json::Value;

pub(crate) fn read_jsonl(path: PathBuf) -> Result<Vec<Value>> {
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io_error(path, source)),
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|source| json_error(&path, source)))
        .collect()
}

pub(crate) fn write_json(path: PathBuf, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|source| json_error(&path, source))?;
    fs::write(&path, text).map_err(|source| io_error(path, source))
}

pub(crate) fn write_jsonl(path: PathBuf, rows: &[Value]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        let line = serde_json::to_string(row).map_err(|source| json_error(&path, source))?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(&path, text).map_err(|source| io_error(path, source))
}

pub(crate) fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub(crate) fn dotted_ext(ext: &str) -> String {
    if ext.is_empty() {
        String::new()
    } else {
        format!(".{ext}")
    }
}

pub(crate) fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
