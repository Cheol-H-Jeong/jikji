use std::collections::BTreeMap;
use std::path::PathBuf;

use jikji_core::WorkspaceRoot;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub(crate) const EVAL_DIR: &str = ".jikji/eval";
pub(crate) const EVAL_SET_NAME: &str = "eval_set.jsonl";
pub(crate) const EVAL_REPORT_NAME: &str = "eval_report.json";
pub(crate) const EVAL_ANALYSIS_NAME: &str = "eval_analysis.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkScenario {
    pub root: WorkspaceRoot,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchmarkReport {
    pub scenario_name: String,
    pub measured_operations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCase {
    pub query: String,
    #[serde(
        default,
        alias = "expected_path",
        deserialize_with = "deserialize_expected_paths"
    )]
    pub expected_paths: Vec<String>,
    #[serde(default)]
    pub scenario: String,
}

fn deserialize_expected_paths<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if let Some(path) = value.as_str() {
        return Ok(vec![path.to_owned()]);
    }
    if let Some(paths) = value.as_array() {
        return Ok(paths
            .iter()
            .filter_map(|path| path.as_str().map(str::to_owned))
            .collect());
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvalGenerateResult {
    pub root: PathBuf,
    pub eval_set: PathBuf,
    pub cases: usize,
    pub scenarios: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalRunResult {
    pub root: PathBuf,
    pub eval_set: PathBuf,
    pub report: PathBuf,
    pub metrics: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalAnalyzeResult {
    pub root: PathBuf,
    pub analysis: PathBuf,
    pub cases: usize,
    pub summary: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub eval_set: Option<PathBuf>,
    pub modes: Vec<String>,
    pub top_k: usize,
    pub prepare: bool,
    pub allow_leak: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            eval_set: None,
            modes: vec!["raw".to_owned(), "jikji".to_owned()],
            top_k: 10,
            prepare: false,
            allow_leak: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    pub dataset: String,
    pub split: String,
    pub cases: usize,
    pub no_fetch: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            dataset: "fixture".to_owned(),
            split: "test".to_owned(),
            cases: 3,
            no_fetch: true,
        }
    }
}
