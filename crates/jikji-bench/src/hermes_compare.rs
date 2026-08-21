use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

const ZERO_CHAT_MODES: [&str; 3] = ["answer-pack", "discover-direct", "jikji-answer-pack"];

#[derive(Clone, Debug)]
pub struct CompareOptions {
    pub raw_mode: String,
    pub jikji_mode: String,
    pub max_token_ratio: f64,
    pub max_call_ratio: f64,
    pub max_seconds_ratio: f64,
    pub max_avg_llm_calls: Option<f64>,
    pub max_p95_llm_calls: Option<i64>,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            raw_mode: "raw".to_owned(),
            jikji_mode: "jikji-discover".to_owned(),
            max_token_ratio: 0.75,
            max_call_ratio: 0.75,
            max_seconds_ratio: 1.0,
            max_avg_llm_calls: None,
            max_p95_llm_calls: None,
        }
    }
}

#[derive(Debug)]
pub enum ReportError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    Invalid(String),
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Json { path, source } => {
                write!(f, "invalid JSON in {}: {source}", path.display())
            }
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ReportError {}

pub fn compare_benchmark_reports(
    raw_report: &Path,
    jikji_report: &Path,
    options: &CompareOptions,
) -> Result<Value, ReportError> {
    let raw = read_metrics(raw_report, &options.raw_mode)?;
    let jikji = read_metrics(jikji_report, &options.jikji_mode)?;
    let raw_usage_status = string_or(&raw, "usage_status", "missing_usage");
    let jikji_usage_status = string_or(&jikji, "usage_status", "missing_usage");

    let mut checks = Map::new();
    checks.insert(
        "hit_at_1_not_lower".into(),
        json!(number(&jikji, "hit_at_1") >= number(&raw, "hit_at_1")),
    );
    checks.insert(
        "hit_at_10_not_lower".into(),
        json!(number(&jikji, "hit_at_10") >= number(&raw, "hit_at_10")),
    );
    checks.insert(
        "total_tokens_below_ratio".into(),
        json!(
            number(&jikji, "total_tokens")
                <= number(&raw, "total_tokens") * options.max_token_ratio
        ),
    );
    checks.insert(
        "llm_calls_below_ratio".into(),
        json!(number(&jikji, "llm_calls") <= number(&raw, "llm_calls") * options.max_call_ratio),
    );
    checks.insert(
        "seconds_not_slower".into(),
        json!(number(&jikji, "seconds") <= number(&raw, "seconds") * options.max_seconds_ratio),
    );
    checks.insert(
        "usage_accounting_ok".into(),
        json!(
            usage_status_ok(&options.raw_mode, &raw_usage_status)
                && usage_status_ok(&options.jikji_mode, &jikji_usage_status)
        ),
    );
    if let Some(maximum) = options.max_avg_llm_calls {
        checks.insert(
            "avg_llm_calls_below_budget".into(),
            json!(number(&jikji, "avg_llm_calls") <= maximum),
        );
    }
    if let Some(maximum) = options.max_p95_llm_calls {
        checks.insert(
            "p95_llm_calls_below_budget".into(),
            json!(integer(&jikji, "p95_llm_calls") <= maximum),
        );
    }
    let ok = checks.values().all(Value::as_bool_or_false);

    Ok(json!({
        "ok": ok,
        "raw_mode": options.raw_mode,
        "jikji_mode": options.jikji_mode,
        "checks": checks,
        "ratios": {
            "total_tokens": round4(number(&jikji, "total_tokens") / number(&raw, "total_tokens").max(1.0)),
            "llm_calls": round4(number(&jikji, "llm_calls") / number(&raw, "llm_calls").max(1.0)),
            "seconds": round4(number(&jikji, "seconds") / number(&raw, "seconds").max(1.0)),
        },
        "raw": raw,
        "jikji": jikji,
        "thresholds": {
            "max_token_ratio": options.max_token_ratio,
            "max_call_ratio": options.max_call_ratio,
            "max_seconds_ratio": options.max_seconds_ratio,
            "max_avg_llm_calls": options.max_avg_llm_calls,
            "max_p95_llm_calls": options.max_p95_llm_calls,
            "allowed_usage_statuses": {
                "all_modes": ["ok"],
                "zero_chat_modes": ZERO_CHAT_MODES,
                "zero_chat_status": "not_applicable_zero_chat",
            },
        },
    }))
}

