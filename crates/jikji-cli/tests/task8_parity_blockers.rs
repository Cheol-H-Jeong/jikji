use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

#[test]
fn central_sqlite_prepare_and_filename_search_do_not_create_sidecars() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("visual.png"), b"not-real-image").unwrap();
    fixture.json(["prepare", fixture.root_str().as_str(), "--json"]);
    assert!(!fixture.root.join(".jikji").exists());
    let found = fixture.json(["find", fixture.root_str().as_str(), "visual.png", "--json"]);
    assert_eq!(found["paths"][0], "visual.png");
}

#[test]
fn cli_help_exposes_deep_index_and_agent_installers() {
    let fixture = Fixture::new();
    let help = fixture.run(["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    for command in [
        "deep-index",
        "agent-skill-install",
        "find",
        "search",
        "doctor",
    ] {
        assert!(text.contains(command), "missing {command}");
    }
}

#[test]
fn installed_skill_requires_jikji_first_and_bounded_fallback() {
    let fixture = Fixture::new();
    let skill = fixture.json(["skill-export", "--json"]);
    let markdown = skill["skill_markdown"].as_str().unwrap();
    assert!(markdown.contains("Jikji Find First"));
    assert!(markdown.contains("exactly one sharper Jikji retry"));
    assert!(markdown.contains("raw fallback"));
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
