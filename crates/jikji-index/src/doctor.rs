use std::fs;
use std::path::{Path, PathBuf};

use jikji_core::{JIKJI_DIR, LEGACY_ROOT_AGENT_MAP, ROOT_AGENT_MAP, Result, io_error, json_error};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoctorReport {
    pub root: PathBuf,
    pub ok: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub manifest: Value,
    pub image_support: Value,
    pub media_support: Value,
}

const REQUIRED_FILES: &[&str] = &[
    ".jikji/manifest.json",
    ".jikji/file_index.jsonl",
    ".jikji/folder_index.jsonl",
    ".jikji/document_index.jsonl",
    ".jikji/file_cards.jsonl",
    ".jikji/chunk_map.jsonl",
    ".jikji/search_index.sqlite",
    ".jikji/duplicate_map.jsonl",
    ".jikji/folder_profile.jsonl",
    ".jikji/corpus_profile.json",
    ".jikji/intent_taxonomy.json",
    ".jikji/autorag_manifest.json",
    ".jikji/knowledge_graph.json",
    ".jikji/graph_routes.jsonl",
    ".jikji/llm_wiki_schema.md",
    ".jikji/wiki/index.md",
    ".jikji/parse_errors.jsonl",
    ".jikji/agent_map.md",
];

pub fn doctor(root: &Path) -> Result<DoctorReport> {
    let clean_root = root
        .canonicalize()
        .map_err(|source| io_error(root, source))?;
    let mut errors = Vec::new();
    for rel in REQUIRED_FILES {
        let path = clean_root.join(rel);
        if !path.is_file() {
            errors.push(format!("missing required artifact: {rel}"));
        }
    }
    if !clean_root.join(ROOT_AGENT_MAP).is_file()
        && !clean_root.join(LEGACY_ROOT_AGENT_MAP).is_file()
    {
        errors.push(format!("missing required artifact: {ROOT_AGENT_MAP}"));
    }

    let manifest_path = clean_root.join(JIKJI_DIR).join("manifest.json");
    let manifest = read_manifest(&manifest_path, &mut errors)?;
    validate_jsonl_files(&clean_root, &mut errors)?;
    let media_manifest = manifest
        .get("media_index")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let image_support = serde_json::json!({
        "metadata_indexing": true,
        "ocr_active": false,
        "ocr_available": false,
        "indexed_image_documents": image_doc_count(&clean_root)?,
    });
    let media_support = serde_json::json!({
        "enabled": media_manifest.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        "status": media_manifest.get("status").and_then(Value::as_str).unwrap_or("unknown"),
        "max_mb": media_manifest.get("max_mb").cloned().unwrap_or(Value::Null),
        "media_files": media_manifest.get("media_files").and_then(Value::as_u64).unwrap_or(0),
        "image_ocr_available": false,
        "audio_video_transcription_available": false,
        "opt_in_flag": "--enable-media-index",
    });

    Ok(DoctorReport {
        root: clean_root,
        ok: errors.is_empty(),
        warnings: Vec::new(),
        errors,
        manifest: serde_json::json!({
            "schema_version": manifest.get("schema_version").cloned().unwrap_or(Value::Null),
            "search_index_schema_version": manifest.get("search_index_schema_version").cloned().unwrap_or(Value::Null),
            "non_destructive": manifest.get("non_destructive").cloned().unwrap_or(Value::Null),
            "media_index": media_manifest,
        }),
        image_support,
        media_support,
    })
}

pub fn read_map(root: &Path) -> Result<String> {
    let clean_root = root
        .canonicalize()
        .map_err(|source| io_error(root, source))?;
    for rel in [ROOT_AGENT_MAP, LEGACY_ROOT_AGENT_MAP, ".jikji/agent_map.md"] {
        let path = clean_root.join(rel);
        if path.is_file() {
            return fs::read_to_string(&path).map_err(|source| io_error(path, source));
        }
    }
    Ok(format!(
        "No Jikji map found under {}. Run: jikji prepare {}",
        clean_root.display(),
        clean_root.display()
    ))
}

fn read_manifest(path: &Path, errors: &mut Vec<String>) -> Result<Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({}));
        }
        Err(source) => return Err(io_error(path, source)),
    };
    let manifest: Value = match serde_json::from_str(&text) {
        Ok(manifest) => manifest,
        Err(source) => {
            errors.push(format!("malformed manifest: {source}"));
            return Ok(serde_json::json!({}));
        }
    };
    if manifest.get("schema_version").and_then(Value::as_u64) != Some(1) {
        errors.push("unsupported schema_version".to_owned());
    }
    if manifest.get("non_destructive").and_then(Value::as_bool) != Some(true) {
        errors.push("manifest non_destructive must be true".to_owned());
    }
    Ok(manifest)
}

fn validate_jsonl_files(root: &Path, errors: &mut Vec<String>) -> Result<()> {
    for rel in [
        ".jikji/file_index.jsonl",
        ".jikji/folder_index.jsonl",
        ".jikji/document_index.jsonl",
        ".jikji/file_cards.jsonl",
        ".jikji/chunk_map.jsonl",
        ".jikji/duplicate_map.jsonl",
        ".jikji/folder_profile.jsonl",
        ".jikji/parse_errors.jsonl",
    ] {
        let path = root.join(rel);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        for (idx, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            if serde_json::from_str::<Value>(line).is_err() {
                errors.push(format!("{rel}:{} invalid JSON", idx + 1));
            }
        }
    }
    Ok(())
}

fn image_doc_count(root: &Path) -> Result<usize> {
    let path = root.join(JIKJI_DIR).join("document_index.jsonl");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Ok(0),
    };
    let mut count = 0usize;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).map_err(|source| json_error(&path, source))?;
        let ext = row.get("ext").and_then(Value::as_str).unwrap_or("");
        if matches!(
            ext,
            ".png" | ".jpg" | ".jpeg" | ".tif" | ".tiff" | ".webp" | ".bmp" | ".gif"
        ) {
            count += 1;
        }
    }
    Ok(count)
}
