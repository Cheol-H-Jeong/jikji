use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

#[test]
fn prepare_refresh_doctor_map_and_clean_use_central_database() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("note.txt"), "task3 central marker").unwrap();
    let prepared = fixture.json(["prepare", fixture.root_str().as_str(), "--json"]);
    assert_eq!(prepared["files"], 1);
    assert!(!fixture.root.join(".jikji").exists());
    assert!(fixture.database().is_file());

    let refreshed = fixture.json(["refresh", fixture.root_str().as_str(), "--json"]);
    assert_eq!(refreshed["files"], 1);
    let doctor = fixture.json(["doctor", fixture.root_str().as_str(), "--json"]);
    assert_eq!(doctor["ok"], true);
    let map = fixture.run(["map", fixture.root_str().as_str()]);
    assert!(map.status.success());
    assert!(String::from_utf8_lossy(&map.stdout).contains("note.txt"));

    let cleaned = fixture.json(["clean", fixture.root_str().as_str(), "--json"]);
    assert_eq!(cleaned["ok"], true);
    assert!(fixture.root.join("note.txt").is_file());
}

#[test]
fn explicit_max_files_remains_bounded_without_root_sidecars() {
    let fixture = Fixture::new();
    for index in 0..5 {
        fs::write(fixture.root.join(format!("file-{index}.txt")), "bounded").unwrap();
    }
    let prepared = fixture.json([
        "prepare",
        fixture.root_str().as_str(),
        "--max-files",
        "2",
        "--json",
    ]);
    assert_eq!(prepared["files"], 2);
    assert!(!fixture.root.join(".jikji").exists());
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    data: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let data = temp.path().join("data");
        fs::create_dir_all(&root).unwrap();
        Self {
            _temp: temp,
            root,
            data,
        }
    }
    fn root_str(&self) -> String {
        path_str(&self.root)
    }
    fn database(&self) -> PathBuf {
        self.data.join("jikji/index.sqlite")
    }
    fn run<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        Command::new(env!("CARGO_BIN_EXE_jikji"))
            .env("JIKJI_DATA_DIR", &self.data)
            .args(args)
            .output()
            .unwrap()
    }
    fn json<I, S>(&self, args: I) -> Value
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
