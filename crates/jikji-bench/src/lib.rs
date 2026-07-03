#![forbid(unsafe_code)]

mod dataset;
mod eval;
mod io;
mod metrics;
mod models;
mod run;

pub use dataset::{import_fixture_dataset, public_dataset_contract};
pub use eval::{analyze_eval, generate_eval_set};
pub use models::{
    BenchmarkReport, BenchmarkScenario, EvalAnalyzeResult, EvalCase, EvalGenerateResult,
    EvalRunResult, ImportOptions, RunOptions,
};
pub use run::run_benchmark;

pub fn dry_run_report(scenario: &BenchmarkScenario) -> BenchmarkReport {
    BenchmarkReport {
        scenario_name: scenario.name.clone(),
        measured_operations: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use jikji_core::WorkspaceRoot;

    use super::{
        BenchmarkScenario, ImportOptions, RunOptions, analyze_eval, dry_run_report,
        generate_eval_set, import_fixture_dataset, public_dataset_contract, run_benchmark,
    };

    #[test]
    fn dry_run_report_has_no_external_dataset_requirement() {
        let scenario = BenchmarkScenario {
            root: WorkspaceRoot::new(PathBuf::from("/tmp/jikji-fixture")),
            name: "fixture".to_owned(),
        };

        let report = dry_run_report(&scenario);

        assert_eq!(report.scenario_name, "fixture");
        assert_eq!(report.measured_operations, 0);
    }

    #[test]
    fn eval_generate_run_and_analyze_use_local_fixtures() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("ACME_contract.txt"), "ACME payment clause").expect("write");

        let generated = generate_eval_set(dir.path(), 5, None).expect("generate");
        jikji_index::prepare(dir.path(), &jikji_core::PrepareOptions::default()).expect("prepare");
        let run = run_benchmark(
            dir.path(),
            &RunOptions {
                eval_set: Some(generated.eval_set.clone()),
                prepare: false,
                ..RunOptions::default()
            },
        )
        .expect("run");
        let analysis = analyze_eval(dir.path(), Some(&run.report)).expect("analyze");

        assert_eq!(generated.cases, 1);
        assert_eq!(analysis.cases, 1);
        assert!(run.report.exists());
    }

    #[test]
    fn external_dataset_contract_is_smoke_only_without_network() {
        let dir = tempfile::tempdir().expect("tempdir");

        let imported = import_fixture_dataset(
            dir.path(),
            &ImportOptions {
                dataset: "beir-scifact".to_owned(),
                cases: 2,
                ..ImportOptions::default()
            },
        )
        .expect("import");
        let contract =
            public_dataset_contract(&dir.path().join("public"), "edith", 1).expect("contract");

        assert_eq!(imported.cases, 2);
        assert_eq!(contract["network"], "not_used");
    }

    #[test]
    fn bench_run_accepts_python_style_expected_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let eval_dir = tempfile::tempdir().expect("eval tempdir");
        fs::create_dir_all(dir.path().join("docs")).expect("mkdir");
        fs::write(dir.path().join("docs/target.txt"), "needle answer").expect("write target");
        fs::write(dir.path().join("docs/decoy.txt"), "unrelated").expect("write decoy");
        jikji_index::prepare(dir.path(), &jikji_core::PrepareOptions::default()).expect("prepare");
        let eval_set = eval_dir.path().join("hippocamp_eval.jsonl");
        fs::write(
            &eval_set,
            r#"{"query":"needle","expected_paths":["docs/missing.txt","docs/target.txt"],"scenario":"hippocamp"}
"#,
        )
        .expect("write eval");

        let run = run_benchmark(
            dir.path(),
            &RunOptions {
                eval_set: Some(eval_set),
                modes: vec!["jikji".to_owned()],
                ..RunOptions::default()
            },
        )
        .expect("run");

        assert_eq!(run.metrics["jikji"]["hit_at_1"], 1.0);
    }
}
