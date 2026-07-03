use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

const TASK3_CORE_ARTIFACTS: &[&str] = &[
    ".jikji/manifest.json",
    ".jikji/file_index.jsonl",
    ".jikji/folder_index.jsonl",
    ".jikji/document_index.jsonl",
    ".jikji/file_cards.jsonl",
    ".jikji/chunk_map.jsonl",
    ".jikji/search_index.sqlite",
    ".jikji/duplicate_map.jsonl",
    ".jikji/folder_profile.jsonl",
    ".jikji/corpus_profile.json",
    ".jikji/intent_taxonomy.json",
    ".jikji/autorag_manifest.json",
    ".jikji/knowledge_graph.json",
    ".jikji/graph_routes.jsonl",
    ".jikji/llm_wiki_schema.md",
    ".jikji/wiki/index.md",
    ".jikji/parse_errors.jsonl",
    ".jikji/agent_map.md",
    ".jikji/agent_routes.md",
    ".jikji/agent_skill_context.md",
    ".jikji/human_guide.md",
    ".jikji_agent_map.md",
];

pub(crate) fn assert_prepare_core_fields(actual: &Value, expected: &Value) {
    for key in [
        "files",
        "folders",
        "docs_parsed",
        "docs_reused",
        "docs_failed",
        "deleted",
    ] {
        assert_eq!(actual[key], expected[key], "prepare field {key}");
    }
}

pub(crate) fn assert_task3_artifacts_match_python_names(root: &Path, scenario: &str) {
    let golden_paths = golden_artifact_paths(scenario);
    for rel in TASK3_CORE_ARTIFACTS {
        assert!(golden_paths.contains(*rel), "Python golden missing {rel}");
        assert!(
            root.join(rel).exists(),
            "Rust missing Task 3 artifact {rel}"
        );
    }
}

pub(crate) fn assert_manifest_contract(root: &Path) {
    let manifest = json_file(root.join(".jikji/manifest.json"));
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["search_index_schema_version"], 3);
    assert_eq!(manifest["non_destructive"], true);
    assert_eq!(
        manifest["source_tree_signature"]["algorithm"],
        "sha256(relpath,size,mtime_ns).v1"
    );
    assert_path_list_contains(&manifest["owned_paths"], ".jikji/doc_text/");
    assert_path_list_contains(&manifest["owned_paths"], ".jikji/manifest.json");
}

pub(crate) fn assert_file_index_contract(root: &Path) {
    let paths = jsonl(root.join(".jikji/file_index.jsonl"))
        .into_iter()
        .map(|row| row["path"].as_str().expect("row path").to_owned())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path == "docs/acme-contract.txt"));
    assert!(
        paths
            .iter()
            .any(|path| path == "\u{c790}\u{b8cc}/\u{d68c}\u{c758}\u{b85d}.txt")
    );
    assert!(paths.iter().all(|path| path != ".env"));
    assert!(paths.iter().all(|path| !path.starts_with(".jikji/")));
}

pub(crate) fn assert_path_list_contains(value: &Value, suffix: &str) {
    assert!(
        value
            .as_array()
            .expect("path list")
            .iter()
            .any(|path| path_string_ends_with(path.as_str().expect("path"), suffix)),
        "missing path suffix {suffix}"
    );
}

pub(crate) fn assert_path_list_excludes(value: &Value, suffix: &str) {
    assert!(
        value
            .as_array()
            .expect("path list")
            .iter()
            .all(|path| !path_string_ends_with(path.as_str().expect("path"), suffix)),
        "unexpected path suffix {suffix}"
    );
}

pub(crate) fn path_string_ends_with(path: &str, suffix: &str) -> bool {
    path.replace('\\', "/").ends_with(suffix)
}

pub(crate) fn write_ascii_cjk_fixture(root: &Path) {
    fs::create_dir(root.join("docs")).expect("create docs");
    fs::write(
        root.join("docs/acme-contract.txt"),
        "ACME renewal contract contains indemnity marker direct-answer-771.",
    )
    .expect("write acme");
    fs::create_dir(root.join("\u{c790}\u{b8cc}")).expect("create cjk dir");
    fs::write(
        root.join("\u{c790}\u{b8cc}/\u{d68c}\u{c758}\u{b85d}.txt"),
        "\u{c11c}\u{c6b8} \u{c5f0}\u{ad6c}\u{c18c} \u{d68c}\u{c758}\u{b85d} contains cjk-marker-902.",
    )
    .expect("write cjk");
    fs::write(root.join(".env"), "secret").expect("write sensitive");
}

pub(crate) fn golden_command(scenario: &str, name: &str) -> Value {
    json_file(golden_root().join(format!("scenarios/{scenario}/commands.json")))
        .as_array()
        .expect("golden commands")
        .iter()
        .find(|command| command["name"] == name)
        .unwrap_or_else(|| panic!("missing golden command {scenario}/{name}"))
        .clone()
}

pub(crate) fn golden_artifact_paths(scenario: &str) -> BTreeSet<String> {
    json_file(golden_root().join(format!("scenarios/{scenario}/generated_files.json")))
        .as_array()
        .expect("golden artifacts")
        .iter()
        .map(|row| row["path"].as_str().expect("artifact path").to_owned())
        .collect()
}

pub(crate) fn json_cmd(args: &[&str]) -> Value {
    let output = run_ok(args);
    serde_json::from_slice(&output.stdout).expect("json stdout")
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

pub(crate) fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(args)
        .output()
        .expect("run jikji")
}

pub(crate) fn json_file(path: PathBuf) -> Value {
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

fn golden_root() -> PathBuf {
    workspace_root().join("tests/golden/python")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("jikji-cli crate must live under crates/jikji-cli")
        .to_path_buf()
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

pub(crate) fn root_arg(root: &Path) -> String {
    root.display().to_string()
}

pub(crate) fn minimal_png() -> &'static [u8] {
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00IEND\xaeB`\x82"
}
