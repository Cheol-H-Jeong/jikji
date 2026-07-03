use std::fs;
use std::path::Path;

use jikji_core::{Result, io_error};

pub(crate) fn text_cache_path_for(digest: &str, body_text: &str, chunk_chars: usize) -> String {
    if body_text.chars().count() > chunk_chars.max(1) {
        format!(".jikji/doc_text/sha256_{digest}")
    } else {
        format!(".jikji/doc_text/sha256_{digest}.txt")
    }
}

pub(crate) fn cache_text_with_header(rel: &str, digest: &str, body_text: &str) -> String {
    format!("# Source: {rel}\n# File ID: sha256:{digest}\n# Parsed by: Jikji\n\n{body_text}")
}

pub(crate) fn write_doc_text_cache(
    doc_text_dir: &Path,
    rel: &str,
    digest: &str,
    body_text: &str,
    chunk_chars: usize,
) -> Result<()> {
    let chunk_chars = chunk_chars.max(1);
    if body_text.chars().count() > chunk_chars {
        let file_path = doc_text_dir.join(format!("sha256_{digest}.txt"));
        remove_path_if_exists(&file_path)?;
        let chunk_dir = doc_text_dir.join(format!("sha256_{digest}"));
        remove_symlink_or_file_at_dir_path(&chunk_dir)?;
        fs::create_dir_all(&chunk_dir).map_err(|source| io_error(&chunk_dir, source))?;
        for entry in fs::read_dir(&chunk_dir).map_err(|source| io_error(&chunk_dir, source))? {
            let entry = entry.map_err(|source| io_error(&chunk_dir, source))?;
            if entry.file_name().to_string_lossy().starts_with("chunk_") {
                remove_path_if_exists(&entry.path())?;
            }
        }
        let body_chars = body_text.chars().collect::<Vec<_>>();
        for (idx, chunk) in body_chars.chunks(chunk_chars).enumerate() {
            let chunk_text = chunk.iter().collect::<String>();
            write_generated_cache_file(
                &chunk_dir.join(format!("chunk_{:04}.txt", idx + 1)),
                cache_text_with_header(rel, digest, &chunk_text).as_bytes(),
            )?;
        }
        return Ok(());
    }

    let chunk_dir = doc_text_dir.join(format!("sha256_{digest}"));
    remove_path_if_exists(&chunk_dir)?;
    write_generated_cache_file(
        &doc_text_dir.join(format!("sha256_{digest}.txt")),
        cache_text_with_header(rel, digest, body_text).as_bytes(),
    )
}

pub(crate) fn write_generated_cache_file(path: &Path, contents: &[u8]) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|source| io_error(path, source))?;
        }
        Ok(_) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(io_error(path, source)),
    }
    fs::write(path, contents).map_err(|source| io_error(path, source))
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(path, source)),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|source| io_error(path, source))
    } else {
        fs::remove_file(path).map_err(|source| io_error(path, source))
    }
}

fn remove_symlink_or_file_at_dir_path(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(path, source)),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|source| io_error(path, source))?;
    }
    Ok(())
}
