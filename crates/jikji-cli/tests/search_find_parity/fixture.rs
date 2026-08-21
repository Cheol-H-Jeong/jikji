use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub(crate) struct TempRoot {
    path: PathBuf,
}

impl std::ops::Deref for TempRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jikji"));
    command.args(args);
    if let Some(root) = args.get(1) {
        command.env("JIKJI_DATA_DIR", data_dir_for(root));
    }
    command.output().expect("run jikji")
}

pub(crate) fn run_ok(args: &[&str]) -> Output {
    let output = run(args);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub(crate) fn json_cmd(args: &[&str]) -> Value {
    let output = run_ok(args);
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

pub(crate) fn root_arg(root: &Path) -> String {
    root.display().to_string()
}

pub(crate) fn database_path(root: &Path) -> PathBuf {
    data_dir_for(root.to_string_lossy().as_ref()).join("jikji/index.sqlite")
}

fn data_dir_for(root: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    std::env::temp_dir().join(format!("jikji-search-test-db-{:016x}", hasher.finish()))
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
