#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use thiserror::Error;

const VISIBLE_MAPS: [&str; 2] = [".jikji_agent_map.md", "000_JIKJI_AGENT_MAP.md"];

#[derive(Debug, Error)]
pub enum HermesBenchError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSONL case at {path}:{line}: {source}")]
    JsonLine {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
    #[error("no Hermes benchmark cases found: {0}")]
    NoCases(PathBuf),
    #[error("benchmark root is not a directory: {0}")]
    InvalidRoot(PathBuf),
    #[error(
        "eval set or report is inside the benchmark root; set allow_leak only for deliberate tests"
    )]
    LeakRoot,
    #[error("failed to serialize report: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T, E = HermesBenchError> = std::result::Result<T, E>;
#[derive(Debug, Clone)]
pub struct HermesBenchOptions {
    pub root: PathBuf,
    pub eval_set: PathBuf,
    pub out: PathBuf,
    pub modes: Vec<String>,
    pub cases_limit: Option<usize>,
    pub hermes_bin: PathBuf,
    pub model: String,
    pub provider: String,
    pub timeout: Duration,
    pub max_turns: u32,
    pub fast_max_turns: u32,
    pub skills: String,
    pub candidate_top_k: usize,
    pub retries: usize,
    pub allow_leak: bool,
    pub yolo: bool,
    pub hermes_home: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct HermesBenchResult {
    pub report_path: PathBuf,
    pub metrics: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsage {
    pub llm_calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
}

impl SessionUsage {
    fn add(&mut self, other: Self) {
        self.llm_calls += other.llm_calls;
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Deserialize)]
struct EvalCase {
    #[serde(default)]
    id: Value,
    #[serde(default)]
    scenario: Value,
    #[serde(default)]
    query: Value,
    #[serde(default)]
    expected_paths: Vec<Value>,
}

#[derive(Debug)]
struct ProcessOutput {
    returncode: i32,
    timeout: bool,
    stdout: String,
    stderr: String,
    seconds: f64,
}

pub fn recover_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value @ Value::Object(_)) = serde_json::from_str(trimmed) {
        return Some(value);
    }
    let mut starts = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' if depth > 0 => in_string = true,
            '{' => {
                if depth == 0 {
                    starts.push(idx);
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let start = starts.pop().unwrap_or(idx);
                    let end = idx + ch.len_utf8();
                    if let Ok(value @ Value::Object(_)) = serde_json::from_str(&text[start..end]) {
                        return Some(value);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

pub fn normalise_paths(data: &Value) -> Vec<String> {
    let Some(object) = data.as_object() else {
        return Vec::new();
    };
    let raw = object
        .get("paths")
        .or_else(|| object.get("path"))
        .or_else(|| object.get("predicted_paths"));
    let values: Vec<&Value> = match raw {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for value in values {
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        let clean = text.trim().trim_matches(['`', '\'']).to_owned();
        if !clean.is_empty() && seen.insert(clean.clone()) {
            out.push(clean);
        }
    }
    out
}

pub fn extract_session_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for suffix in text.split("session_id:").skip(1) {
        let id: String = suffix
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .collect();
        if !id.is_empty() && seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

pub fn read_session_usage(home: &Path, session_id: &str) -> SessionUsage {
    if session_id.is_empty() {
        return SessionUsage::default();
    }
    let mut usage = SessionUsage::default();
    let db = home.join("state.db");
    if db.is_file() {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
        if let Ok(connection) = Connection::open_with_flags(&db, flags) {
            let row = connection.query_row(
                "SELECT input_tokens, output_tokens, reasoning_tokens, message_count, tool_call_count FROM sessions WHERE id=?1",
                [session_id],
                |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?.unwrap_or(0), row.get::<_, Option<i64>>(2)?.unwrap_or(0), row.get::<_, Option<i64>>(3)?.unwrap_or(0), row.get::<_, Option<i64>>(4)?.unwrap_or(0))),
            );
            if let Ok((prompt, completion, reasoning, messages, tools)) = row {
                usage.prompt_tokens = prompt;
                usage.completion_tokens = completion;
                usage.reasoning_tokens = reasoning;
                usage.total_tokens = prompt + completion + reasoning;
                usage.llm_calls = (tools + 1).max(messages - tools - 1).max(1);
            }
        }
    }
    for path in [
        home.join("sessions")
            .join(format!("session_{session_id}.json")),
        home.join("sessions").join(format!("{session_id}.json")),
    ] {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(messages) = value.get("messages").and_then(Value::as_array) {
            let calls = messages
                .iter()
                .filter(|item| item.get("role").and_then(Value::as_str) == Some("assistant"))
                .count() as i64;
            if calls > 0 {
                usage.llm_calls = calls;
            }
            break;
        }
    }
    usage
}

pub fn corpus_mutations(
    root: &Path,
    before: &BTreeMap<String, (u64, u128)>,
) -> Result<Vec<String>> {
    let after = inventory(root)?;
    Ok(before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .take(50)
        .cloned()
        .collect())
}

pub fn corpus_inventory(root: &Path) -> Result<BTreeMap<String, (u64, u128)>> {
    inventory(root)
}

pub fn run_hermes_benchmark(options: &HermesBenchOptions) -> Result<HermesBenchResult> {
    let root = canonical(&options.root)?;
    if !root.is_dir() {
        return Err(HermesBenchError::InvalidRoot(root));
    }
    let eval_set = canonical(&options.eval_set)?;
    let out = canonical_destination(&options.out)?;
    let eval_is_owned = is_jikji_owned_path(&root, &eval_set);
    let out_is_owned = is_jikji_owned_path(&root, &out);
    if !options.allow_leak
        && ((eval_set.starts_with(&root) && !eval_is_owned)
            || (out.starts_with(&root) && !out_is_owned))
    {
        return Err(HermesBenchError::LeakRoot);
    }
    let mut cases = read_cases(&eval_set)?;
    if let Some(limit) = options.cases_limit {
        cases.truncate(limit);
    }
    if cases.is_empty() {
        return Err(HermesBenchError::NoCases(eval_set));
    }
    let evidence_dir = out.with_extension("");
    create_dir_all(&evidence_dir)?;
    let home = options
        .hermes_home
        .clone()
        .or_else(|| std::env::var_os("HERMES_HOME").map(PathBuf::from))
        .unwrap_or_else(default_hermes_home);
    let modes = if options.modes.is_empty() {
        vec!["raw".to_owned(), "jikji".to_owned()]
    } else {
        options.modes.clone()
    };
    let mut report_modes = Map::new();
    let mut result_metrics = BTreeMap::new();
    for mode in modes {
        let mode_started = Instant::now();
        let family = mode_family(&mode);
        let mut details = Vec::new();
        for (idx, case) in cases.iter().enumerate() {
            let case_started = Instant::now();
            let before = inventory(&root)?;
            let max_attempts = options.retries.saturating_add(1).max(1);
            let turns = if family == "jikji-one-shot" {
                1
            } else if family == "jikji-fast" {
                options.fast_max_turns
            } else {
                options.max_turns
            };
            let mut attempts = Vec::new();
            let mut predicted = Vec::new();
            let mut returncode = 0;
            let mut timed_out = false;
            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut session_ids = Vec::new();
            let mut usage = SessionUsage::default();
            for attempt_idx in 0..max_attempts {
                let prompt =
                    build_prompt(&root, &mode, case, attempt_idx > 0, options.candidate_top_k)?;
                let output = run_process(options, &root, turns, &prompt)?;
                let parsed = recover_json_object(if output.stdout.is_empty() {
                    &output.stderr
                } else {
                    &output.stdout
                })
                .unwrap_or_else(|| json!({}));
                predicted = normalise_paths(&parsed);
                let found_ids = extract_session_ids(&output.stdout)
                    .into_iter()
                    .chain(extract_session_ids(&output.stderr))
                    .collect::<Vec<_>>();
                let mut attempt_usage = SessionUsage::default();
                for id in &found_ids {
                    if !session_ids.contains(id) {
                        session_ids.push(id.clone());
                        attempt_usage.add(read_session_usage(&home, id));
                    }
                }
                usage.add(attempt_usage);
                returncode = output.returncode;
                timed_out |= output.timeout;
                stdout = output.stdout;
                stderr = output.stderr;
                attempts.push(json!({
                    "attempt": attempt_idx + 1, "returncode": returncode, "timeout": output.timeout,
                    "seconds": round(output.seconds, 3), "predicted_paths": predicted,
                    "stdout_tail": tail_chars(&stdout, 800), "session_ids": found_ids, "usage": attempt_usage,
                }));
                if !predicted.is_empty() || returncode == -1 {
                    break;
                }
            }
            let mutated_paths = corpus_mutations(&root, &before)?;
            let expected: BTreeSet<String> = case.expected_paths.iter().map(value_text).collect();
            let rank = if mutated_paths.is_empty() {
                predicted
                    .iter()
                    .position(|path| expected.contains(path))
                    .map(|pos| pos + 1)
            } else {
                None
            };
            let usage_status = if session_ids.is_empty() {
                "missing_session_ids"
            } else if usage.total_tokens <= 0 {
                "missing_usage"
            } else {
                "ok"
            };
            let evidence_path = evidence_dir.join(format!(
                "{}_{:04}_{}.txt",
                safe_component(&mode),
                idx + 1,
                safe_component(&value_text(&case.id))
            ));
            let evidence = attempts
                .iter()
                .map(|item| {
                    format!(
                        "=== attempt {} rc={} timeout={} ===\n{}",
                        item["attempt"],
                        item["returncode"],
                        item["timeout"],
                        item["stdout_tail"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
                + if stderr.is_empty() { "" } else { "\nSTDERR:\n" }
                + &stderr;
            write_atomic(&evidence_path, evidence.as_bytes())?;
            details.push(json!({
                "id": case.id, "scenario": case.scenario, "query": case.query,
                "expected_count": expected.len(), "expected_paths": expected, "predicted_paths": predicted,
                "rank": rank, "hash_rank": Value::Null, "duplicate_rank": rank, "hit": rank.is_some(),
                "returncode": returncode, "timeout": timed_out, "mutated_paths": mutated_paths,
                "attempts": attempts, "mode_family": family, "candidate_top_k": if family.starts_with("jikji") { options.candidate_top_k } else { 0 },
                "max_turns": turns, "agent_chat_turns": turns, "seconds": round(case_started.elapsed().as_secs_f64(), 3),
                "output_path": evidence_path, "stdout_tail": tail_chars(&stdout, 1200), "session_ids": session_ids,
                "usage_status": usage_status, "usage": usage, "llm_calls": usage.llm_calls,
                "prompt_tokens": usage.prompt_tokens, "completion_tokens": usage.completion_tokens,
            }));
        }
        let metric = metrics(&details, mode_started.elapsed().as_secs_f64());
        result_metrics.insert(mode.clone(), metric.clone());
        report_modes.insert(mode, json!({"metrics": metric, "details": details}));
    }
    let report = json!({
        "root": root, "eval_set": eval_set, "hermes_bin": options.hermes_bin,
        "model": options.model, "provider": options.provider, "modes": report_modes, "no_leak": !options.allow_leak,
    });
    let mut encoded = serde_json::to_vec_pretty(&report)?;
    encoded.push(b'\n');
    write_atomic(&out, &encoded)?;
    Ok(HermesBenchResult {
        report_path: out,
        metrics: result_metrics,
    })
}

const MAX_PROCESS_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

fn run_process(
    options: &HermesBenchOptions,
    root: &Path,
    turns: u32,
    prompt: &str,
) -> Result<ProcessOutput> {
    let mut command = Command::new(&options.hermes_bin);
    command.args(["chat", "-Q", "--max-turns", &turns.to_string()]);
    if !options.model.is_empty() {
        command.args(["-m", &options.model]);
    }
    if !options.provider.is_empty() {
        command.args(["--provider", &options.provider]);
    }
    if options.yolo {
        command.args(["--yolo", "--accept-hooks"]);
    }
    if !options.skills.is_empty() {
        command.args(["--skills", &options.skills]);
    }
    command
        .args(["-q", prompt])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProcessOutput {
                returncode: -1,
                timeout: false,
                stdout: String::new(),
                stderr: error.to_string(),
                seconds: started.elapsed().as_secs_f64(),
            });
        }
        Err(source) => {
            return Err(HermesBenchError::Io {
                path: options.hermes_bin.clone(),
                source,
            });
        }
    };
    let stdout = child.stdout.take().map(|pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe
                .take(MAX_PROCESS_OUTPUT_BYTES + 1)
                .read_to_end(&mut bytes);
            bytes.truncate(MAX_PROCESS_OUTPUT_BYTES as usize);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    });
    let stderr = child.stderr.take().map(|pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe
                .take(MAX_PROCESS_OUTPUT_BYTES + 1)
                .read_to_end(&mut bytes);
            bytes.truncate(MAX_PROCESS_OUTPUT_BYTES as usize);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    });
    let mut timeout = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < options.timeout => {
                thread::sleep(Duration::from_millis(10))
            }
            Ok(None) => {
                timeout = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Err(source) => {
                return Err(HermesBenchError::Io {
                    path: options.hermes_bin.clone(),
                    source,
                });
            }
        }
    };
    let (stdout, stderr) = if timeout {
        drop(stdout);
        drop(stderr);
        (String::new(), String::new())
    } else {
        (
            stdout
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default(),
            stderr
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default(),
        )
    };
    Ok(ProcessOutput {
        returncode: if timeout {
            -1
        } else {
            status.and_then(|value| value.code()).unwrap_or(-1)
        },
        timeout,
        stdout,
        stderr,
        seconds: started.elapsed().as_secs_f64(),
    })
}

fn metrics(details: &[Value], seconds: f64) -> Value {
    let total = details.len();
    let rank = |item: &Value| item.get("rank").and_then(Value::as_u64);
    let hits = details
        .iter()
        .filter(|item| item.get("hit").and_then(Value::as_bool) == Some(true))
        .count();
    let at = |limit| {
        details
            .iter()
            .filter(|item| rank(item).is_some_and(|value| value <= limit))
            .count()
    };
    let mut usage = SessionUsage::default();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut scenarios = BTreeMap::<String, Vec<&Value>>::new();
    let mut calls = Vec::new();
    for detail in details {
        if let Ok(value) = serde_json::from_value::<SessionUsage>(detail["usage"].clone()) {
            usage.add(value);
        }
        let status = detail["usage_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        *statuses.entry(status).or_default() += 1;
        scenarios
            .entry(value_text(&detail["scenario"]))
            .or_default()
            .push(detail);
        calls.push(detail["llm_calls"].as_i64().unwrap_or(0));
    }
    calls.sort_unstable();
    let usage_status = if statuses.len() == 1 && statuses.contains_key("ok") {
        "ok"
    } else if statuses.keys().any(|key| key.starts_with("missing")) {
        "missing_usage"
    } else {
        "unknown"
    };
    let by_scenario = scenarios.into_iter().map(|(name, items)| {
        let count = items.len();
        let scenario_hits = items.iter().filter(|item| item["hit"].as_bool() == Some(true)).count();
        (name, json!({"cases": count, "accuracy": ratio(scenario_hits, count), "hit_at_3": ratio(items.iter().filter(|item| rank(item).is_some_and(|v| v <= 3)).count(), count), "hit_at_5": ratio(items.iter().filter(|item| rank(item).is_some_and(|v| v <= 5)).count(), count), "hit_at_10": ratio(items.iter().filter(|item| rank(item).is_some_and(|v| v <= 10)).count(), count)}))
    }).collect::<Map<_, _>>();
    json!({
        "cases": total, "accuracy": ratio(hits, total), "hit_at_1": ratio(at(1), total), "hit_at_3": ratio(at(3), total), "hit_at_5": ratio(at(5), total), "hit_at_10": ratio(at(10), total),
        "duplicate_or_exact_hit_at_10": ratio(at(10), total), "seconds": round(seconds, 3), "avg_seconds": if total == 0 { 0.0 } else { round(seconds / total as f64, 3) },
        "llm_calls": usage.llm_calls, "prompt_tokens": usage.prompt_tokens, "completion_tokens": usage.completion_tokens, "reasoning_tokens": usage.reasoning_tokens, "total_tokens": usage.total_tokens,
        "usage_status": usage_status, "usage_status_counts": statuses, "avg_llm_calls": if total == 0 { 0.0 } else { round(usage.llm_calls as f64 / total as f64, 3) },
        "median_llm_calls": percentile(&calls, 0.50), "p90_llm_calls": percentile(&calls, 0.90), "p95_llm_calls": percentile(&calls, 0.95), "max_llm_calls": calls.last().copied().unwrap_or(0),
        "avg_prompt_tokens": if total == 0 { 0.0 } else { round(usage.prompt_tokens as f64 / total as f64, 1) }, "avg_completion_tokens": if total == 0 { 0.0 } else { round(usage.completion_tokens as f64 / total as f64, 1) }, "avg_total_tokens": if total == 0 { 0.0 } else { round(usage.total_tokens as f64 / total as f64, 1) }, "by_scenario": by_scenario,
    })
}

fn inventory(root: &Path) -> Result<BTreeMap<String, (u64, u128)>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, (u64, u128)>) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(|source| HermesBenchError::Io {
            path: dir.to_owned(),
            source,
        })? {
            let entry = entry.map_err(|source| HermesBenchError::Io {
                path: dir.to_owned(),
                source,
            })?;
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if relative
                .components()
                .next()
                .is_some_and(|part| part.as_os_str() == ".jikji")
            {
                continue;
            }
            if relative.components().count() == 1
                && VISIBLE_MAPS
                    .iter()
                    .any(|name| relative.as_os_str() == *name)
            {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                visit(root, &path, out)?;
            } else if metadata.is_file() {
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos())
                    .unwrap_or(0);
                out.insert(
                    relative.to_string_lossy().replace('\\', "/"),
                    (metadata.len(), modified),
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out)?;
    Ok(out)
}

fn read_cases(path: &Path) -> Result<Vec<EvalCase>> {
    let file = File::open(path).map_err(|source| HermesBenchError::Io {
        path: path.to_owned(),
        source,
    })?;
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| match line {
            Ok(line) if line.trim().is_empty() => None,
            Ok(line) => {
                Some(
                    serde_json::from_str(&line).map_err(|source| HermesBenchError::JsonLine {
                        path: path.to_owned(),
                        line: idx + 1,
                        source,
                    }),
                )
            }
            Err(source) => Some(Err(HermesBenchError::Io {
                path: path.to_owned(),
                source,
            })),
        })
        .collect()
}

fn build_prompt(
    root: &Path,
    mode: &str,
    case: &EvalCase,
    retry: bool,
    top_k: usize,
) -> Result<String> {
    let query = value_text(&case.query).replace(['\0', '\r'], " ");
    let family = mode_family(mode);
    let strategy = if family.starts_with("jikji") {
        let payload = jikji_search::discover(
            root,
            &query,
            jikji_search::DiscoverOptions {
                top_k,
                retry_exhausted: retry,
                retry_proof: String::new(),
            },
        )
        .map_err(|error| HermesBenchError::Io {
            path: root.to_owned(),
            source: std::io::Error::other(error.to_string()),
        })?;
        format!(
            "Use this Jikji discovery payload as the only candidate source; preserve answer_paths/candidates order and do not perform broad filesystem discovery:\n{}",
            serde_json::to_string(&payload)?,
        )
    } else {
        "Use raw filesystem discovery only. Do not read .jikji or .jikji_agent_map.md, and do not invoke Jikji commands.".to_owned()
    };
    Ok(format!(
        "You are running a local-file retrieval benchmark. Return JSON only: {{\"paths\":[\"relative/path\"],\"reason\":\"brief\"}}.\nRoot: {}\nMode: {}\nQuery: {}\nCandidate top-k: {}\nRetry: {}\n{}\nDo not modify, create, rename, or delete corpus files.",
        root.display(),
        mode,
        query,
        top_k,
        retry,
        strategy,
    ))
}

fn mode_family(mode: &str) -> String {
    let normalized = mode.trim().to_lowercase().replace('_', "-");
    match normalized.as_str() {
        "map-first" | "fast" => "jikji-fast".to_owned(),
        "one-shot" => "jikji-one-shot".to_owned(),
        _ => normalized,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&temp, bytes).map_err(|source| HermesBenchError::Io {
        path: temp.clone(),
        source,
    })?;
    fs::rename(&temp, path).map_err(|source| HermesBenchError::Io {
        path: path.to_owned(),
        source,
    })
}

fn is_jikji_owned_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .is_some_and(|relative| relative.starts_with(".jikji"))
}

