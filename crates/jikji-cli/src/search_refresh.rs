use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use jikji_core::PrepareOptions;

pub(crate) fn start_background_refresh(root: &Path, options: &PrepareOptions) -> bool {
    let index_dir = root.join(".jikji");
    if !safe_existing_index_dir(&index_dir) {
        return false;
    }
    if index_dir.join(".lock").exists() {
        return false;
    }
    spawn_background_prepare(root, options).is_ok()
}

fn safe_existing_index_dir(index_dir: &Path) -> bool {
    match fs::symlink_metadata(index_dir) {
        Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => false,
    }
}

fn spawn_background_prepare(
    root: &Path,
    options: &PrepareOptions,
) -> std::io::Result<std::process::Child> {
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg("prepare")
        .arg(root)
        .arg("--parse-timeout")
        .arg(options.parse_timeout_seconds.to_string())
        .arg("--max-hash-bytes")
        .arg(options.max_hash_bytes.to_string())
        .arg("--doc-text-max-chars")
        .arg(options.doc_text_max_chars.to_string())
        .arg("--doc-text-chunk-chars")
        .arg(options.doc_text_chunk_chars.to_string())
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    append_optional_prepare_args(&mut command, options);
    command.spawn()
}

fn append_optional_prepare_args(command: &mut Command, options: &PrepareOptions) {
    if options.include_hidden {
        command.arg("--include-hidden");
    }
    if options.include_sensitive {
        command.arg("--include-sensitive");
    }
    if let Some(max_files) = options.max_files {
        command.arg("--max-files").arg(max_files.to_string());
    }
    for pattern in &options.exclude_patterns {
        command.arg("--exclude").arg(pattern);
    }
    if options.enable_media_index {
        command.arg("--enable-media-index");
        command
            .arg("--media-index-max-mb")
            .arg(options.media_index_max_mb.to_string());
    }
}
