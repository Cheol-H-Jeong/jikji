use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{DEFAULT_MAX_DOWNLOAD_BYTES, DatasetError, ResourceFetcher, Result, safe_join};

pub const HF_REPO: &str = "MMMem-org/HippoCamp";
pub const HF_API_TREE_BASE: &str =
    "https://huggingface.co/api/datasets/MMMem-org/HippoCamp/tree/main";
pub const HF_RESOLVE: &str = "https://huggingface.co/datasets/MMMem-org/HippoCamp/resolve/main/";

#[derive(Debug, Clone)]
pub struct HippoCampFetchOptions {
    pub destination: PathBuf,
    pub profile: String,
    pub split: String,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub annotation_resource: Option<String>,
    pub tree_resource: Option<String>,
}

impl HippoCampFetchOptions {
    pub fn new(destination: impl Into<PathBuf>) -> Self {
        Self {
            destination: destination.into(),
            profile: "Adam".into(),
            split: "Subset".into(),
            max_files: 120,
            max_file_bytes: 10 * 1024 * 1024,
            max_total_bytes: 250 * 1024 * 1024,
            annotation_resource: None,
            tree_resource: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HippoCampFetchResult {
    pub root: PathBuf,
    pub annotation_path: PathBuf,
    pub files_downloaded: usize,
    pub bytes_downloaded: u64,
    pub skipped: usize,
}

#[derive(Debug, Clone)]
pub struct HippoCampImportOptions {
    pub root: PathBuf,
    pub annotation: Option<PathBuf>,
    pub max_cases: usize,
    pub output: Option<PathBuf>,
}

impl HippoCampImportOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            annotation: None,
            max_cases: 200,
            output: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HippoCampImportResult {
    pub eval_set_path: PathBuf,
    pub cases: usize,
    pub scenarios: BTreeMap<String, usize>,
    pub skipped_cases: usize,
}

#[derive(Debug, Deserialize)]
struct TreeItem {
    #[serde(rename = "type")]
    kind: Option<String>,
    path: Option<String>,
    size: Option<u64>,
}

pub fn fetch_subset(
    fetcher: &impl ResourceFetcher,
    options: &HippoCampFetchOptions,
) -> Result<HippoCampFetchResult> {
    let profile = clean_segment(&options.profile)?;
    let split = clean_segment(&options.split)?;
    let inner = if split.eq_ignore_ascii_case("subset") {
        format!("{profile}_{split}")
    } else {
        profile.clone()
    };
    let prefix = format!("{profile}/{split}/{inner}/");
    let annotation_remote = format!("{profile}/{split}/{inner}.json");
    let root = options.destination.join(&inner);
    fs::create_dir_all(&root)?;
    let annotation_path = options.destination.join(format!("{inner}.annotation.json"));
    let annotation_resource = options
        .annotation_resource
        .clone()
        .unwrap_or_else(|| format!("{HF_RESOLVE}{annotation_remote}"));
    let mut total = if annotation_path.exists() {
        annotation_path.metadata()?.len()
    } else {
        fetcher.fetch_to(
            &annotation_resource,
            &annotation_path,
            options.max_file_bytes.max(1),
        )?
    };
    let tree_resource = options.tree_resource.clone().unwrap_or_else(|| {
        format!("{HF_API_TREE_BASE}/{profile}/{split}/{inner}?recursive=true&expand=true")
    });
    let tree_path = options.destination.join(format!(".{inner}.tree.json"));
    fetcher.fetch_to(&tree_resource, &tree_path, DEFAULT_MAX_DOWNLOAD_BYTES)?;
    let tree_bytes = fs::read_to_string(&tree_path)?;
    let tree: Vec<TreeItem> = serde_json::from_str(&tree_bytes)?;
    let mut count = 0;
    let mut skipped = 0;
    for item in tree {
        if item.kind.as_deref() != Some("file") {
            continue;
        }
        let remote = item.path.unwrap_or_default();
        if !remote.starts_with(&prefix) {
            continue;
        }
        let size = item.size.unwrap_or(0);
        let rel = &remote[prefix.len()..];
        if rel.is_empty() || rel.ends_with(".json") {
            continue;
        }
        if count >= options.max_files
            || (options.max_file_bytes > 0 && size > options.max_file_bytes)
            || (options.max_total_bytes > 0 && total.saturating_add(size) > options.max_total_bytes)
        {
            skipped += 1;
            continue;
        }
        let target = safe_join(&root, Path::new(rel))?;
        if target.is_file() && target.metadata()?.len() == size {
            count += 1;
            total += size;
            continue;
        }
        let resource = format!("{HF_RESOLVE}{remote}");
        let got = fetcher.fetch_to(&resource, &target, options.max_file_bytes.max(1))?;
        total += got;
        count += 1;
    }
    let _ = fs::remove_file(options.destination.join(format!(".{inner}.tree.json")));
    Ok(HippoCampFetchResult {
        root,
        annotation_path,
        files_downloaded: count,
        bytes_downloaded: total,
        skipped,
    })
}

pub fn import_eval_set(options: &HippoCampImportOptions) -> Result<HippoCampImportResult> {
    let root = options.root.canonicalize()?;
    let annotation = match &options.annotation {
        Some(path) => path.clone(),
        None => {
            let mut matches = fs::read_dir(&root)?
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .collect::<Vec<_>>();
            matches.sort();
            matches.into_iter().next().ok_or_else(|| {
                DatasetError::Invalid("No HippoCamp annotation JSON found; pass annotation".into())
            })?
        }
    };
    let data: Value = serde_json::from_str(&fs::read_to_string(&annotation)?)?;
    let rows = data
        .as_array()
        .ok_or_else(|| DatasetError::Invalid("HippoCamp annotation JSON must be a list".into()))?;
    let mut cases = Vec::new();
    let mut scenarios = BTreeMap::new();
    let mut skipped_cases = 0;
    for row in rows {
        if cases.len() >= options.max_cases {
            break;
        }
        let Some(row) = row.as_object() else {
            skipped_cases += 1;
            continue;
        };
        let query = row
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let paths = row
            .get("file_path")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        let existing = paths
            .into_iter()
            .filter(|path| {
                safe_join(&root, Path::new(path)).is_ok_and(|candidate| candidate.is_file())
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if query.is_empty() || existing.is_empty() {
            skipped_cases += 1;
            continue;
        }
        let label = row
            .get("QA_type")
            .or_else(|| row.get("profiling_type"))
            .and_then(Value::as_str)
            .unwrap_or("qa");
        let scenario = format!(
            "hippocamp_{}",
            label.trim().to_ascii_lowercase().replace(' ', "_")
        );
        let number = scenarios.entry(scenario.clone()).or_insert(0);
        *number += 1;
        let evidence = row
            .get("evidence")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_object)
            .and_then(|item| item.get("evidence_text"))
            .and_then(Value::as_str)
            .or_else(|| row.get("evidence_text_joined").and_then(Value::as_str))
            .or_else(|| row.get("gold_text").and_then(Value::as_str))
            .or_else(|| row.get("answer").and_then(Value::as_str))
            .unwrap_or("");
        cases.push(json!({"id": format!("{}-{:04}", scenario, number), "scenario": scenario, "query": query, "expected_paths": existing, "evidence": evidence.chars().take(1000).collect::<String>(), "answer": row.get("answer").and_then(Value::as_str).unwrap_or("").chars().take(2000).collect::<String>(), "source": "HippoCamp"}));
    }
    let output = options.output.clone().unwrap_or_else(|| {
        root.parent().unwrap_or(&root).join(format!(
            "{}_hippocamp_eval_set.jsonl",
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("dataset")
        ))
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(&output)?;
    for row in &cases {
        serde_json::to_writer(&mut file, row)?;
        file.write_all(b"\n")?;
    }
    let report = output.with_file_name("hippocamp_import_report.json");
    fs::write(
        report,
        serde_json::to_vec_pretty(
            &json!({"root": root, "annotation": annotation, "cases": cases.len(), "skipped_cases": skipped_cases, "scenarios": scenarios}),
        )?,
    )?;
    Ok(HippoCampImportResult {
        eval_set_path: output,
        cases: cases.len(),
        scenarios,
        skipped_cases,
    })
}

fn clean_segment(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(DatasetError::Invalid(format!(
            "unsafe profile/split: {value:?}"
        )));
    }
    Ok(value.to_owned())
}
