use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn prepare_doctor_map_find_and_clean_use_central_sqlite() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("needle.txt"), "central sqlite marker").unwrap();

    let prepared = fixture.json(["prepare", fixture.root_str().as_str(), "--json"]);
    assert_eq!(prepared["files"], 1);
    assert!(!fixture.root.join(".jikji").exists());
    assert!(fixture.database().is_file());
    let doctor = fixture.json(["doctor", fixture.root_str().as_str(), "--json"]);
    assert_eq!(doctor["ok"], true);
    let map = fixture.run(["map", fixture.root_str().as_str()]);
    assert!(map.status.success());
    assert!(String::from_utf8_lossy(&map.stdout).contains("needle.txt"));

    let found = fixture.json([
        "find",
        fixture.root_str().as_str(),
        "central sqlite marker",
        "--json",
    ]);
    assert_eq!(found["paths"][0], "needle.txt");

    let clean = fixture.json(["clean", fixture.root_str().as_str(), "--json"]);
    assert_eq!(clean["ok"], true);
    let connection = Connection::open(fixture.database()).unwrap();
    let roots: i64 = connection
        .query_row("SELECT COUNT(*) FROM roots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(roots, 0);
    assert!(fixture.root.join("needle.txt").is_file());
}

#[test]
fn central_database_isolates_roots() {
    let fixture = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    fs::write(fixture.root.join("alpha.txt"), "only alpha").unwrap();
    fs::write(other.path().join("beta.txt"), "only beta").unwrap();
    fixture.json(["prepare", fixture.root_str().as_str(), "--json"]);
    fixture.json(["prepare", path_str(other.path()).as_str(), "--json"]);

    let alpha = fixture.json(["find", fixture.root_str().as_str(), "only beta", "--json"]);
    let beta = fixture.json([
        "find",
        path_str(other.path()).as_str(),
        "only beta",
        "--json",
    ]);
    assert!(
        !alpha["paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "beta.txt")
    );
    assert_eq!(beta["paths"][0], "beta.txt");
}

#[test]
fn doctor_reports_prepared_state_and_resolvable_artifact_paths() {
    let fixture = Fixture::new();
    let unprepared = fixture.run(["doctor", fixture.root_str().as_str(), "--json"]);
    assert!(!unprepared.status.success());
    let unprepared: Value = serde_json::from_slice(&unprepared.stdout).unwrap();
    assert_eq!(unprepared["prepared"], false);
    assert_eq!(unprepared["index_dir"], Value::Null);
    assert_eq!(unprepared["database"], path_str(&fixture.database()));

    fs::write(fixture.root.join("a.txt"), "refund policy").unwrap();
    let prepared = fixture.json(["prepare", fixture.root_str().as_str(), "--json"]);
    let doctor = fixture.json(["doctor", fixture.root_str().as_str(), "--json"]);
    assert_eq!(doctor["prepared"], true);
    assert_eq!(doctor["index_dir"], prepared["index_dir"]);
    assert_eq!(doctor["manifest"]["index_dir"], prepared["index_dir"]);
    let index_dir = PathBuf::from(prepared["index_dir"].as_str().unwrap());
    assert!(index_dir.starts_with(&fixture.data));
    for field in [
        "llm_wiki",
        "knowledge_graph",
        "graph_routes",
        "llm_wiki_schema",
    ] {
        let path = Path::new(doctor["manifest"][field].as_str().unwrap());
        assert!(
            path.is_absolute() && path.starts_with(&index_dir),
            "{field}: {}",
            path.display()
        );
        assert!(path.is_file(), "{field} must exist: {}", path.display());
    }
    let routes = fs::read_to_string(doctor["manifest"]["graph_routes"].as_str().unwrap()).unwrap();
    assert!(
        routes.contains("\"a.txt\""),
        "graph routes file is empty: {routes:?}"
    );
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
