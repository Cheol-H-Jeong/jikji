use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use jikji_bench::{
    CompareOptions, ImportOptions, Pricing, RunOptions, analyze_eval, compare_benchmark_reports,
    generate_eval_set, import_fixture_dataset, run_benchmark, write_accuracy_first_value_report,
    write_two_call_value_report,
};
use jikji_hermes_bench::{HermesBenchOptions, run_hermes_benchmark};
use jikji_public_datasets::HttpFetcher;
use jikji_public_datasets::beir::{
    BeirFetchOptions, BeirMaterializeOptions, fetch_beir_dataset, materialize_beir_dataset,
};
use jikji_public_datasets::hippocamp::{
    HippoCampFetchOptions, HippoCampImportOptions, fetch_subset, import_eval_set,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::args::{
    BenchAnalyzeArgs, BenchIterateArgs, BenchRunArgs, BenchmarkValueReportArgs, EvalGenerateArgs,
    EvalRunArgs, HermesBenchArgs, HermesCompareArgs, ImportArgs, PublicImportArgs, PublicSuiteArgs,
};
use crate::output::print_json;

pub(crate) fn run_eval_generate(args: EvalGenerateArgs) -> jikji_core::Result<ExitCode> {
    let result = generate_eval_set(&args.root, args.cases, args.out.as_deref())?;
    if args.json {
        print_json(&result)?;
    } else {
        println!("Jikji eval set generated: {}", result.eval_set.display());
        println!("- cases={} scenarios={:?}", result.cases, result.scenarios);
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_eval(args: EvalRunArgs) -> jikji_core::Result<ExitCode> {
    let result = run_benchmark(
        &args.root,
        &RunOptions {
            eval_set: args.eval_set,
            modes: vec!["jikji".to_owned()],
            top_k: args.top_k,
            prepare: false,
            allow_leak: false,
        },
    )?;
    let metrics = result.metrics.get("jikji").cloned().unwrap_or(Value::Null);
    if args.json {
        print_json(&json!({
            "root": result.root,
            "eval_set": result.eval_set,
            "report": result.report,
            "metrics": metrics,
        }))?;
    } else {
        println!("Jikji eval complete: {}", result.report.display());
        println!("- {}", metrics_line(&metrics));
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_bench_analyze(args: BenchAnalyzeArgs) -> jikji_core::Result<ExitCode> {
    let result = analyze_eval(&args.root, args.report.as_deref())?;
    if args.json {
        print_json(&result)?;
    } else {
        println!(
            "Jikji benchmark analysis complete: {}",
            result.analysis.display()
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result.summary)
                .map_err(|source| { jikji_core::json_error("<stdout>", source) })?
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_hippocamp_import(args: ImportArgs) -> jikji_core::Result<ExitCode> {
    let mut options = HippoCampImportOptions::new(&args.path);
    options.annotation = args.annotation;
    options.max_cases = args.cases;
    options.output = args.out;
    let result = import_eval_set(&options).map_err(dataset_error)?;
    if args.json {
        print_json(&result)?;
    } else {
        println!(
            "HippoCamp eval set written: {}",
            result.eval_set_path.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_bench_run(args: BenchRunArgs) -> jikji_core::Result<ExitCode> {
    let result = run_benchmark(
        &args.root,
        &RunOptions {
            eval_set: args.eval_set,
            modes: split_modes(&args.modes),
            top_k: args.top_k,
            prepare: args.prepare,
            allow_leak: args.allow_leak,
        },
    )?;
    if args.json {
        print_json(&json!({
            "report": result.report,
            "metrics": result.metrics,
        }))?;
    } else {
        println!("Benchmark complete: {}", result.report.display());
        print_metrics_map(&result.metrics);
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_bench_iterate(args: BenchIterateArgs) -> jikji_core::Result<ExitCode> {
    let root = args
        .root
        .canonicalize()
        .map_err(|source| jikji_core::io_error(&args.root, source))?;
    let iterations = args.iterations.max(1);
    let modes = split_modes(&args.modes);
    let target_mode = if modes.iter().any(|mode| mode == "jikji") {
        "jikji"
    } else {
        modes.last().map(String::as_str).unwrap_or("raw")
    };
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("root");
    let out = args
        .eval_set
        .parent()
        .unwrap_or(root.as_path())
        .join(format!("jikji_improvement_loop_{root_name}.json"));
    let mut entries = Vec::with_capacity(iterations);
    let mut best_metrics = Value::Object(serde_json::Map::new());
    let mut best_score = f64::NEG_INFINITY;
    for iteration in 1..=iterations {
        let result = run_benchmark(
            &root,
            &RunOptions {
                eval_set: Some(args.eval_set.clone()),
                modes: modes.clone(),
                top_k: args.top_k,
                prepare: false,
                allow_leak: false,
            },
        )?;
        let metrics = result.metrics.clone();
        let target = metrics.get(target_mode).cloned().unwrap_or(Value::Null);
        let score = target
            .get("hit_at_1")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            + target.get("mrr").and_then(Value::as_f64).unwrap_or(0.0) / 10.0;
        if score >= best_score {
            best_score = score;
            best_metrics = metrics.clone();
        }
        entries.push(json!({
            "iteration": iteration,
            "run_type": "deterministic_repeat_after_current_implementation",
            "target_mode": target_mode,
            "benchmark_report": result.report,
            "metrics": metrics,
        }));
    }
    let payload = json!({
        "root": root,
        "eval_set": args.eval_set,
        "iterations_requested": iterations,
        "iterations": entries,
        "completed_iterations": iterations,
        "best_metrics": best_metrics,
    });
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|source| jikji_core::json_error(&out, source))?;
    fs::write(&out, format!("{text}\n")).map_err(|source| jikji_core::io_error(&out, source))?;
    let result = IterateResult {
        report: out,
        iterations,
        best_metrics: payload["best_metrics"].clone(),
    };
    if args.json {
        print_json(&result)?;
    } else {
        println!(
            "Benchmark repeat loop complete: {}",
            result.report.display()
        );
        println!("- iterations={}", result.iterations);
        println!(
            "- best={}",
            serde_json::to_string(&result.best_metrics)
                .map_err(|source| jikji_core::json_error("<stdout>", source))?
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct IterateResult {
    report: PathBuf,
    iterations: usize,
    best_metrics: Value,
}

fn split_modes(modes: &str) -> Vec<String> {
    modes
        .split(',')
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(str::to_owned)
        .collect()
}

fn metrics_line(metrics: &Value) -> String {
    let null = Value::Null;
    let object = metrics.as_object();
    format!(
        "cases={} hit@1={} hit@5={} hit@10={} mrr={}",
        object.and_then(|value| value.get("cases")).unwrap_or(&null),
        object
            .and_then(|value| value.get("hit_at_1"))
            .unwrap_or(&null),
        object
            .and_then(|value| value.get("hit_at_5"))
            .unwrap_or(&null),
        object
            .and_then(|value| value.get("hit_at_10"))
            .unwrap_or(&null),
        object.and_then(|value| value.get("mrr")).unwrap_or(&null),
    )
}

fn print_metrics_map(metrics: &Value) {
    if let Some(object) = metrics.as_object() {
        for (mode, value) in object {
            println!("- {mode}: {}", metrics_line(value));
        }
    }
}

pub(crate) fn run_public_import(
    label: &'static str,
    args: PublicImportArgs,
) -> jikji_core::Result<ExitCode> {
    let payload = match label {
        "beir" if args.no_fetch => serde_json::to_value(import_fixture_dataset(
            &args.dest,
            &ImportOptions {
                dataset: args.dataset,
                split: "fixture".to_owned(),
                cases: args.cases,
                no_fetch: true,
            },
        )?)
        .map_err(|source| jikji_core::json_error("beir-import", source))?,
        "beir" => {
            let mut materialize = BeirMaterializeOptions::new(&args.dataset, &args.dest);
            materialize.max_cases = args.cases;
            fetch_beir_dataset(
                &HttpFetcher::default(),
                &BeirFetchOptions::new(&args.dataset, &args.dest),
            )
            .map_err(dataset_error)?;
            serde_json::to_value(materialize_beir_dataset(&materialize).map_err(dataset_error)?)
                .map_err(|source| jikji_core::json_error("beir-import", source))?
        }
        "hippocamp" => {
            if args.no_fetch {
                return Err(jikji_core::io_error(
                    "hippocamp-fetch",
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "hippocamp-fetch requires network access; remove --no-fetch",
                    ),
                ));
            }
            let mut options = HippoCampFetchOptions::new(&args.dest);
            options.profile = args.dataset;
            options.max_files = args.cases.max(1);
            serde_json::to_value(
                fetch_subset(&HttpFetcher::default(), &options).map_err(dataset_error)?,
            )
            .map_err(|source| jikji_core::json_error("hippocamp-fetch", source))?
        }
        other => {
            return Err(jikji_core::io_error(
                other,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "native Rust materializer is not wired for this dataset command",
                ),
            ));
        }
    };
    if args.json {
        print_json(&payload)?;
    } else {
        println!("{label} dataset prepared: {}", args.dest.display());
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_hermes_bench(args: HermesBenchArgs) -> jikji_core::Result<ExitCode> {
    let root = args
        .root
        .canonicalize()
        .map_err(|source| jikji_core::io_error(&args.root, source))?;
    let eval_set = args
        .eval_set
        .unwrap_or_else(|| root.join(".jikji/eval/hermes_eval_set.jsonl"));
    let out = args
        .out
        .unwrap_or_else(|| root.join(".jikji/eval/hermes_benchmark_report.json"));
    let result = run_hermes_benchmark(&HermesBenchOptions {
        root,
        eval_set,
        out,
        modes: split_modes(&args.modes),
        cases_limit: (args.cases > 0).then_some(args.cases),
        hermes_bin: PathBuf::from(args.hermes_bin),
        model: args.model,
        provider: args.provider,
        timeout: std::time::Duration::from_secs(args.timeout),
        max_turns: args.max_turns as u32,
        fast_max_turns: args.fast_max_turns as u32,
        skills: args.skills,
        candidate_top_k: args.candidate_top_k,
        retries: args.retries,
        allow_leak: args.allow_leak,
        yolo: args.yolo,
        hermes_home: None,
    })
    .map_err(|error| jikji_core::io_error("hermes-bench", io::Error::other(error.to_string())))?;
    if args.json {
        print_json(&json!({"report": result.report_path, "metrics": result.metrics}))?;
    } else {
        println!(
            "Hermes benchmark complete: {}",
            result.report_path.display()
        );
        print_metrics_map(&json!(result.metrics));
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_hermes_compare(args: HermesCompareArgs) -> jikji_core::Result<ExitCode> {
    let result = compare_benchmark_reports(
        &args.raw_report,
        &args.jikji_report,
        &CompareOptions {
            raw_mode: args.raw_mode,
            jikji_mode: args.jikji_mode,
            max_token_ratio: args.max_token_ratio,
            max_call_ratio: args.max_call_ratio,
            max_seconds_ratio: args.max_seconds_ratio,
            max_avg_llm_calls: args.max_avg_llm_calls,
            max_p95_llm_calls: args.max_p95_llm_calls.map(|v| v as i64),
        },
    )
    .map_err(|error| jikji_core::io_error("hermes-compare", io::Error::other(error.to_string())))?;
    if args.json {
        print_json(&result)?;
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|source| jikji_core::json_error("<stdout>", source))?
        );
    }
    Ok(if result["ok"].as_bool().unwrap_or(false) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) fn run_benchmark_value_report(
    args: BenchmarkValueReportArgs,
) -> jikji_core::Result<ExitCode> {
    let pricing = Pricing {
        input_per_1m_usd: args.input_per_1m_usd,
        output_per_1m_usd: args.output_per_1m_usd,
        usd_to_krw: args.usd_to_krw,
    };
    let raw = args.raw_discover_dir.unwrap_or(args.raw_report_dir);
    let result = if let Some(answer_dir) = args.answer_pack_dir {
        write_two_call_value_report(
            &raw,
            &args.out,
            &answer_dir,
            args.answer_pack_report.as_deref(),
            pricing,
            args.judge_top_k as i64,
            args.llm_latency_seconds,
        )
        .map_err(|error| jikji_core::io_error("benchmark-value-report", io::Error::other(error)))?
    } else {
        write_accuracy_first_value_report(
            &raw,
            &args.out,
            args.answer_pack_report.as_deref(),
            pricing,
        )
        .map_err(|error| jikji_core::io_error("benchmark-value-report", io::Error::other(error)))?
    };
    if args.json {
        print_json(&result)?;
    } else {
        println!("Benchmark value report written: {}", args.out.display());
    }
    Ok(ExitCode::SUCCESS)
}
pub(crate) fn run_public_suite(
    label: &'static str,
    _args: PublicSuiteArgs,
) -> jikji_core::Result<ExitCode> {
    Err(jikji_core::io_error(
        label,
        io::Error::new(
            io::ErrorKind::Unsupported,
            "native Rust suite is not wired for this dataset command",
        ),
    ))
}

fn dataset_error(error: jikji_public_datasets::DatasetError) -> jikji_core::JikjiError {
    jikji_core::io_error("public-dataset", io::Error::other(error.to_string()))
}
