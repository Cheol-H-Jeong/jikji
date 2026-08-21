use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{DEFAULT_MAX_DOWNLOAD_BYTES, DatasetError, ResourceFetcher, Result, safe_join};

pub const BEIR_BASE_URL: &str =
    "https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets";

#[derive(Debug, Clone)]
pub struct BeirFetchOptions {
    pub dataset: String,
    pub destination: PathBuf,
    pub resource: Option<String>,
    pub max_download_bytes: u64,
    pub max_extracted_bytes: u64,
}

impl BeirFetchOptions {
    pub fn new(dataset: impl Into<String>, destination: impl Into<PathBuf>) -> Self {
        Self {
            dataset: dataset.into(),
            destination: destination.into(),
            resource: None,
            max_download_bytes: DEFAULT_MAX_DOWNLOAD_BYTES,
            max_extracted_bytes: 2 * DEFAULT_MAX_DOWNLOAD_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeirFetchResult {
    pub dataset: String,
    pub source_dir: PathBuf,
    pub archive_path: PathBuf,
    pub bytes_downloaded: u64,
    pub bytes_extracted: u64,
}

#[derive(Debug, Clone)]
pub struct BeirMaterializeOptions {
    pub dataset: String,
    pub destination: PathBuf,
    pub source_dir: Option<PathBuf>,
    pub split: String,
    pub max_cases: usize,
}

impl BeirMaterializeOptions {
    pub fn new(dataset: impl Into<String>, destination: impl Into<PathBuf>) -> Self {
        Self {
            dataset: dataset.into(),
            destination: destination.into(),
            source_dir: None,
            split: "test".into(),
            max_cases: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeirMaterializeResult {
    pub dataset: String,
    pub source_dir: PathBuf,
    pub corpus_root: PathBuf,
    pub eval_set_path: PathBuf,
    pub documents: usize,
    pub cases: usize,
    pub qrels: usize,
}

pub fn fetch_beir_dataset(
    fetcher: &impl ResourceFetcher,
    options: &BeirFetchOptions,
) -> Result<BeirFetchResult> {
    let dataset = normalize_dataset(&options.dataset)?;
    let source_parent = options.destination.join("source");
    let source_dir = source_parent.join(&dataset);
    let archive_path = source_parent.join(format!("{dataset}.zip"));
    if source_dir.join("corpus.jsonl").is_file() {
        return Ok(BeirFetchResult {
            dataset,
            source_dir,
            archive_path,
            bytes_downloaded: 0,
            bytes_extracted: 0,
        });
    }
    fs::create_dir_all(&source_parent)?;
    let resource = options
        .resource
        .clone()
        .unwrap_or_else(|| format!("{BEIR_BASE_URL}/{dataset}.zip"));
    let bytes_downloaded = if archive_path.exists() {
        0
    } else {
        fetcher.fetch_to(&resource, &archive_path, options.max_download_bytes)?
    };
    let temporary = source_parent.join(format!(".{dataset}.extracting"));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(&temporary)?;
    let bytes_extracted =
        match extract_zip_bounded(&archive_path, &temporary, options.max_extracted_bytes) {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = fs::remove_dir_all(&temporary);
                return Err(error);
            }
        };
    let expected = temporary.join(&dataset);
    let extracted = if expected.is_dir() {
        expected
    } else {
        let directories = fs::read_dir(&temporary)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        if directories.len() != 1 {
            return Err(DatasetError::Invalid(format!(
                "cannot locate BEIR dataset root in {}",
                archive_path.display()
            )));
        }
        directories[0].clone()
    };
    if source_dir.exists() {
        fs::remove_dir_all(&source_dir)?;
    }
    fs::rename(extracted, &source_dir)?;
    fs::remove_dir_all(&temporary)?;
    Ok(BeirFetchResult {
        dataset,
        source_dir,
        archive_path,
        bytes_downloaded,
        bytes_extracted,
    })
}

pub fn materialize_beir_dataset(options: &BeirMaterializeOptions) -> Result<BeirMaterializeResult> {
    let dataset = normalize_dataset(&options.dataset)?;
    let source_dir = options
        .source_dir
        .clone()
        .unwrap_or_else(|| options.destination.join("source").join(&dataset));
    let corpus_root = options.destination.join("corpora").join(&dataset);
    let eval_set_path = options
        .destination
        .join("eval")
        .join(format!("{}_{}.jsonl", dataset, options.split));
    if corpus_root.exists() {
        fs::remove_dir_all(&corpus_root)?;
    }
    let docs_dir = corpus_root.join("docs");
    fs::create_dir_all(&docs_dir)?;

    let mut doc_paths = BTreeMap::new();
    for row in read_jsonl(&source_dir.join("corpus.jsonl"))? {
        let id = string_field(&row, "_id");
        if id.is_empty() {
            continue;
        }
        let relative = format!("docs/{}", safe_doc_name(&id));
        let title = string_field(&row, "title").trim().to_owned();
        let text = string_field(&row, "text").trim().to_owned();
        let mut output = File::create(corpus_root.join(&relative))?;
        writeln!(
            output,
            "# {}\n\nBEIR dataset: {dataset}\nDocument ID: {id}\n\n{text}",
            if title.is_empty() { &id } else { &title }
        )?;
        doc_paths.insert(id, relative);
    }

    let queries = read_jsonl(&source_dir.join("queries.jsonl"))?
        .into_iter()
        .map(|row| (string_field(&row, "_id"), string_field(&row, "text")))
        .collect::<BTreeMap<_, _>>();
    let qrels_path = select_qrels(&source_dir, &options.split)?;
    let mut lines = BufReader::new(File::open(qrels_path)?).lines();
    let header = lines
        .next()
        .transpose()?
        .ok_or_else(|| DatasetError::Invalid("empty BEIR qrels".into()))?;
    let columns = header.split('\t').collect::<Vec<_>>();
    let qid_index = column_index(&columns, &["query-id", "query_id"])?;
    let cid_index = column_index(&columns, &["corpus-id", "corpus_id"])?;
    let score_index = column_index(&columns, &["score"])?;
    let mut by_query: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut qrels = 0;
    for line in lines {
        let line = line?;
        let fields = line.split('\t').collect::<Vec<_>>();
        let score = fields
            .get(score_index)
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        if score <= 0.0 {
            continue;
        }
        let qid = fields.get(qid_index).copied().unwrap_or_default();
        let cid = fields.get(cid_index).copied().unwrap_or_default();
        if queries.contains_key(qid) {
            if let Some(path) = doc_paths.get(cid) {
                by_query
                    .entry(qid.to_owned())
                    .or_default()
                    .insert(path.clone());
                qrels += 1;
            }
        }
    }

    if let Some(parent) = eval_set_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = File::create(&eval_set_path)?;
    let mut query_ids = by_query.keys().cloned().collect::<Vec<_>>();
    query_ids.sort_by(|left, right| natural_id(left).cmp(&natural_id(right)));
    let mut cases = 0;
    for qid in query_ids.into_iter().take(options.max_cases) {
        cases += 1;
        let expected = by_query[&qid].iter().cloned().collect::<Vec<_>>();
        let row = json!({
            "id": format!("beir-{dataset}-{}-{cases:04}", options.split),
            "scenario": format!("beir_{dataset}"),
            "query": queries[&qid],
            "expected_paths": expected,
            "expected_count": expected.len(),
            "source": "BEIR",
            "dataset": dataset,
            "split": options.split,
            "query_id": qid,
            "public_benchmark": true
        });
        serde_json::to_writer(&mut output, &row)?;
        output.write_all(b"\n")?;
    }
    Ok(BeirMaterializeResult {
        dataset,
        source_dir,
        corpus_root,
        eval_set_path,
        documents: doc_paths.len(),
        cases,
        qrels,
    })
}

fn extract_zip_bounded(archive: &Path, destination: &Path, max_bytes: u64) -> Result<u64> {
    let mut archive = zip::ZipArchive::new(File::open(archive)?)?;
    const MAX_ARCHIVE_ENTRIES: usize = 100_000;
    const MAX_ARCHIVE_DEPTH: usize = 32;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(DatasetError::Invalid(format!(
            "BEIR ZIP has too many entries: {} > {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        )));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| DatasetError::UnsafePath(entry.name().into()))?
            .to_owned();
        if enclosed.components().count() > MAX_ARCHIVE_DEPTH {
            return Err(DatasetError::UnsafePath(entry.name().into()));
        }
        let target = safe_join(destination, &enclosed)?;
        if entry.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        let entry_size = entry.size();
        total = total
            .checked_add(entry_size)
            .ok_or_else(|| DatasetError::ByteLimit {
                resource: "BEIR ZIP extraction".into(),
                limit: max_bytes,
            })?;
        if total > max_bytes {
            return Err(DatasetError::ByteLimit {
                resource: "BEIR ZIP extraction".into(),
                limit: max_bytes,
            });
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(target)?;
        std::io::copy(&mut entry.take(entry_size), &mut output)?;
    }
    Ok(total)
}

fn normalize_dataset(dataset: &str) -> Result<String> {
    let dataset = dataset.trim().to_ascii_lowercase();
    if dataset.is_empty()
        || !dataset
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(DatasetError::Invalid(format!(
            "unsafe BEIR dataset name: {dataset:?}"
        )));
    }
    Ok(dataset)
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)?;
        if value.is_object() {
            rows.push(value);
        }
    }
    Ok(rows)
}

fn safe_doc_name(id: &str) -> String {
    let mut name = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "_.-".contains(character) {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    name = name.trim_matches(['.', '_']).chars().take(180).collect();
    if name.is_empty() {
        name.push_str("doc");
    }
    name.push_str(".md");
    name
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn select_qrels(source: &Path, split: &str) -> Result<PathBuf> {
    let exact = source.join("qrels").join(format!("{split}.tsv"));
    if exact.is_file() {
        return Ok(exact);
    }
    let mut candidates = fs::read_dir(source.join("qrels"))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "tsv"))
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .pop()
        .ok_or_else(|| DatasetError::Invalid("no BEIR qrels found".into()))
}

fn column_index(columns: &[&str], names: &[&str]) -> Result<usize> {
    columns
        .iter()
        .position(|column| names.contains(column))
        .ok_or_else(|| DatasetError::Invalid(format!("missing qrels column: {}", names.join("/"))))
}

fn natural_id(value: &str) -> (bool, u128, &str) {
    match value.parse::<u128>() {
        Ok(number) => (false, number, ""),
        Err(_) => (true, 0, value),
    }
}
