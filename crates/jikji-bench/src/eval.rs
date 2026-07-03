use std::collections::BTreeMap;
use std::path::Path;

use jikji_core::Result;
use serde_json::{Value, json};

use crate::io::{canonical_dir, corpus_files, read_json, write_json, write_jsonl};
use crate::models::{
    EVAL_ANALYSIS_NAME, EVAL_DIR, EVAL_REPORT_NAME, EVAL_SET_NAME, EvalAnalyzeResult, EvalCase,
    EvalGenerateResult,
};

pub fn generate_eval_set(
    root: &Path,
    max_cases: usize,
    out: Option<&Path>,
) -> Result<EvalGenerateResult> {
    let clean_root = canonical_dir(root)?;
    let paths = corpus_files(&clean_root)?;
    let limit = max_cases.max(1);
    let cases = paths
        .into_iter()
        .take(limit)
        .map(|relative| EvalCase {
            query: query_for_path(&relative),
            expected_paths: vec![relative],
            scenario: "filename".to_owned(),
        })
        .collect::<Vec<_>>();
    let eval_set = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| clean_root.join(EVAL_DIR).join(EVAL_SET_NAME));
    write_jsonl(&eval_set, &cases)?;
    let mut scenarios = BTreeMap::new();
    scenarios.insert("filename".to_owned(), cases.len());
    Ok(EvalGenerateResult {
        root: clean_root,
        eval_set,
        cases: cases.len(),
        scenarios,
    })
}

pub fn analyze_eval(root: &Path, report: Option<&Path>) -> Result<EvalAnalyzeResult> {
    let clean_root = canonical_dir(root)?;
    let report_path = report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| clean_root.join(EVAL_DIR).join(EVAL_REPORT_NAME));
    let payload = read_json(&report_path)?;
    let metrics = payload
        .get("metrics")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let cases = metrics
        .values()
        .filter_map(|mode| mode.get("cases").and_then(Value::as_u64))
        .max()
        .unwrap_or(0) as usize;
    let summary = json!({
        "modes": metrics.keys().cloned().collect::<Vec<_>>(),
        "cases": cases,
        "best_hit_at_1": metrics
            .values()
            .filter_map(|mode| mode.get("hit_at_1").and_then(Value::as_f64))
            .fold(0.0, f64::max),
        "network": "not_used",
    });
    let analysis = clean_root.join(EVAL_DIR).join(EVAL_ANALYSIS_NAME);
    write_json(&analysis, &summary)?;
    Ok(EvalAnalyzeResult {
        root: clean_root,
        analysis,
        cases,
        summary,
    })
}

fn query_for_path(relative: &str) -> String {
    Path::new(relative)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(relative)
        .replace(['_', '-'], " ")
}
