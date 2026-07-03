use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::ops::Deref;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use jikji_core::PrepareOptions;
use jikji_index::{CleanOptions, clean, prepare, scan_root};
use serde_json::Value;

#[test]
fn prepare_scanner_and_clean_core_behaviors_when_called_as_library() {
    let root = temp_root("index-core");
    fs::write(root.join("keep.txt"), "keep").expect("write source file");
    fs::write(root.join(".env"), "secret").expect("write sensitive file");
    fs::create_dir(root.join(".jikji")).expect("create sidecar");
    fs::write(root.join(".jikji/user-created-note.txt"), "user").expect("write user file");

    let result = prepare(&root, &PrepareOptions::default()).expect("prepare");

    assert_eq!(result.files, 1);
    assert!(root.join("keep.txt").exists());
    assert!(root.join(".jikji/user-created-note.txt").exists());
    let scan = scan_root(
        &root,
        &PrepareOptions {
            include_hidden: true,
            ..PrepareOptions::default()
        },
    )
    .expect("scan");
    assert!(
        scan.files
            .iter()
            .all(|path| !path.starts_with(root.join(".jikji")))
    );

    let dry = clean(
        &root,
        CleanOptions {
            dry_run: true,
            force: false,
        },
    )
    .expect("dry clean");
    assert!(
        dry.would_remove
            .iter()
            .any(|path| path.ends_with("manifest.json"))
    );
    assert!(
        !dry.would_remove
            .iter()
            .any(|path| path.ends_with("user-created-note.txt"))
    );
}

#[test]
fn source_tree_signature_uses_path_size_and_mtime_when_scanning() {
    let root = temp_root("signature");
    fs::write(root.join("a.txt"), "one").expect("write source file");
    let first = scan_root(&root, &PrepareOptions::default()).expect("first scan");
    fs::write(root.join("a.txt"), "two-two").expect("rewrite source file");
    let second = scan_root(&root, &PrepareOptions::default()).expect("second scan");

    assert_eq!(
        first.signature.algorithm,
        "sha256(relpath,size,mtime_ns).v1"
    );
    assert_ne!(first.signature.digest, second.signature.digest);
    assert_eq!(second.signature.total_size, 7);
}

#[test]
fn clean_preserves_forged_cache_shaped_files_when_document_index_is_forged() {
    let root = temp_root("index-forged-cache");
    fs::write(root.join("source.txt"), "source").expect("write source");
    prepare(&root, &PrepareOptions::default()).expect("prepare");
    let digest = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let text_cache = root.join(format!(".jikji/doc_text/sha256_{digest}.txt"));
    let doc_meta = root.join(format!(".jikji/doc_meta/sha256_{digest}.json"));
    fs::write(&text_cache, "user-authored cache-shaped text").expect("write fake cache");
    fs::write(&doc_meta, r#"{"user":true}"#).expect("write fake meta");
    fs::write(
        root.join(".jikji/document_index.jsonl"),
        format!(
            "{{\"text_cache_path\":\".jikji/doc_text/sha256_{digest}.txt\",\"doc_meta_path\":\".jikji/doc_meta/sha256_{digest}.json\"}}\n"
        ),
    )
    .expect("forge document index");

    let result = clean(
        &root,
        CleanOptions {
            dry_run: false,
            force: false,
        },
    )
    .expect("clean");

    assert!(result.ok);
    assert!(text_cache.exists());
    assert!(doc_meta.exists());
}

#[cfg(unix)]
#[test]
fn clean_preserves_in_root_files_when_document_index_is_symlinked_outside_root() {
    let root = temp_root("index-symlinked-index");
    let outside = temp_root("index-outside-index");
    fs::write(root.join("source.txt"), "source").expect("write source");
    prepare(&root, &PrepareOptions::default()).expect("prepare");
    let digest = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let text_cache = root.join(format!(".jikji/doc_text/sha256_{digest}.txt"));
    fs::write(&text_cache, "user cache-shaped file").expect("write fake cache");
    fs::write(
        outside.join("document_index.jsonl"),
        format!("{{\"text_cache_path\":\".jikji/doc_text/sha256_{digest}.txt\"}}\n"),
    )
    .expect("write outside index");
    fs::remove_file(root.join(".jikji/document_index.jsonl")).expect("remove generated index");
    unix_fs::symlink(
        outside.join("document_index.jsonl"),
        root.join(".jikji/document_index.jsonl"),
    )
    .expect("symlink index");

    let result = clean(
        &root,
        CleanOptions {
            dry_run: false,
            force: false,
        },
    )
    .expect("clean");

    assert!(result.ok);
    assert!(text_cache.exists());
}

#[test]
fn prepare_uses_parser_registry_outputs_when_building_doc_cache() {
    let root = temp_root("parser-doc-cache");
    fs::write(root.join("note.rtf"), br"{\rtf1 visible-rtf-token}").expect("write rtf");
    fs::write(
        root.join("photo.png"),
        b"\x89PNG\r\n\x1a\nraw-media-body-token",
    )
    .expect("write media");
    write_zip(
        &root.join("bundle.zip"),
        "nested/archive-member-token.txt",
        b"raw-archive-body-token",
    );

    prepare(&root, &PrepareOptions::default()).expect("prepare");

    let rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let rtf = row_for(&rows, "note.rtf");
    let rtf_cache = fs::read_to_string(root.join(rtf["text_cache_path"].as_str().expect("cache")))
        .expect("read rtf cache");
    assert!(rtf_cache.contains("visible-rtf-token"));
    assert!(!rtf_cache.contains(r"{\rtf1"));
    assert_eq!(rtf["parse_status"], "success");

    let media = row_for(&rows, "photo.png");
    let media_cache =
        fs::read_to_string(root.join(media["text_cache_path"].as_str().expect("cache")))
            .expect("read media cache");
    assert!(!media_cache.contains("raw-media-body-token"));
    assert_eq!(media["parse_status"], "metadata_only");

    let archive = row_for(&rows, "bundle.zip");
    let archive_cache =
        fs::read_to_string(root.join(archive["text_cache_path"].as_str().expect("cache")))
            .expect("read archive cache");
    assert!(archive_cache.contains("nested/archive-member-token.txt"));
    assert!(!archive_cache.contains("raw-archive-body-token"));
    assert_eq!(archive["parse_status"], "archive_listing");
}

struct TempRoot {
    path: PathBuf,
}

impl Deref for TempRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<Path> for TempRoot {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for TempRoot {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn temp_root(label: &str) -> TempRoot {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("jikji-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("create temp root");
    TempRoot { path: root }
}

fn jsonl(path: PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl row"))
        .collect()
}

fn row_for<'a>(rows: &'a [Value], path: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["path"] == path)
        .unwrap_or_else(|| panic!("missing row for {path}"))
}

fn write_zip(path: &std::path::Path, member: &str, body: &[u8]) {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::<u8>::new()));
    zip.start_file(member, zip::write::SimpleFileOptions::default())
        .expect("start zip member");
    zip.write_all(body).expect("write zip body");
    let bytes = zip.finish().expect("finish zip").into_inner();
    fs::write(path, bytes).expect("write zip file");
}
