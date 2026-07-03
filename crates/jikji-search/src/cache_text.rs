use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn read_cache_text(root: &Path, cache_path: &str, limit: usize) -> String {
    if cache_path.is_empty() {
        return String::new();
    }
    let path = root.join(cache_path);
    if path.is_file() {
        return read_source_text(path, limit);
    }
    if path.is_dir() {
        return read_cache_dir_text(&path, limit);
    }
    String::new()
}

pub(crate) fn read_source_text(path: PathBuf, limit: usize) -> String {
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(_) => return String::new(),
    };
    String::from_utf8_lossy(&raw)
        .chars()
        .take(limit)
        .collect::<String>()
}

fn read_cache_dir_text(path: &Path, limit: usize) -> String {
    let mut entries = match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return String::new(),
    };
    entries.sort_by_key(|entry| entry.file_name());
    let mut text = String::new();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("chunk_") || !entry.path().is_file() {
            continue;
        }
        text.push_str(&read_source_text(
            entry.path(),
            limit.saturating_sub(text.chars().count()),
        ));
        if text.chars().count() >= limit {
            break;
        }
        text.push('\n');
    }
    text.chars().take(limit).collect()
}
