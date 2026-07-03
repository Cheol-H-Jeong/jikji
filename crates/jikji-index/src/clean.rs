use std::fs;
use std::path::{Path, PathBuf};

use jikji_core::{JIKJI_DIR, Result, io_error};
use serde::Serialize;
use serde_json::Value;

use crate::clean_agent_rules::clean_agent_rules;
use crate::clean_targets::clean_targets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanOptions {
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CleanResult {
    pub root: PathBuf,
    pub ok: bool,
    pub reason: String,
    pub dry_run: bool,
    pub removed: Vec<PathBuf>,
    pub would_remove: Vec<PathBuf>,
    pub agent_rules_edited: Vec<PathBuf>,
    pub preserved_original_files: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn clean(root: &Path, options: CleanOptions) -> Result<CleanResult> {
    let clean_root = root
        .canonicalize()
        .map_err(|source| io_error(root, source))?;
    let decision = clean_allowed(&clean_root, options.force)?;
    let targets = clean_targets(&clean_root)?;
    let rules_preview = clean_agent_rules(&clean_root, true)?;
    let mut would_remove = targets.clone();
    would_remove.extend(rules_preview.removed.iter().cloned());
    let has_agent_dir = clean_root.join(JIKJI_DIR).exists();
    let has_agent_rule_changes =
        !rules_preview.edited.is_empty() || !rules_preview.removed.is_empty();
    if !decision.allowed && (has_agent_dir || !would_remove.is_empty() || has_agent_rule_changes) {
        return Ok(CleanResult {
            root: clean_root.clone(),
            ok: false,
            reason: decision.reason,
            dry_run: options.dry_run,
            removed: Vec::new(),
            would_remove,
            agent_rules_edited: rules_preview.edited,
            preserved_original_files: true,
            error: Some(decision.error.unwrap_or_else(|| {
                format!(
                    "Refusing to remove {} without a verified Jikji manifest.",
                    clean_root.join(JIKJI_DIR).display()
                )
            })),
        });
    }

    if options.dry_run {
        return Ok(CleanResult {
            root: clean_root,
            ok: true,
            reason: decision.reason,
            dry_run: true,
            removed: Vec::new(),
            would_remove,
            agent_rules_edited: rules_preview.edited,
            preserved_original_files: true,
            error: None,
        });
    }

    let mut removed = Vec::new();
    for target in targets {
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|source| io_error(&target, source))?;
        } else {
            fs::remove_file(&target).map_err(|source| io_error(&target, source))?;
        }
        removed.push(target);
    }
    let rules_applied = clean_agent_rules(&clean_root, false)?;
    removed.extend(rules_applied.removed.iter().cloned());
    Ok(CleanResult {
        root: clean_root,
        ok: true,
        reason: decision.reason,
        dry_run: false,
        removed,
        would_remove,
        agent_rules_edited: rules_applied.edited,
        preserved_original_files: true,
        error: None,
    })
}

struct CleanDecision {
    allowed: bool,
    reason: String,
    error: Option<String>,
}

fn clean_allowed(root: &Path, force: bool) -> Result<CleanDecision> {
    if force {
        return Ok(clean_decision(true, "force", None));
    }
    let manifest_path = root.join(JIKJI_DIR).join("manifest.json");
    let text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(clean_decision(false, "missing_manifest", None));
        }
        Err(source) => return Err(io_error(manifest_path, source)),
    };
    let manifest: Value = match serde_json::from_str(&text) {
        Ok(manifest) => manifest,
        Err(source) => {
            return Ok(clean_decision(
                false,
                "malformed_manifest",
                Some(format!(
                    "JSON error at {}: {source}",
                    manifest_path.display()
                )),
            ));
        }
    };
    let manifest_root = manifest.get("root").and_then(Value::as_str).unwrap_or("");
    let same_root = PathBuf::from(manifest_root)
        .canonicalize()
        .is_ok_and(|path| path == root);
    let non_destructive = manifest.get("non_destructive").and_then(Value::as_bool) == Some(true);
    if same_root && non_destructive {
        Ok(clean_decision(true, "manifest_verified", None))
    } else {
        Ok(clean_decision(false, "manifest_mismatch", None))
    }
}

fn clean_decision(allowed: bool, reason: &str, error: Option<String>) -> CleanDecision {
    CleanDecision {
        allowed,
        reason: reason.to_owned(),
        error,
    }
}
