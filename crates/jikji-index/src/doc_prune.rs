use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use jikji_core::{Result, io_error};

pub(crate) fn prune_doc_caches(
    doc_text_dir: &Path,
    doc_meta_dir: &Path,
    live_digests: &BTreeSet<String>,
) -> Result<()> {
    remove_stale_matching(doc_text_dir, live_digests, ".txt")?;
    remove_stale_matching(doc_meta_dir, live_digests, ".json")
}

fn remove_stale_matching(dir: &Path, live_digests: &BTreeSet<String>, suffix: &str) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(dir, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| io_error(dir, source))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let digest = digest_from_cache_name(&name, suffix);
        if digest.is_some_and(|digest| !live_digests.contains(digest)) {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|source| io_error(&path, source))?;
            if metadata.is_dir() {
                fs::remove_dir_all(&path).map_err(|source| io_error(&path, source))?;
            } else {
                fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
            }
        }
    }
    Ok(())
}

fn digest_from_cache_name<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let value = name.strip_prefix("sha256_")?;
    if suffix == ".txt" {
        return Some(value.strip_suffix(suffix).unwrap_or(value));
    }
    value.strip_suffix(suffix)
}
