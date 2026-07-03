use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub(super) fn write_sqlite_fixture(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("open sqlite");
    connection
        .execute(
            "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT, body TEXT)",
            [],
        )
        .expect("create table");
    connection
        .execute(
            "INSERT INTO notes (title, body) VALUES (?1, ?2)",
            ("Research", "sqlitebodytoken-3301 inside row"),
        )
        .expect("insert row");
}

pub(super) fn write_epub_fixture(path: &Path) {
    let file = fs::File::create(path).expect("create epub");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("mimetype", options).expect("mimetype");
    std::io::Write::write_all(&mut zip, b"application/epub+zip").expect("write mimetype");
    zip.start_file("OEBPS/chapter1.xhtml", options)
        .expect("chapter");
    std::io::Write::write_all(
        &mut zip,
        b"<html><body><h1>Chapter</h1><p>epubtoken-8802 appears here.</p></body></html>",
    )
    .expect("write chapter");
    zip.finish().expect("finish epub");
}

pub(super) fn write_zip_fixture(path: &Path) {
    let file = fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        "nested/archive_lookup_marker_9123.txt",
        zip::write::SimpleFileOptions::default(),
    )
    .expect("start member");
    std::io::Write::write_all(&mut zip, b"body not extracted").expect("write member");
    zip.finish().expect("finish zip");
}

pub(super) fn write_png(path: &Path, width: u32, height: u32) {
    let mut bytes = Vec::from(&b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"[..]);
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend(b"\x08\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00IEND\xaeB`\x82");
    fs::write(path, bytes).expect("write png");
}

pub(super) fn document_row(root: &Path, target: &str) -> Value {
    jsonl(root.join(".jikji/document_index.jsonl"))
        .into_iter()
        .find(|row| row["path"] == target)
        .unwrap_or_else(|| panic!("missing document row {target}"))
}

pub(super) fn first_candidate_path(report: &Value) -> &str {
    report["candidates"][0]["path"]
        .as_str()
        .expect("candidate path")
}

pub(super) fn json_cmd(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(args)
        .output()
        .expect("run jikji");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

pub(super) fn jsonl(path: PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("json row"))
        .collect()
}

pub(super) fn json_file(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file")).expect("json file")
}

pub(super) fn assert_has_keys(row: &Value, keys: &[&str]) {
    for key in keys {
        assert!(row.get(*key).is_some(), "missing key {key} in {row}");
    }
}

pub(super) struct TempRoot {
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

pub(super) fn temp_root(label: &str) -> TempRoot {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("jikji-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("create temp root");
    TempRoot { path: root }
}

pub(super) fn root_arg(root: &Path) -> String {
    root.display().to_string()
}
