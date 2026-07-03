use std::fs;
use std::path::Path;

use jikji_core::{JIKJI_DIR, Result, io_error, json_error};
use serde_json::Value;

use crate::clean_targets::{path_under_root, safe_rel_path};

pub(crate) fn recorded_cache_paths(root: &Path) -> Result<Vec<String>> {
    let path = root.join(JIKJI_DIR).join("document_index.jsonl");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io_error(&path, source)),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Ok(Vec::new());
    }
    if !path_under_root(&path, root) {
        return Ok(Vec::new());
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io_error(path, source)),
    };
    let mut paths = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).map_err(|source| json_error(&path, source))?;
        paths.extend(verified_row_cache_paths(root, &row)?);
    }
    Ok(paths)
}

fn verified_row_cache_paths(root: &Path, row: &Value) -> Result<Vec<String>> {
    let Some(row_digest) = row.get("sha256").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    if !is_sha256_digest(row_digest) {
        return Ok(Vec::new());
    }
    let Some(meta_rel) = row.get("doc_meta_path").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    if digest_from_cache_path(meta_rel, ".jikji/doc_meta/", ".json") != Some(row_digest) {
        return Ok(Vec::new());
    }
    if !has_jikji_doc_meta_marker(root, meta_rel, row_digest)? {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    if let Some(text_rel) = row.get("text_cache_path").and_then(Value::as_str) {
        if digest_from_doc_text_path(text_rel) == Some(row_digest) {
            paths.push(text_rel.to_owned());
        }
    }
    paths.push(meta_rel.to_owned());
    Ok(paths)
}

fn has_jikji_doc_meta_marker(root: &Path, rel: &str, digest: &str) -> Result<bool> {
    let Some(rel) = safe_rel_path(rel) else {
        return Ok(false);
    };
    let path = root.join(rel);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(io_error(&path, source)),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Ok(false);
    }
    if !path_under_root(&path, root) {
        return Ok(false);
    }
    let text = fs::read_to_string(&path).map_err(|source| io_error(&path, source))?;
    let Ok(meta) = serde_json::from_str::<Value>(&text) else {
        return Ok(false);
    };
    Ok(meta.get("source").and_then(Value::as_str) == Some("jikji")
        && meta.get("file_id").and_then(Value::as_str) == Some(format!("sha256:{digest}").as_str()))
}

fn digest_from_cache_path<'a>(rel: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    safe_rel_path(rel)?;
    let digest = rel.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let digest = digest.strip_prefix("sha256_")?;
    if is_sha256_digest(digest) {
        Some(digest)
    } else {
        None
    }
}

fn digest_from_doc_text_path(rel: &str) -> Option<&str> {
    safe_rel_path(rel)?;
    let digest = rel.strip_prefix(".jikji/doc_text/")?;
    let digest = digest.strip_prefix("sha256_")?;
    let digest = digest.strip_suffix(".txt").unwrap_or(digest);
    if is_sha256_digest(digest) {
        Some(digest)
    } else {
        None
    }
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
