use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[path = "prepare_clean_doctor_parity/clean_security.rs"]
mod clean_security;
#[path = "prepare_clean_doctor_parity/prepare_security.rs"]
mod prepare_security;
#[path = "prepare_clean_doctor_parity/python_options.rs"]
mod python_options;
#[path = "prepare_clean_doctor_parity/safety_misc.rs"]
mod safety_misc;

#[test]
fn prepare_clean_doctor_and_map_match_task_three_contract() {
    let root = temp_root("cli-parity");
    fs::create_dir_all(root.join("docs")).expect("create docs");
    fs::write(root.join("docs/meeting.txt"), "Jikji smoke").expect("write text");
    fs::write(root.join("report.rtf"), r"{\rtf1 stale body}").expect("write rtf");
    fs::write(root.join(".env"), "secret").expect("write sensitive file");
    fs::create_dir_all(root.join(".hidden")).expect("create hidden dir");
    fs::write(root.join(".hidden/hidden.txt"), "hidden").expect("write hidden");

    let prepared = json_cmd(["prepare", root_str(&root).as_str(), "--json"]);
    assert_eq!(prepared["files"], 2);
    assert!(root.join("docs/meeting.txt").exists());
    assert!(root.join(".jikji/agent_map.md").exists());
    assert!(root.join(".jikji_agent_map.md").exists());
    let file_rows = jsonl(root.join(".jikji/file_index.jsonl"));
    assert!(
        file_rows
            .iter()
            .any(|row| row["path"] == "docs/meeting.txt")
    );
    assert!(file_rows.iter().all(|row| row["path"] != ".env"));
    assert!(
        file_rows
            .iter()
            .all(|row| row["path"] != ".hidden/hidden.txt")
    );

    let manifest = json_file(root.join(".jikji/manifest.json"));
    assert_eq!(manifest["non_destructive"], true);
    assert_eq!(
        manifest["source_tree_signature"]["algorithm"],
        "sha256(relpath,size,mtime_ns).v1"
    );
    assert!(
        manifest["owned_paths"]
            .as_array()
            .expect("owned paths")
            .contains(&Value::String(".jikji/doc_text/".to_owned()))
    );
    assert_eq!(manifest["media_index"]["enabled"], false);

    let doc_cache = root.join(
        jsonl(root.join(".jikji/document_index.jsonl"))[0]["text_cache_path"]
            .as_str()
            .expect("cache path"),
    );
    assert!(doc_cache.exists());
    fs::remove_file(root.join("report.rtf")).expect("delete rtf");
    let refreshed = json_cmd(["refresh", root_str(&root).as_str(), "--json"]);
    assert_eq!(refreshed["deleted"], 1);
    assert!(!doc_cache.exists());

    let doctor = json_cmd(["doctor", root_str(&root).as_str(), "--json"]);
    assert_eq!(doctor["ok"], true);
    assert_eq!(doctor["errors"].as_array().expect("errors").len(), 0);

    let map = run_ok(["map", root_str(&root).as_str()]);
    let map_text = String::from_utf8(map.stdout).expect("utf8 map");
    assert!(map_text.contains("Jikji Agent Map"));
    assert!(map_text.contains("docs/meeting.txt"));

    fs::write(root.join(".jikji/user-created-note.txt"), "user").expect("write user note");
    let dry = json_cmd(["clean", root_str(&root).as_str(), "--dry-run", "--json"]);
    assert_eq!(dry["dry_run"], true);
    assert!(
        dry["would_remove"]
            .as_array()
            .expect("would remove")
            .iter()
            .any(|path| is_jikji_manifest_path(path.as_str().expect("path")))
    );
    assert!(
        !dry["would_remove"]
            .as_array()
            .expect("would remove")
            .iter()
            .any(|path| path
                .as_str()
                .expect("path")
                .ends_with("user-created-note.txt"))
    );
    let cleaned = json_cmd(["clean", root_str(&root).as_str(), "--json"]);
    assert_eq!(cleaned["ok"], true);
    assert!(root.join(".jikji/user-created-note.txt").exists());
    assert!(!root.join(".jikji/manifest.json").exists());
    assert!(root.join("docs/meeting.txt").exists());
}

fn is_jikji_manifest_path(path: &str) -> bool {
    let path = Path::new(path);
    path.file_name().is_some_and(|name| name == "manifest.json")
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == ".jikji")
}

#[test]
fn prepare_has_no_default_cap_and_explicit_max_files_is_partial() {
    let no_cap = temp_root("no-cap");
    for idx in 0..5001 {
        fs::write(no_cap.join(format!("bulk_{idx:04}.bin")), []).expect("write bulk");
    }
    let prepared = json_cmd(["prepare", root_str(&no_cap).as_str(), "--json"]);
    assert_eq!(prepared["files"], 5001);

    let capped = temp_root("capped");
    for idx in 0..4 {
        fs::write(capped.join(format!("bulk_{idx}.txt")), "x").expect("write capped");
    }
    let partial = json_cmd([
        "prepare",
        root_str(&capped).as_str(),
        "--max-files",
        "3",
        "--json",
    ]);
    assert_eq!(partial["files"], 3);
    assert_eq!(json_file(capped.join(".jikji/manifest.json"))["files"], 3);

    let zero_disabled = temp_root("zero-cap-disabled");
    for idx in 0..3 {
        fs::write(zero_disabled.join(format!("bulk_{idx}.txt")), "x").expect("write zero cap");
    }
    let disabled = json_cmd([
        "prepare",
        root_str(&zero_disabled).as_str(),
        "--max-files",
        "0",
        "--json",
    ]);
    assert_eq!(disabled["files"], 3);
}

pub(crate) fn run<const N: usize>(args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(args)
        .output()
        .expect("run jikji")
}

pub(crate) fn run_ok<const N: usize>(args: [&str; N]) -> Output {
    let output = run(args);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub(crate) fn json_cmd<const N: usize>(args: [&str; N]) -> Value {
    let output = run_ok(args);
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

pub(crate) fn json_file(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
}

pub(crate) fn jsonl(path: PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl row"))
        .collect()
}

pub(crate) fn root_str(root: &Path) -> String {
    root.display().to_string()
}

pub(crate) struct TempRoot {
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

pub(crate) fn temp_root(label: &str) -> TempRoot {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("jikji-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("create temp root");
    TempRoot { path: root }
}
