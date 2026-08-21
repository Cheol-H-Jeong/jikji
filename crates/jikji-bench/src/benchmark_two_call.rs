use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::benchmark_value::{Pricing, build_accuracy_first_value_report, estimate_cost};

pub const QUERY_WRITE_INPUT_BASE: i64 = 180;
pub const QUERY_WRITE_OUTPUT_TOKENS: i64 = 24;
pub const JUDGE_INPUT_BASE: i64 = 260;
pub const JUDGE_OUTPUT_TOKENS: i64 = 48;
pub const ONE_CALL_JUDGE_OUTPUT_TOKENS: i64 = 32;
pub const DEFAULT_LLM_LATENCY_SECONDS: f64 = 1.5;

pub fn build_two_call_value_report(
    raw_dir: &Path,
    answer_dir: &Path,
    answer_pack_report: Option<&Path>,
    pricing: Pricing,
    judge_top_k: i64,
    latency: f64,
) -> Result<Value, String> {
    let mut payload = build_accuracy_first_value_report(raw_dir, answer_pack_report, pricing)?;
    let one = load_policy(answer_dir, pricing, judge_top_k, latency, true)?;
    let two = load_policy(answer_dir, pricing, judge_top_k, latency, false)?;
    payload["modes"]["jikji-one-call-judge"] = one;
    payload["modes"]["jikji-two-call-judge"] = two;
    payload["headline_strategy"] = json!("jikji-one-call-raw-floor");
    payload["one_call_policy"] = json!({
        "mode": "jikji-one-call-judge", "headline_mode": "jikji-one-call-raw-floor",
        "calls_per_cycle": 1, "judge_top_k": judge_top_k,
        "llm_latency_seconds_per_call": latency,
        "source_answer_pack_dir": answer_dir.to_string_lossy()
    });
    payload["two_call_policy"] = json!({
        "mode": "jikji-two-call-judge", "calls_per_cycle": 2,
        "judge_top_k": judge_top_k, "llm_latency_seconds_per_call": latency,
        "source_answer_pack_dir": answer_dir.to_string_lossy()
    });
    Ok(payload)
}

pub fn write_two_call_value_report(
    raw_dir: &Path,
    out: &Path,
    answer_dir: &Path,
    answer_pack_report: Option<&Path>,
    pricing: Pricing,
    judge_top_k: i64,
    latency: f64,
) -> Result<Value, String> {
    let value = build_two_call_value_report(
        raw_dir,
        answer_dir,
        answer_pack_report,
        pricing,
        judge_top_k,
        latency,
    )?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(out, serde_json::to_vec_pretty(&value).unwrap()).map_err(|e| e.to_string())?;
    Ok(value)
}

fn load_policy(
    dir: &Path,
    pricing: Pricing,
    top_k: i64,
    latency: f64,
    one_call: bool,
) -> Result<Value, String> {
    let mut cases = 0_i64;
    let mut hits = 0_i64;
    let mut calls = 0_i64;
    let mut prompt = 0_i64;
    let mut completion = 0_i64;
    let mut seconds = 0.0;
    let mut paths = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.to_string_lossy()
                .ends_with("_jikji_answer_pack_report.json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let report: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for detail in report["modes"]["jikji-answer-pack"]["details"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let found = detail["rank"]
                .as_i64()
                .is_some_and(|rank| rank > 0 && rank <= top_k);
            let cycles = if found { 1 } else { 2 };
            let case_calls = if one_call { 1 } else { cycles * 2 };
            let query = detail["query"].as_str().unwrap_or("");
            let path_context = detail["predicted_paths"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            let judge_prompt =
                JUDGE_INPUT_BASE + token_estimate(query) + token_estimate(&path_context);
            let (case_prompt, case_completion) = if one_call {
                (judge_prompt, ONE_CALL_JUDGE_OUTPUT_TOKENS)
            } else {
                (
                    cycles * (QUERY_WRITE_INPUT_BASE + token_estimate(query) + judge_prompt),
                    cycles * (QUERY_WRITE_OUTPUT_TOKENS + JUDGE_OUTPUT_TOKENS),
                )
            };
            cases += 1;
            hits += i64::from(found);
            calls += case_calls;
            prompt += case_prompt;
            completion += case_completion;
            seconds += detail["seconds"].as_f64().unwrap_or(0.0) + case_calls as f64 * latency;
        }
    }
    let denominator = cases.max(1) as f64;
    Ok(json!({
        "cases": cases, "hit_at_1_count": hits, "hit_at_10_count": hits,
        "hit_at_1": round4(hits as f64 / denominator), "hit_at_10": round4(hits as f64 / denominator),
        "llm_calls": calls, "avg_llm_calls": round4(calls as f64 / denominator),
        "prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion,
        "seconds": round3(seconds), "avg_seconds": round3(seconds / denominator),
        "estimated_cost": estimate_cost(prompt, completion, pricing)
    }))
}

fn token_estimate(text: &str) -> i64 {
    ((text.len() as f64 / 4.0).ceil() as i64).max(1)
}
fn round3(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}
fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_and_two_call_reports_reproduce_call_and_token_accounting() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("raw");
        let answer = dir.path().join("answer");
        fs::create_dir_all(&raw).unwrap();
        fs::create_dir_all(&answer).unwrap();
        fs::write(raw.join("p_raw_discover.json"), r#"{"modes":{"raw":{"metrics":{"cases":1,"hit_at_1":1,"hit_at_10":1}},"jikji-discover":{"metrics":{"cases":1,"hit_at_1":1,"hit_at_10":1}}}}"#).unwrap();
        fs::write(answer.join("p_jikji_answer_pack_report.json"), r#"{"modes":{"jikji-answer-pack":{"details":[{"rank":1,"query":"abc","predicted_paths":["x"],"seconds":0}]}}}"#).unwrap();
        let report =
            build_two_call_value_report(&raw, &answer, None, Pricing::default(), 20, 1.5).unwrap();
        assert_eq!(report["modes"]["jikji-one-call-judge"]["llm_calls"], 1);
        assert_eq!(report["modes"]["jikji-two-call-judge"]["llm_calls"], 2);
        assert_eq!(
            report["modes"]["jikji-one-call-judge"]["prompt_tokens"],
            262
        );
    }
}