fn canonical_destination(path: &Path) -> Result<PathBuf> {
    let absolute = absolute_path(path)?;
    let parent = absolute.parent().unwrap_or_else(|| Path::new("."));
    create_dir_all(parent)?;
    let parent = canonical(parent)?;
    let name = absolute.file_name().ok_or_else(|| HermesBenchError::Io {
        path: absolute.clone(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "output path has no file name",
        ),
    })?;
    Ok(parent.join(name))
}

fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| HermesBenchError::Io {
        path: path.to_owned(),
        source,
    })
}
fn canonical(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|source| HermesBenchError::Io {
        path: path.to_owned(),
        source,
    })
}
fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()
            .map_err(|source| HermesBenchError::Io {
                path: path.to_owned(),
                source,
            })?
            .join(path))
    }
}
fn default_hermes_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hermes")
}
fn value_text(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string()
        }
    })
}
fn safe_component(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let clean = value.trim_matches(['.', '_']);
    if clean.is_empty() {
        "case".to_owned()
    } else {
        clean.chars().take(70).collect()
    }
}
fn tail_chars(value: &str, count: usize) -> String {
    let start = value
        .char_indices()
        .rev()
        .nth(count.saturating_sub(1))
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    value[start..].to_owned()
}
fn round(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (value * factor).round() / factor
}
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        round(numerator as f64 / denominator as f64, 4)
    }
}
fn percentile(values: &[i64], pct: f64) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let index = (((values.len() - 1) as f64 * pct).round() as usize).min(values.len() - 1);
    values[index]
}
