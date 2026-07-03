use std::process::ExitCode;

use crate::args::{
    BenchAnalyzeArgs, BenchIterateArgs, BenchRunArgs, EvalGenerateArgs, EvalRunArgs, ImportArgs,
    PublicImportArgs, PublicSuiteArgs,
};

pub(crate) fn run_eval_generate(_args: EvalGenerateArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("eval-generate")
}

pub(crate) fn run_eval(_args: EvalRunArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("eval")
}

pub(crate) fn run_bench_analyze(_args: BenchAnalyzeArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("bench-analyze")
}

pub(crate) fn run_hippocamp_import(_args: ImportArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("hippocamp-import")
}

pub(crate) fn run_bench_run(_args: BenchRunArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("bench-run")
}

pub(crate) fn run_bench_iterate(_args: BenchIterateArgs) -> jikji_core::Result<ExitCode> {
    python_only_benchmark("bench-iterate")
}

pub(crate) fn run_public_import(
    label: &'static str,
    _args: PublicImportArgs,
) -> jikji_core::Result<ExitCode> {
    python_only_benchmark(label)
}

pub(crate) fn run_public_suite(
    label: &'static str,
    _args: PublicSuiteArgs,
) -> jikji_core::Result<ExitCode> {
    python_only_benchmark(label)
}

fn python_only_benchmark(command: &'static str) -> jikji_core::Result<ExitCode> {
    Err(jikji_core::JikjiError::UnimplementedCommand(
        match command {
            "eval-generate" => {
                "eval-generate is Python-only; use the Python Jikji evaluator or tools/parity scripts so Rust and Python runs share one benchmark implementation"
            }
            "eval" => {
                "eval is Python-only; use the Python Jikji evaluator so Rust and Python runs share one benchmark implementation"
            }
            "bench-analyze" => {
                "bench-analyze is Python-only; use the Python Jikji evaluator reports"
            }
            "hippocamp-import" => {
                "hippocamp-import is Python-only; use Python Jikji to import HippoCamp annotations"
            }
            "bench-run" => {
                "bench-run is Python-only; use tools/parity/compare_victoria_python_eval.py for Rust-vs-Python recall comparisons"
            }
            "bench-iterate" => "bench-iterate is Python-only; use the Python Jikji evaluator loop",
            "beir" => "beir benchmark helpers are Python-only in the Rust port",
            "edith" => "edith benchmark helpers are Python-only in the Rust port",
            "publicdata" => "publicdata benchmark helpers are Python-only in the Rust port",
            "workspacebench" => "workspacebench benchmark helpers are Python-only in the Rust port",
            "hardbench" => "hardbench benchmark helpers are Python-only in the Rust port",
            "hippocamp" => {
                "hippocamp benchmark helpers are Python-only; use tools/parity/compare_victoria_python_eval.py for Rust-vs-Python recall comparisons"
            }
            _ => "benchmark helpers are Python-only in the Rust port",
        },
    ))
}
