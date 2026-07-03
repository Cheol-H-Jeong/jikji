use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct EvalGenerateArgs {
    #[arg(default_value = ".")]
    pub(crate) root: PathBuf,
    #[arg(long, default_value_t = 80)]
    pub(crate) cases: usize,
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct EvalRunArgs {
    #[arg(default_value = ".")]
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) eval_set: Option<PathBuf>,
    #[arg(long, default_value_t = 10)]
    pub(crate) top_k: usize,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct BenchAnalyzeArgs {
    #[arg(default_value = ".")]
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) report: Option<PathBuf>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct ImportArgs {
    pub(crate) path: PathBuf,
    #[arg(long)]
    pub(crate) annotation: Option<PathBuf>,
    #[arg(long, default_value_t = 200)]
    pub(crate) cases: usize,
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct BenchRunArgs {
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) eval_set: Option<PathBuf>,
    #[arg(long, default_value = "raw,jikji")]
    pub(crate) modes: String,
    #[arg(long, default_value_t = 10)]
    pub(crate) top_k: usize,
    #[arg(long)]
    pub(crate) prepare: bool,
    #[arg(long)]
    pub(crate) allow_leak: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct BenchIterateArgs {
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) eval_set: PathBuf,
    #[arg(long, default_value_t = 20)]
    pub(crate) iterations: usize,
    #[arg(long, default_value = "raw,jikji")]
    pub(crate) modes: String,
    #[arg(long, default_value_t = 10)]
    pub(crate) top_k: usize,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct PublicImportArgs {
    pub(crate) dest: PathBuf,
    #[arg(long, default_value = "fixture")]
    pub(crate) dataset: String,
    #[arg(long, default_value_t = 3)]
    pub(crate) cases: usize,
    #[arg(long)]
    pub(crate) no_fetch: bool,
    #[arg(long)]
    pub(crate) no_docs: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct PublicSuiteArgs {
    pub(crate) dest: PathBuf,
    #[arg(long, default_value_t = 3)]
    pub(crate) cases: usize,
    #[arg(long, default_value_t = 10)]
    pub(crate) top_k: usize,
    #[arg(long)]
    pub(crate) no_fetch: bool,
    #[arg(long)]
    pub(crate) no_prepare: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HermesBenchArgs {
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) eval_set: Option<PathBuf>,
    #[arg(long, default_value = "raw,jikji")]
    pub(crate) modes: String,
    #[arg(long, default_value_t = 0)]
    pub(crate) cases: usize,
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
    #[arg(long, default_value = "hermes")]
    pub(crate) hermes_bin: String,
    #[arg(long, default_value = "")]
    pub(crate) model: String,
    #[arg(long, default_value = "")]
    pub(crate) provider: String,
    #[arg(long, default_value_t = 240)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = 20)]
    pub(crate) max_turns: usize,
    #[arg(long, default_value_t = 1)]
    pub(crate) fast_max_turns: usize,
    #[arg(long, default_value = "")]
    pub(crate) skills: String,
    #[arg(long, default_value_t = 20)]
    pub(crate) candidate_top_k: usize,
    #[arg(long, default_value_t = 1)]
    pub(crate) retries: usize,
    #[arg(long)]
    pub(crate) yolo: bool,
    #[arg(long)]
    pub(crate) allow_leak: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HermesCompareArgs {
    pub(crate) raw_report: PathBuf,
    pub(crate) jikji_report: PathBuf,
    #[arg(long, default_value = "raw")]
    pub(crate) raw_mode: String,
    #[arg(long, default_value = "jikji")]
    pub(crate) jikji_mode: String,
    #[arg(long, default_value_t = 0.75)]
    pub(crate) max_token_ratio: f64,
    #[arg(long, default_value_t = 0.75)]
    pub(crate) max_call_ratio: f64,
    #[arg(long, default_value_t = 1.0)]
    pub(crate) max_seconds_ratio: f64,
    #[arg(long)]
    pub(crate) max_avg_llm_calls: Option<f64>,
    #[arg(long)]
    pub(crate) max_p95_llm_calls: Option<usize>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct BenchmarkValueReportArgs {
    #[arg(
        long,
        default_value = ".benchmarks/hippocamp-full/_hermes_full_gpt54mini_chunks5"
    )]
    pub(crate) raw_report_dir: PathBuf,
    #[arg(long)]
    pub(crate) raw_discover_dir: Option<PathBuf>,
    #[arg(
        long,
        default_value = ".benchmarks/hippocamp-full/_hermes_answer_pack_full_20260623_anchorfix/full_answer_pack_aggregate_report.json"
    )]
    pub(crate) find_candidate_report: PathBuf,
    #[arg(long)]
    pub(crate) answer_pack_report: Option<PathBuf>,
    #[arg(
        long,
        default_value = ".benchmarks/hippocamp-full/_hermes_answer_pack_full_20260623_anchorfix"
    )]
    pub(crate) find_candidate_dir: PathBuf,
    #[arg(long)]
    pub(crate) answer_pack_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    pub(crate) judge_top_k: usize,
    #[arg(long, default_value_t = 1.5)]
    pub(crate) llm_latency_seconds: f64,
    #[arg(long, default_value = "docs/jikji-value-report.json")]
    pub(crate) out: PathBuf,
    #[arg(long, default_value_t = 0.30)]
    pub(crate) input_per_1m_usd: f64,
    #[arg(long, default_value_t = 2.50)]
    pub(crate) output_per_1m_usd: f64,
    #[arg(long, default_value_t = 1380.0)]
    pub(crate) usd_to_krw: f64,
    #[arg(long)]
    pub(crate) json: bool,
}
