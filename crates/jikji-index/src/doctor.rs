use std::path::{Path, PathBuf};

use jikji_core::storage::{load_artifact, load_artifacts};
use jikji_core::{LEGACY_ROOT_AGENT_MAP, ROOT_AGENT_MAP, Result, io_error};
use serde::Serialize;
use serde_json::{Value, json};

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

const REQUIRED_KINDS: &[&str] = &[
    "manifest",
    "files",
    "folders",
    "cards",
    "graph",
    "wiki_index",
];

pub fn doctor(root: &Path) -> Result<DoctorReport> {
    let clean_root = root
        .canonicalize()
        .map_err(|source| io_error(root, source))?;
    let mut errors = Vec::new();
    for kind in REQUIRED_KINDS {
        if load_artifacts(&clean_root, kind)?.is_empty() {
            errors.push(format!("missing required artifact row: {kind}"));
        }
    }
    let manifest = load_artifact(&clean_root, "manifest")?.unwrap_or(Value::Null);
    if manifest.get("root").and_then(Value::as_str) != Some(clean_root.to_string_lossy().as_ref()) {
        errors.push("manifest root does not match selected root".to_owned());
    }
    let documents = load_artifacts(&clean_root, "documents")?;
    let image_docs = documents
        .iter()
        .filter(|row| {
            row.get("ext").and_then(Value::as_str).is_some_and(|ext| {
                matches!(
                    ext,
                    ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".tif" | ".tiff" | ".bmp"
                )
            })
        })
        .count();
    let media = manifest
        .get("media_index")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(DoctorReport {
        root: clean_root,
        ok: errors.is_empty(),
        warnings: Vec::new(),
        errors,
        manifest,
        image_support: json!({"indexed_image_documents": image_docs,"metadata_indexing":true,"ocr_active":media.get("enabled").and_then(Value::as_bool).unwrap_or(false),"ocr_available":false}),
        media_support: json!({"enabled":media.get("enabled").and_then(Value::as_bool).unwrap_or(false),"status":media.get("status").cloned().unwrap_or(Value::String("unknown".to_owned())),"media_files":media.get("media_files").cloned().unwrap_or(Value::from(0)),"max_mb":media.get("max_mb").cloned().unwrap_or(Value::Null),"image_ocr_available":false,"audio_video_transcription_available":false,"opt_in_flag":"--enable-media-index"}),
    })
}

pub fn read_map(root: &Path) -> Result<String> {
    if let Some(markdown) = load_artifact(root, "agent_map")?.and_then(|value| {
        value
            .get("markdown")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }) {
        return Ok(markdown);
    }
    for path in [root.join(ROOT_AGENT_MAP), root.join(LEGACY_ROOT_AGENT_MAP)] {
        match std::fs::read_to_string(&path) {
            Ok(text) => return Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error(path, source)),
        }
    }
    Err(io_error(
        root,
        std::io::Error::new(std::io::ErrorKind::NotFound, "Jikji map is unavailable"),
    ))
}
