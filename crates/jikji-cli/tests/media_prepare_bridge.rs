use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[test]
fn prepare_records_rust_native_media_metadata() {
    let root = temp_root("native-media");
    let mut png = vec![0_u8; 24];
    png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    png[12..16].copy_from_slice(b"IHDR");
    png[16..20].copy_from_slice(&320_u32.to_be_bytes());
    png[20..24].copy_from_slice(&200_u32.to_be_bytes());
    fs::write(root.join("photo.png"), png).expect("write media");

    let prepared = json_cmd(&[
        "prepare",
        root_str(&root).as_str(),
        "--enable-media-index",
        "--json",
    ]);
    assert_eq!(prepared["files"], 1);
    let rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let row = row_for(&rows, "photo.png");
    assert_eq!(row["parse_status"], "metadata_only");
    assert_eq!(row["media_bridge_status"], "metadata_only");
    let meta = json_file(root.join(row["doc_meta_path"].as_str().expect("meta")));
    assert_eq!(meta["media_bridge"]["metadata"]["engine"], "rust-native");
    assert_eq!(meta["media_bridge"]["metadata"]["width"], "320");
}

fn json_cmd(args: &[&str]) -> Value {
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

fn jsonl(path: impl AsRef<Path>) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("jsonl row"))
        .collect()
}

fn json_file(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("json file")
}

fn row_for<'a>(rows: &'a [Value], path: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["path"] == path)
        .expect("document row")
}

fn root_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jikji-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).expect("temp root");
    path
}
