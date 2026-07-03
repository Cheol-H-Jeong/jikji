use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[test]
fn prepare_records_fake_media_bridge_output_when_media_index_is_enabled() {
    let root = temp_root("fake-bridge");
    let script_root = temp_root("fake-bridge-script");
    fs::write(root.join("photo.png"), b"\x89PNG\r\n\x1a\nraw-media-token").expect("write media");
    let script = script_root.join("fake_bridge.py");
    fs::write(
        &script,
        "import json\nprint(json.dumps({'text':'fake-ocr-token-4242','metadata':{'engine':'fake'}}))\n",
    )
    .expect("write script");

    let prepared = json_cmd_with_env(
        &[
            "prepare",
            root_str(&root).as_str(),
            "--enable-media-index",
            "--json",
        ],
        &[
            ("JIKJI_MEDIA_BRIDGE_PYTHON", python3().as_str()),
            ("JIKJI_MEDIA_BRIDGE_SCRIPT", root_str(&script).as_str()),
        ],
    );

    assert_eq!(prepared["files"], 1);
    let rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let row = row_for(&rows, "photo.png");
    assert_eq!(row["media_bridge_status"], "success");
    let cache = fs::read_to_string(root.join(row["text_cache_path"].as_str().expect("cache")))
        .expect("read cache");
    assert!(cache.contains("fake-ocr-token-4242"));
    assert!(!cache.contains("raw-media-token"));
    let meta = json_file(root.join(row["doc_meta_path"].as_str().expect("meta")));
    assert_eq!(meta["media_bridge"]["status"], "success");
    assert_eq!(meta["parser"], "media-metadata");
}

#[test]
fn prepare_surfaces_unavailable_media_bridge_when_python_is_missing() {
    let root = temp_root("missing-bridge");
    fs::write(root.join("clip.mp4"), b"video-raw-token").expect("write media");

    let prepared = json_cmd_with_env(
        &[
            "prepare",
            root_str(&root).as_str(),
            "--enable-media-index",
            "--json",
        ],
        &[
            ("JIKJI_MEDIA_BRIDGE_PYTHON", "/missing/jikji-python"),
            ("JIKJI_MEDIA_BRIDGE_SCRIPT", "bridge.py"),
        ],
    );

    assert_eq!(prepared["files"], 1);
    let rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let row = row_for(&rows, "clip.mp4");
    assert_eq!(row["parse_status"], "metadata_only");
    assert_eq!(row["media_bridge_status"], "unavailable");
    let cache = fs::read_to_string(root.join(row["text_cache_path"].as_str().expect("cache")))
        .expect("read cache");
    assert!(!cache.contains("video-raw-token"));
    let meta = json_file(root.join(row["doc_meta_path"].as_str().expect("meta")));
    assert_eq!(meta["media_bridge"]["status"], "unavailable");
    assert!(
        meta["media_bridge"]["error"]
            .as_str()
            .expect("bridge error")
            .contains("/missing/jikji-python")
    );
}

fn run_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jikji"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run jikji")
}

fn json_cmd_with_env(args: &[&str], envs: &[(&str, &str)]) -> Value {
    let output = run_with_env(args, envs);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn json_file(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
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

fn python3() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_owned())
}

fn root_str(root: &Path) -> String {
    root.display().to_string()
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
