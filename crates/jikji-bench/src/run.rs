use std::path::Path;
use std::time::Instant;

use jikji_core::{PrepareOptions, Result};
use jikji_index::prepare;
use jikji_search::{SearchOptions, search};
use serde_json::{Value, json};

use crate::io::{assert_no_leak, canonical_dir, corpus_files, read_eval_cases, write_json};
use crate::metrics::{metrics, rank_of_any, round3};
use crate::models::{
    EVAL_DIR, EVAL_REPORT_NAME, EVAL_SET_NAME, EvalCase, EvalRunResult, RunOptions,
};

pub fn run_benchmark(root: &Path, options: &RunOptions) -> Result<EvalRunResult> {
    let clean_root = canonical_dir(root)?;
    if options.prepare {
        prepare(&clean_root, &PrepareOptions::default())?;
    }
    let eval_set = options
        .eval_set
        .clone()
        .unwrap_or_else(|| clean_root.join(EVAL_DIR).join(EVAL_SET_NAME));
    assert_no_leak(&clean_root, &eval_set, options.allow_leak)?;
    let cases = read_eval_cases(&eval_set)?;
    let started = Instant::now();
    let mut mode_metrics = serde_json::Map::new();
    for mode in normalized_modes(&options.modes) {
        let metrics = match mode.as_str() {
            "raw" => run_raw_mode(&clean_root, &cases, options.top_k)?,
            "jikji" | "jikji-find" => run_jikji_mode(&clean_root, &cases, options.top_k)?,
            _ => {
                return Err(crate::io::invalid_input(format!(
                    "unknown benchmark mode {mode:?}; expected raw or jikji"
                )));
            }
        };
        mode_metrics.insert(mode, metrics);
    }
    let report = clean_root.join(EVAL_DIR).join(EVAL_REPORT_NAME);
    let payload = json!({
        "root": clean_root,
        "eval_set": eval_set,
        "metrics": mode_metrics,
        "seconds": round3(started.elapsed().as_secs_f64()),
        "network": "not_used",
    });
    write_json(&report, &payload)?;
    Ok(EvalRunResult {
        root: clean_root,
        eval_set,
        report,
        metrics: payload["metrics"].clone(),
    })
}

fn run_raw_mode(root: &Path, cases: &[EvalCase], top_k: usize) -> Result<Value> {
    let files = corpus_files(root)?;
    let mut ranks = Vec::new();
    for case in cases {
        let terms = case.query.to_lowercase();
        let mut ranked = files
            .iter()
            .filter(|path| path.to_lowercase().contains(&terms))
            .cloned()
            .collect::<Vec<_>>();
        if ranked.is_empty() {
            ranked = files.clone();
        }
        ranked.truncate(top_k.max(1));
        ranks.push(rank_of_any(&ranked, &case.expected_paths));
    }
    Ok(metrics(cases.len(), &ranks))
}

fn run_jikji_mode(root: &Path, cases: &[EvalCase], top_k: usize) -> Result<Value> {
    let mut ranks = Vec::new();
    for case in cases {
        let ranked = search(
            root,
            &case.query,
            SearchOptions {
                top_k: top_k.max(1),
            },
        )?
        .into_iter()
        .map(|candidate| candidate.path)
        .collect::<Vec<_>>();
        ranks.push(rank_of_any(&ranked, &case.expected_paths));
    }
    Ok(metrics(cases.len(), &ranks))
}

fn normalized_modes(modes: &[String]) -> Vec<String> {
    let out = modes
        .iter()
        .flat_map(|mode| mode.split(','))
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if out.is_empty() {
        vec!["raw".to_owned(), "jikji".to_owned()]
    } else {
        out
    }
}