fn read_metrics(path: &Path, mode: &str) -> Result<Map<String, Value>, ReportError> {
    let text = fs::read_to_string(path).map_err(|source| ReportError::Io {
        path: path.to_owned(),
        source,
    })?;
    let data: Value = serde_json::from_str(&text).map_err(|source| ReportError::Json {
        path: path.to_owned(),
        source,
    })?;
    let modes = data
        .get("modes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let available = modes.keys().cloned().collect::<Vec<_>>();
    let mode_data = modes.get(mode).and_then(Value::as_object).ok_or_else(|| {
        ReportError::Invalid(format!(
            "mode {mode:?} not found in {}; available={available:?}",
            path.display()
        ))
    })?;
    mode_data
        .get("metrics")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| {
            ReportError::Invalid(format!(
                "missing metrics for mode {mode:?} in {}",
                path.display()
            ))
        })
}

fn usage_status_ok(mode: &str, status: &str) -> bool {
    status == "ok" || (status == "not_applicable_zero_chat" && ZERO_CHAT_MODES.contains(&mode))
}

fn number(values: &Map<String, Value>, key: &str) -> f64 {
    values.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn integer(values: &Map<String, Value>, key: &str) -> i64 {
    values
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| number(values, key) as i64)
}

fn string_or(values: &Map<String, Value>, key: &str, default: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_owned()
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

trait ValueBool {
    fn as_bool_or_false(&self) -> bool;
}

impl ValueBool for Value {
    fn as_bool_or_false(&self) -> bool {
        self.as_bool().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn report(dir: &Path, name: &str, mode: &str, metrics: Value) -> PathBuf {
        let path = dir.join(name);
        fs::write(
            &path,
            serde_json::to_vec(&json!({"modes": {mode: {"metrics": metrics}}})).unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn compare_reports_covers_success_and_failure_gates() {
        let dir = tempfile::tempdir().unwrap();
        let raw = report(
            dir.path(),
            "raw.json",
            "raw",
            json!({"hit_at_1": 0.8, "hit_at_10": 0.9, "total_tokens": 1000, "llm_calls": 100, "seconds": 10, "usage_status": "ok"}),
        );
        let good = report(
            dir.path(),
            "good.json",
            "jikji-discover",
            json!({"hit_at_1": 0.8, "hit_at_10": 0.95, "total_tokens": 750, "llm_calls": 70, "seconds": 9, "usage_status": "ok", "avg_llm_calls": 7, "p95_llm_calls": 9}),
        );
        let options = CompareOptions {
            max_avg_llm_calls: Some(8.0),
            max_p95_llm_calls: Some(10),
            ..CompareOptions::default()
        };
        let success = compare_benchmark_reports(&raw, &good, &options).unwrap();
        assert_eq!(success["ok"], true);
        assert_eq!(
            success["ratios"],
            json!({"total_tokens": 0.75, "llm_calls": 0.7, "seconds": 0.9})
        );

        let bad = report(
            dir.path(),
            "bad.json",
            "jikji-discover",
            json!({"hit_at_1": 0.7, "hit_at_10": 0.95, "total_tokens": 751, "llm_calls": 70, "seconds": 9, "usage_status": "missing_usage"}),
        );
        let failure = compare_benchmark_reports(&raw, &bad, &options).unwrap();
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["checks"]["hit_at_1_not_lower"], false);
        assert_eq!(failure["checks"]["total_tokens_below_ratio"], false);
        assert_eq!(failure["checks"]["usage_accounting_ok"], false);
    }
}
