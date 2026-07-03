use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use jikji_core::{JIKJI_DIR, OWNED_GENERATED_PATHS, RETIRED_GENERATED_PATHS, Result, io_error};

use crate::clean_cache::recorded_cache_paths;

pub(crate) fn clean_targets(root: &Path) -> Result<Vec<PathBuf>> {
    let mut targets = BTreeSet::new();
    for raw_path in OWNED_GENERATED_PATHS.iter().chain(RETIRED_GENERATED_PATHS) {
        add_owned_target(root, raw_path, &mut targets)?;
    }
    add_matching_generated_files(root, ".jikji/wiki/sources", "md", &mut targets)?;
    for raw_path in recorded_cache_paths(root)? {
        add_relative_file(root, &raw_path, &mut targets)?;
    }
    add_empty_dir(root, ".jikji/doc_text", &mut targets)?;
    add_empty_dir(root, ".jikji/doc_meta", &mut targets)?;
    add_dir_if_entries_targeted(root, ".jikji/wiki/sources", &mut targets)?;
    add_dir_if_entries_targeted(root, ".jikji/doc_text", &mut targets)?;
    add_dir_if_entries_targeted(root, ".jikji/doc_meta", &mut targets)?;
    add_dir_if_entries_targeted(root, ".jikji/wiki", &mut targets)?;
    add_dir_if_entries_targeted(root, JIKJI_DIR, &mut targets)?;
    Ok(removal_order(targets))
}

pub(crate) fn safe_rel_path(raw_path: &str) -> Option<&str> {
    let rel = raw_path.strip_suffix('/').unwrap_or(raw_path);
    let path = Path::new(rel);
    if path.has_root()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        None
    } else {
        Some(rel)
    }
}

pub(crate) fn path_under_root(path: &Path, root: &Path) -> bool {
    path.canonicalize()
        .is_ok_and(|resolved| resolved == root || resolved.starts_with(root))
}

fn add_owned_target(root: &Path, raw_path: &str, targets: &mut BTreeSet<PathBuf>) -> Result<()> {
    let rel = match safe_rel_path(raw_path) {
        Some(rel) => rel,
        None => return Ok(()),
    };
    match raw_path {
        ".jikji/wiki/" => add_relative_file(root, ".jikji/wiki/index.md", targets),
        ".jikji/doc_text/" | ".jikji/doc_meta/" | ".jikji/eval/" => Ok(()),
        path if path.ends_with('/') => add_empty_dir(root, rel, targets),
        _ => add_relative_file(root, rel, targets),
    }
}

fn add_relative_file(root: &Path, rel: &str, targets: &mut BTreeSet<PathBuf>) -> Result<()> {
    let path = root.join(rel);
    if !path.exists() || !path_under_root(&path, root) {
        return Ok(());
    }
    if path.is_file() || path.is_dir() || path.is_symlink() {
        targets.insert(path);
    }
    Ok(())
}

fn add_empty_dir(root: &Path, rel: &str, targets: &mut BTreeSet<PathBuf>) -> Result<()> {
    let path = root.join(rel);
    if path.exists()
        && path_under_root(&path, root)
        && path.is_dir()
        && fs::read_dir(&path)
            .map_err(|source| io_error(&path, source))?
            .next()
            .is_none()
    {
        targets.insert(path);
    }
    Ok(())
}

fn add_matching_generated_files(
    root: &Path,
    rel: &str,
    extension: &str,
    targets: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let dir = root.join(rel);
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&dir, source)),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Ok(());
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&dir, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| io_error(&dir, source))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some(extension)
            && has_generated_wiki_source_name(&path)
        {
            add_relative_file(root, &rel_path(root, &path), targets)?;
        }
    }
    Ok(())
}

fn add_dir_if_entries_targeted(
    root: &Path,
    rel: &str,
    targets: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let dir = root.join(rel);
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&dir, source)),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Ok(());
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&dir, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| io_error(&dir, source))?;
        if !targets.contains(&entry.path()) {
            return Ok(());
        }
    }
    targets.insert(dir);
    Ok(())
}

fn removal_order(targets: BTreeSet<PathBuf>) -> Vec<PathBuf> {
    let mut ordered = targets.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|path| {
        (
            path.is_dir(),
            std::cmp::Reverse(path.components().count()),
            path.clone(),
        )
    });
    ordered
}

fn has_generated_wiki_source_name(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((_, suffix)) = stem.rsplit_once('-') else {
        return false;
    };
    suffix.len() == 12 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::safe_rel_path;
    use jikji_core::ROOT_AGENT_MAP;

    #[test]
    fn safe_rel_path_rejects_forged_paths_when_cleaning() {
        assert_eq!(safe_rel_path("../outside"), None);
        assert_eq!(safe_rel_path("/tmp/outside"), None);
        assert_eq!(safe_rel_path(ROOT_AGENT_MAP), Some(ROOT_AGENT_MAP));
    }

    #[cfg(windows)]
    #[test]
    fn safe_rel_path_rejects_windows_rooted_paths_when_cleaning() {
        assert_eq!(safe_rel_path(r"\tmp\outside"), None);
        assert_eq!(safe_rel_path(r"C:\tmp\outside"), None);
    }
}
