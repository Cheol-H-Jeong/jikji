use std::fs;
use std::path::Path;

use jikji_core::{Result, io_error};
use serde_json::{Value, json};

use crate::eval::generate_eval_set;
use crate::io::{absolute_path, invalid_input};
use crate::models::{EvalGenerateResult, ImportOptions};

pub fn import_fixture_dataset(dest: &Path, options: &ImportOptions) -> Result<EvalGenerateResult> {
    if !options.no_fetch {
        return Err(invalid_input(
            "network dataset downloads are not performed by the Rust smoke importer; pass --no-fetch",
        ));
    }
    let clean_dest = absolute_path(dest)?;
    fs::create_dir_all(&clean_dest).map_err(|source| io_error(&clean_dest, source))?;
    let corpus = clean_dest.join(format!("{}_{}", options.dataset, options.split));
    fs::create_dir_all(&corpus).map_err(|source| io_error(&corpus, source))?;
    let count = options.cases.max(1);
    for idx in 0..count {
        let path = corpus.join(format!("case_{idx:03}.txt"));
        fs::write(
            &path,
            format!(
                "{} {} fixture document {idx}\n",
                options.dataset, options.split
            ),
        )
        .map_err(|source| io_error(&path, source))?;
    }
    generate_eval_set(&corpus, count, Some(&clean_dest.join("eval_set.jsonl")))
}

pub fn public_dataset_contract(dest: &Path, name: &str, cases: usize) -> Result<Value> {
    let result = import_fixture_dataset(
        dest,
        &ImportOptions {
            dataset: name.to_owned(),
            split: "contract".to_owned(),
            cases,
            no_fetch: true,
        },
    )?;
    Ok(json!({
        "dataset": name,
        "corpus_root": result.root,
        "eval_set": result.eval_set,
        "cases": result.cases,
        "public_benchmark": true,
        "network": "not_used",
    }))
}
