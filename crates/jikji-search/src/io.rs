use std::fs;
use std::path::{Path, PathBuf};

use jikji_core::{Result, io_error, json_error};
use serde::Serialize;
use serde_json::Value;

pub(crate) fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).map_err(|source| io_error(path, source))?;
    serde_json::from_str(&text).map_err(|source| json_error(path, source))
}

pub(crate) fn read_json_optional(path: &Path) -> Value {
    read_json(path).unwrap_or(Value::Object(Default::default()))
}

pub(crate) fn read_jsonl_optional(path: &Path) -> Vec<Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub(crate) fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    let text =
        serde_json::to_string_pretty(value).map_err(|source| json_error(path.clone(), source))?;
    fs::write(&path, text).map_err(|source| io_error(path, source))
}

pub(crate) fn write_jsonl(path: PathBuf, rows: &[Value]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        let line = serde_json::to_string(row).map_err(|source| json_error(path.clone(), source))?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(&path, text).map_err(|source| io_error(path, source))
}

pub(crate) fn sqlite_error(path: &Path, source: rusqlite::Error) -> jikji_core::JikjiError {
    io_error(path, std::io::Error::other(source))
}
