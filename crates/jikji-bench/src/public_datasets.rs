use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Component, Path, PathBuf};

use jikji_core::{Result, io_error, json_error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const EDITH_SOURCE: &str = "lightonai/veracier-industries";
const WORKSPACEBENCH_SOURCE: &str = "Workspace-Bench/Workspace-Bench-Lite";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteInput {
    pub corpus_root: PathBuf,
    pub eval_set: PathBuf,
    pub cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdithMasterRow {
    pub filename: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdithAnswer {
    pub question: String,
    #[serde(default)]
    pub ground_truth: Value,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub difficulty_factors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdithFixture {
    pub master_index: Vec<EdithMasterRow>,
    pub answers: BTreeMap<String, EdithAnswer>,
    pub documents: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdithOptions {
    pub max_cases: usize,
    pub max_docs: usize,
}

impl Default for EdithOptions {
    fn default() -> Self {
        Self {
            max_cases: 8,
            max_docs: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdithBuildResult {
    pub dest: PathBuf,
    pub metadata_dir: PathBuf,
    pub corpus_root: PathBuf,
    pub eval_set_path: PathBuf,
    pub manifest_path: PathBuf,
    pub selected_questions: usize,
    pub selected_docs: usize,
    pub extracted_docs: usize,
    pub skipped_questions: usize,
}

impl EdithBuildResult {
    pub fn suite_input(&self) -> SuiteInput {
        SuiteInput {
            corpus_root: self.corpus_root.clone(),
            eval_set: self.eval_set_path.clone(),
            cases: self.selected_questions,
        }
    }
}

pub fn materialize_edith_fixture(
    dest: &Path,
    fixture: &EdithFixture,
    options: EdithOptions,
) -> Result<EdithBuildResult> {
    let dest = absolute_path(dest)?;
    let metadata_dir = dest.join("metadata");
    let corpus_root = dest.join("corpus");
    let eval_set_path = dest.join("eval/edith_eval.jsonl");
    reset_owned_dirs(&dest, &["metadata", "corpus", "eval"])?;

    write_json(
        &metadata_dir.join("MASTER_INDEX.json"),
        &serde_json::to_value(&fixture.master_index)
            .map_err(|source| json_error(metadata_dir.join("MASTER_INDEX.json"), source))?,
    )?;
    write_json(
        &metadata_dir.join("ANSWER_KEY.json"),
        &serde_json::to_value(&fixture.answers)
            .map_err(|source| json_error(metadata_dir.join("ANSWER_KEY.json"), source))?,
    )?;

    let master = fixture
        .master_index
        .iter()
        .map(|row| normalize_rel(&row.filename))
        .collect::<Result<BTreeSet<_>>>()?;
    let mut selected = BTreeSet::new();
    let mut candidate_rows = Vec::new();
    let mut skipped = 0usize;
    for (question_id, answer) in &fixture.answers {
        if candidate_rows.len() >= options.max_cases {
            break;
        }
        let docs = flatten_pdf_paths(&answer.ground_truth)?
            .into_iter()
            .map(|path| resolve_master_path(&master, &path))
            .collect::<Result<Vec<_>>>()?;
        if docs.is_empty() {
            skipped += 1;
            continue;
        }
        let remaining = options.max_docs.saturating_sub(selected.len());
        if remaining == 0 {
            break;
        }
        let bounded = docs.into_iter().take(remaining).collect::<Vec<_>>();
        selected.extend(bounded.iter().cloned());
        candidate_rows.push((question_id, answer, bounded));
    }

    let mut extracted = BTreeMap::new();
    for source_path in &selected {
        let Some(bytes) = fixture.documents.get(source_path) else {
            continue;
        };
        let relative = safe_relative_path(source_path)?;
        let target = corpus_root.join(&relative);
        write_bytes(&target, bytes)?;
        extracted.insert(
            source_path.clone(),
            relative.to_string_lossy().replace('\\', "/"),
        );
    }

    let mut eval_rows = Vec::new();
    for (question_id, answer, docs) in candidate_rows {
        let expected = docs
            .iter()
            .filter_map(|path| extracted.get(path).cloned())
            .collect::<Vec<_>>();
        if expected.is_empty() {
            skipped += 1;
            continue;
        }
        let missing = docs
            .iter()
            .filter(|path| !extracted.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();
        eval_rows.push(json!({
            "id": format!("edith-{question_id}"),
            "scenario": "edith_enterprise_pdf",
            "query": if answer.question.is_empty() { question_id } else { &answer.question },
            "expected_paths": expected,
            "expected_count": expected.len(),
            "expected_source_paths": docs,
            "dropped_expected_source_paths": missing,
            "role": answer.role,
            "entity": answer.entity,
            "difficulty_factors": answer.difficulty_factors,
            "source": "EDiTh / Véracier Industries",
            "dataset": EDITH_SOURCE,
            "question_id": question_id,
            "public_benchmark": true,
        }));
    }
    write_jsonl_values(&eval_set_path, &eval_rows)?;
    let manifest_path = dest.join("edith_manifest.json");
    write_json(
        &manifest_path,
        &json!({
            "public_benchmark": true,
            "source": EDITH_SOURCE,
            "metadata_dir": metadata_dir,
            "corpus_root": corpus_root,
            "eval_set": eval_set_path,
            "selected_questions": eval_rows.len(),
            "candidate_questions": eval_rows.len() + skipped,
            "selected_docs": selected.len(),
            "extracted_docs": extracted.len(),
            "skipped_questions": skipped,
            "network": "fixture",
        }),
    )?;
    Ok(EdithBuildResult {
        dest,
        metadata_dir,
        corpus_root,
        eval_set_path,
        manifest_path,
        selected_questions: eval_rows.len(),
        selected_docs: selected.len(),
        extracted_docs: extracted.len(),
        skipped_questions: skipped,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicDataDocument {
    pub id: String,
    pub filename: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub xlsx_text: Vec<String>,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub license_note: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicDataOptions {
    pub max_cases_per_split: usize,
    pub seed: u64,
}

impl Default for PublicDataOptions {
    fn default() -> Self {
        Self {
            max_cases_per_split: 40,
            seed: 20_260_529,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SplitDatasetBuildResult {
    pub dest: PathBuf,
    pub train_root: PathBuf,
    pub valid_root: PathBuf,
    pub test_root: PathBuf,
    pub train_eval_set_path: PathBuf,
    pub valid_eval_set_path: PathBuf,
    pub eval_set_path: PathBuf,
    pub manifest_path: PathBuf,
    pub docs: usize,
    pub train_docs: usize,
    pub valid_docs: usize,
    pub test_docs: usize,
    pub eval_cases: usize,
}

impl SplitDatasetBuildResult {
    pub fn suite_input(&self) -> SuiteInput {
        SuiteInput {
            corpus_root: self.test_root.clone(),
            eval_set: self.eval_set_path.clone(),
            cases: self.eval_cases,
        }
    }
}

pub fn build_publicdata_fixture(
    dest: &Path,
    documents: &[PublicDataDocument],
    options: PublicDataOptions,
) -> Result<SplitDatasetBuildResult> {
    let dest = absolute_path(dest)?;
    reset_owned_dirs(&dest, &["corpus", "metadata", "eval"])?;
    let order = deterministic_order(documents.iter().map(|doc| doc.id.as_str()), options.seed);
    let split_indices = split_indices(&order, false);
    let mut eval_paths = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut eval_counts = BTreeMap::new();
    for (split, indices) in split_indices {
        let mut rows = Vec::new();
        let mut cases = Vec::new();
        for (position, index) in indices.into_iter().enumerate() {
            let doc = &documents[index];
            let relative = publicdata_path(split, position + 1, doc, options.seed);
            write_bytes(&dest.join("corpus").join(split).join(&relative), &doc.bytes)?;
            rows.push(json!({
                "id": doc.id,
                "filename": doc.filename,
                "title": doc.title,
                "description": doc.description,
                "xlsx_text": doc.xlsx_text,
                "source_url": doc.source_url,
                "license_note": doc.license_note,
                "split": split,
                "bench_path": relative,
            }));
            if cases.len() < options.max_cases_per_split {
                let scenario = [
                    "filename_vague",
                    "content_lexical",
                    "semantic_description",
                    "folder_context",
                    "column_or_value",
                ][position % 5];
                let clue = doc
                    .xlsx_text
                    .first()
                    .map(String::as_str)
                    .unwrap_or(&doc.title);
                cases.push(json!({
                    "id": format!("publicdata-{:04}", position + 1),
                    "scenario": scenario,
                    "query": publicdata_query(scenario, &doc.title, clue, &relative),
                    "expected_paths": [relative],
                    "expected_source_url": doc.source_url,
                    "dataset_title": doc.title,
                    "license_note": doc.license_note,
                    "public_benchmark": true,
                }));
            }
        }
        let metadata = dest.join("metadata").join(format!("{split}_docs.jsonl"));
        write_jsonl_values(&metadata, &rows)?;
        let eval = dest
            .join("eval")
            .join(format!("publicdata_{split}_eval.jsonl"));
        write_jsonl_values(&eval, &cases)?;
        counts.insert(split.to_owned(), rows.len());
        eval_counts.insert(split.to_owned(), cases.len());
        eval_paths.insert(split.to_owned(), eval);
    }
    let manifest_path = dest.join("manifest.json");
    write_json(
        &manifest_path,
        &json!({
            "source_family": "Korean public open data",
            "actual_source": "caller-provided fixture",
            "seed": options.seed,
            "docs_downloaded": documents.len(),
            "splits": counts,
            "eval_sets": eval_paths,
            "eval_set": eval_paths["test"],
            "eval_cases": eval_counts["test"],
            "eval_case_counts": eval_counts,
            "network": "fixture",
        }),
    )?;
    Ok(split_result(
        &dest,
        "publicdata",
        documents.len(),
        &counts,
        &eval_counts,
        manifest_path,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    pub filename: String,
    pub stored_relpath: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceTask {
    pub absolute_id: u64,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub task_diff: Value,
    #[serde(default)]
    pub tested_capabilities: Vec<String>,
    #[serde(default)]
    pub output_files: Vec<String>,
    pub data_manifest: Vec<WorkspaceFile>,
    #[serde(default)]
    pub required_filenames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceBenchBuildResult {
    pub dest: PathBuf,
    pub corpus_root: PathBuf,
    pub eval_set_path: PathBuf,
    pub manifest_path: PathBuf,
    pub tasks: usize,
    pub files_materialized: usize,
    pub bytes_materialized: u64,
    pub eval_cases: usize,
}

impl WorkspaceBenchBuildResult {
    pub fn suite_input(&self) -> SuiteInput {
        SuiteInput {
            corpus_root: self.corpus_root.clone(),
            eval_set: self.eval_set_path.clone(),
            cases: self.eval_cases,
        }
    }
}

pub fn build_workspacebench_fixture(
    dest: &Path,
    tasks: &[WorkspaceTask],
) -> Result<WorkspaceBenchBuildResult> {
    let mut task_ids = BTreeSet::new();
    for task in tasks {
        if !task_ids.insert(task.absolute_id) {
            return Err(invalid_input(format!(
                "duplicate Workspace-Bench task id: {}",
                task.absolute_id
            )));
        }
    }
    let dest = absolute_path(dest)?;
    reset_owned_dirs(&dest, &["corpus", "metadata", "eval"])?;
    let corpus_root = dest.join("corpus");
    let mut sorted_tasks = tasks.iter().collect::<Vec<_>>();
    sorted_tasks.sort_by_key(|task| task.absolute_id);
    let mut cases = Vec::new();
    let mut summaries = Vec::new();
    let mut files_materialized = 0usize;
    let mut bytes_materialized = 0u64;
    for task in sorted_tasks {
        let task_dir = format!("task_{}", task.absolute_id);
        let required = if task.required_filenames.is_empty() {
            task.data_manifest
                .iter()
                .map(|file| file.filename.as_str())
                .collect::<BTreeSet<_>>()
        } else {
            task.required_filenames.iter().map(String::as_str).collect()
        };
        let mut expected = Vec::new();
        for file in &task.data_manifest {
            let relative = safe_relative_path(&file.stored_relpath)?;
            write_bytes(&corpus_root.join(&task_dir).join(&relative), &file.bytes)?;
            files_materialized += 1;
            bytes_materialized += file.bytes.len() as u64;
            if required.contains(file.filename.as_str()) {
                expected.push(format!(
                    "{task_dir}/{}",
                    relative.to_string_lossy().replace('\\', "/")
                ));
            }
        }
        expected.sort();
        let metadata_path = dest
            .join("metadata")
            .join(format!("task_{}_metadata.json", task.absolute_id));
        let metadata =
            serde_json::to_value(task).map_err(|source| json_error(&metadata_path, source))?;
        write_json(&metadata_path, &metadata)?;
        if !expected.is_empty() {
            let persona = if task.persona.is_empty() {
                "workspace agent"
            } else {
                &task.persona
            };
            cases.push(json!({
                "id": format!("workspacebench-{}", task.absolute_id),
                "scenario": "workspace_task_supporting_files",
                "query": format!("You are a {persona}. For this Workspace-Bench task, find the source/input files under this workspace that are needed before producing the requested output. Task: {}", task.task),
                "expected_paths": expected,
                "dataset": "Workspace-Bench-Lite",
                "workspace_task_id": task.absolute_id,
                "persona": persona,
                "task_diff": task.task_diff,
                "tested_capabilities": task.tested_capabilities,
                "output_files": task.output_files,
                "expected_count": expected.len(),
            }));
        }
        summaries.push(json!({
            "task_id": task.absolute_id,
            "persona": task.persona,
            "files": task.data_manifest.len(),
            "expected_files": expected.len(),
        }));
    }
    let eval_set_path = dest.join("eval/workspacebench_lite_eval.jsonl");
    write_jsonl_values(&eval_set_path, &cases)?;
    let manifest_path = dest.join("manifest.json");
    write_json(
        &manifest_path,
        &json!({
            "source": "Workspace-Bench-Lite",
            "source_repo": WORKSPACEBENCH_SOURCE,
            "selected_task_ids": tasks.iter().map(|task| task.absolute_id).collect::<Vec<_>>(),
            "tasks": tasks.len(),
            "files_downloaded": files_materialized,
            "bytes_downloaded": bytes_materialized,
            "eval_set": eval_set_path,
            "eval_cases": cases.len(),
            "tasks_summary": summaries,
            "network": "fixture",
        }),
    )?;
    Ok(WorkspaceBenchBuildResult {
        dest,
        corpus_root,
        eval_set_path,
        manifest_path,
        tasks: tasks.len(),
        files_materialized,
        bytes_materialized,
        eval_cases: cases.len(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardBenchDocument {
    pub id: String,
    pub filename: String,
    pub extension: String,
    #[serde(default)]
    pub page_title: String,
    #[serde(default)]
    pub text_excerpt: String,
    #[serde(default)]
    pub doc_type: String,
    #[serde(default)]
    pub source_url: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardBenchDifficulty {
    Hard,
    Extreme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardBenchOptions {
    pub max_cases_per_split: usize,
    pub seed: u64,
    pub difficulty: HardBenchDifficulty,
}

impl Default for HardBenchOptions {
    fn default() -> Self {
        Self {
            max_cases_per_split: 240,
            seed: 20_260_603,
            difficulty: HardBenchDifficulty::Hard,
        }
    }
}

pub fn build_hardbench_fixture(
    dest: &Path,
    documents: &[HardBenchDocument],
    options: HardBenchOptions,
) -> Result<SplitDatasetBuildResult> {
    let dest = absolute_path(dest)?;
    reset_owned_dirs(&dest, &["corpus", "metadata", "eval"])?;
    let order = deterministic_order(documents.iter().map(|doc| doc.id.as_str()), options.seed);
    let split_indices = split_indices(&order, options.difficulty == HardBenchDifficulty::Extreme);
    let mut eval_paths = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut eval_counts = BTreeMap::new();
    for (split, indices) in split_indices {
        let mut rows = Vec::new();
        let mut cases = Vec::new();
        for (position, index) in indices.into_iter().enumerate() {
            let doc = &documents[index];
            let relative =
                hardbench_path(split, position + 1, doc, options.seed, options.difficulty)?;
            let target = dest.join("corpus").join(split).join(&relative);
            write_bytes(&target, &doc.bytes)?;
            if options.difficulty == HardBenchDifficulty::Extreme {
                let note = target.with_extension("후보목록_링크만.txt");
                write_bytes(
                    &note,
                    b"candidate note only; this is not the original document\n",
                )?;
            }
            rows.push(json!({
                "id": doc.id,
                "filename": doc.filename,
                "page_title": doc.page_title,
                "text_excerpt": doc.text_excerpt,
                "doc_type": doc.doc_type,
                "source_url": doc.source_url,
                "ext": doc.extension,
                "split": split,
                "bench_path": relative,
            }));
            for variant in 0..2 {
                if cases.len() >= options.max_cases_per_split {
                    break;
                }
                let scenario = match (options.difficulty, variant) {
                    (HardBenchDifficulty::Extreme, 0) => "body_phrase_no_filename",
                    (HardBenchDifficulty::Extreme, _) => "decoy_note_resistant",
                    (HardBenchDifficulty::Hard, 0) => "body_rare_phrase",
                    (HardBenchDifficulty::Hard, _) => "format_doc_type_semantic",
                };
                let clue = first_clue(&doc.text_excerpt, &doc.page_title, &doc.filename);
                cases.push(json!({
                    "id": format!("hardbench-{:04}-{scenario}", position + 1),
                    "scenario": scenario,
                    "query": hardbench_query(scenario, clue, &doc.extension, &doc.doc_type),
                    "expected_paths": [relative],
                    "dataset": "KOGL mixed hard document benchmark",
                    "source_url": doc.source_url,
                    "source_filename": doc.filename,
                    "ext": doc.extension,
                    "doc_type": doc.doc_type,
                    "public_benchmark": true,
                }));
            }
        }
        write_jsonl_values(
            &dest.join("metadata").join(format!("{split}_docs.jsonl")),
            &rows,
        )?;
        let eval = dest
            .join("eval")
            .join(format!("hardbench_{split}_eval.jsonl"));
        write_jsonl_values(&eval, &cases)?;
        counts.insert(split.to_owned(), rows.len());
        eval_counts.insert(split.to_owned(), cases.len());
        eval_paths.insert(split.to_owned(), eval);
    }
    let manifest_path = dest.join("manifest.json");
    write_json(
        &manifest_path,
        &json!({
            "source_family": "caller-provided KOGL/open document fixture",
            "seed": options.seed,
            "difficulty": options.difficulty,
            "docs_downloaded": documents.len(),
            "splits": counts,
            "eval_sets": eval_paths,
            "eval_set": eval_paths["test"],
            "eval_case_counts": eval_counts,
            "network": "fixture",
        }),
    )?;
    Ok(split_result(
        &dest,
        "hardbench",
        documents.len(),
        &counts,
        &eval_counts,
        manifest_path,
    ))
}

fn split_result(
    dest: &Path,
    prefix: &str,
    docs: usize,
    counts: &BTreeMap<String, usize>,
    eval_counts: &BTreeMap<String, usize>,
    manifest_path: PathBuf,
) -> SplitDatasetBuildResult {
    SplitDatasetBuildResult {
        dest: dest.to_path_buf(),
        train_root: dest.join("corpus/train"),
        valid_root: dest.join("corpus/valid"),
        test_root: dest.join("corpus/test"),
        train_eval_set_path: dest.join(format!("eval/{prefix}_train_eval.jsonl")),
        valid_eval_set_path: dest.join(format!("eval/{prefix}_valid_eval.jsonl")),
        eval_set_path: dest.join(format!("eval/{prefix}_test_eval.jsonl")),
        manifest_path,
        docs,
        train_docs: counts["train"],
        valid_docs: counts["valid"],
        test_docs: counts["test"],
        eval_cases: eval_counts["test"],
    }
}

fn split_indices(order: &[usize], extreme: bool) -> BTreeMap<&'static str, Vec<usize>> {
    let n = order.len();
    let train_ratio = if extreme { 45 } else { 60 };
    let valid_ratio = if extreme { 60 } else { 80 };
    let train_end = if n == 0 {
        0
    } else {
        ((n * train_ratio) / 100).max(1).min(n)
    };
    let valid_end = if train_end == n {
        n
    } else {
        ((n * valid_ratio) / 100).max(train_end + 1).min(n)
    };
    BTreeMap::from([
        ("train", order[..train_end].to_vec()),
        ("valid", order[train_end..valid_end].to_vec()),
        ("test", order[valid_end..].to_vec()),
    ])
}

fn deterministic_order<'a>(ids: impl Iterator<Item = &'a str>, seed: u64) -> Vec<usize> {
    let mut keyed = ids
        .enumerate()
        .map(|(index, id)| (stable_hash(seed, id.as_bytes()), id.to_owned(), index))
        .collect::<Vec<_>>();
    keyed.sort();
    keyed.into_iter().map(|(_, _, index)| index).collect()
}

fn stable_hash(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325 ^ seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn publicdata_path(split: &str, position: usize, doc: &PublicDataDocument, seed: u64) -> String {
    let hash = stable_hash(seed, doc.id.as_bytes());
    let top = ["공유드라이브", "내문서_백업", "팀자료실"][(hash as usize) % 3];
    let state = ["정리전", "검토중", "원본_섞임"][((hash >> 8) as usize) % 3];
    format!(
        "{top}/{split}/{state}/{position:03}_{}",
        slug(&doc.filename, 80)
    )
}

fn hardbench_path(
    split: &str,
    position: usize,
    doc: &HardBenchDocument,
    seed: u64,
    difficulty: HardBenchDifficulty,
) -> Result<String> {
    let hash = stable_hash(seed, doc.id.as_bytes());
    let top = ["공유드라이브", "인수인계", "외부기관_수신"][hash as usize % 3];
    let bucket = if difficulty == HardBenchDifficulty::Extreme {
        ["검토자료", "첨부자료", "원본확인필요"][(hash >> 8) as usize % 3].to_owned()
    } else if doc.doc_type.is_empty() {
        "참고자료".to_owned()
    } else {
        slug(&doc.doc_type, 48)
    };
    let filename = if difficulty == HardBenchDifficulty::Extreme {
        let extension = slug(doc.extension.trim_start_matches('.'), 12);
        format!("붙임_{position:03}_{}.{}", 1000 + hash % 9000, extension)
    } else {
        slug(&doc.filename, 96)
    };
    let relative = format!("{top}/{split}/{bucket}/연도미상/정리전/{filename}");
    Ok(safe_relative_path(&relative)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn publicdata_query(scenario: &str, title: &str, clue: &str, relative: &str) -> String {
    match scenario {
        "filename_vague" => format!(
            "전에 받은 공공데이터 엑셀 중 제목이 '{}' 비슷했던 원본 파일 찾아줘",
            truncate_chars(title, 35)
        ),
        "content_lexical" => format!(
            "엑셀 본문 안에 '{}' 값이나 항목이 들어간 공공데이터 파일 찾아줘",
            truncate_chars(clue, 30)
        ),
        "semantic_description" => format!(
            "파일명은 정확히 모르지만 {} 관련 현황을 담은 데이터셋을 찾아줘",
            truncate_chars(title, 45)
        ),
        "folder_context" => format!(
            "{} 쪽에 정리해 둔 관련 엑셀 원본을 찾아줘",
            relative.split('/').take(3).collect::<Vec<_>>().join("/")
        ),
        _ => format!(
            "컬럼이나 행 값으로 '{}' 단서가 보이는 스프레드시트를 찾아줘",
            truncate_chars(clue, 30)
        ),
    }
}

fn hardbench_query(scenario: &str, clue: &str, extension: &str, doc_type: &str) -> String {
    match scenario {
        "body_phrase_no_filename" => format!(
            "파일명은 기억 안 나. 본문에 '{clue}' 단서가 있는 실제 {extension} 원본을 찾아줘. txt 메모는 제외해줘."
        ),
        "decoy_note_resistant" => {
            format!("메모 파일 말고 {clue} 단서가 있는 {doc_type} 원본 {extension}만 찾아줘.")
        }
        "body_rare_phrase" => {
            format!("본문 어딘가에 '{clue}'라는 단서가 나오는 {extension} 파일을 찾아줘")
        }
        _ => format!(
            "파일명은 정확히 모르는데 {doc_type} 성격의 {extension} 공공자료를 찾아줘. 단서는 {clue} 정도야"
        ),
    }
}

fn first_clue<'a>(excerpt: &'a str, title: &'a str, filename: &'a str) -> &'a str {
    excerpt
        .split(['\n', '.', '。'])
        .map(str::trim)
        .find(|line| line.chars().count() >= 4)
        .or_else(|| (!title.is_empty()).then_some(title))
        .unwrap_or(filename)
}

fn flatten_pdf_paths(value: &Value) -> Result<Vec<String>> {
    fn visit(value: &Value, paths: &mut BTreeSet<String>) -> Result<()> {
        match value {
            Value::String(path) if path.to_ascii_lowercase().ends_with(".pdf") => {
                paths.insert(normalize_rel(path)?);
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, paths)?;
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    visit(value, paths)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut paths = BTreeSet::new();
    visit(value, &mut paths)?;
    Ok(paths.into_iter().collect())
}

fn resolve_master_path(master: &BTreeSet<String>, path: &str) -> Result<String> {
    if master.contains(path) {
        return Ok(path.to_owned());
    }
    let basename = Path::new(path).file_name();
    let matches = master
        .iter()
        .filter(|candidate| Path::new(candidate.as_str()).file_name() == basename)
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(path.to_owned()),
        [unique] => Ok(unique.clone()),
        _ => Err(invalid_input(format!(
            "ambiguous EDiTh basename: {path:?} matches {} master rows",
            matches.len()
        ))),
    }
}

fn normalize_rel(value: &str) -> Result<String> {
    let normalized = value.replace('\\', "/");
    let normalized = normalized.trim_matches('/');
    let path = safe_relative_path(normalized)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid_input(format!("unsafe fixture path: {value:?}")));
    }
    Ok(path.to_path_buf())
}

fn slug(value: &str, max_len: usize) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let safe = if matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || ch.is_whitespace()
        {
            '_'
        } else {
            ch
        };
        if out.chars().count() >= max_len {
            break;
        }
        out.push(safe);
    }
    let trimmed = out.trim_matches(['.', '_', ' ']);
    if trimmed.is_empty() {
        "untitled".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn reset_owned_dirs(dest: &Path, names: &[&str]) -> Result<()> {
    fs::create_dir_all(dest).map_err(|source| io_error(dest, source))?;
    for name in names {
        let path = dest.join(name);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|source| io_error(&path, source))?;
        }
        fs::create_dir_all(&path).map_err(|source| io_error(&path, source))?;
    }
    Ok(())
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    }
    fs::write(path, bytes).map_err(|source| io_error(path, source))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|source| json_error(path, source))?;
    write_bytes(path, format!("{text}\n").as_bytes())
}

fn write_jsonl_values(path: &Path, rows: &[Value]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row).map_err(|source| json_error(path, source))?);
        text.push('\n');
    }
    write_bytes(path, text.as_bytes())
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().map_err(|source| io_error(".", source))?;
        Ok(cwd.join(path))
    }
}

fn invalid_input(message: impl Into<String>) -> jikji_core::JikjiError {
    io_error(
        "<public-dataset>",
        Error::new(ErrorKind::InvalidInput, message.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edith_fixture_materializes_selected_pdf_and_eval_case() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fixture = EdithFixture {
            master_index: vec![EdithMasterRow {
                filename: "entities/acme/report.pdf".to_owned(),
            }],
            answers: BTreeMap::from([(
                "q1".to_owned(),
                EdithAnswer {
                    question: "Which report?".to_owned(),
                    ground_truth: json!({"documents": ["report.pdf"]}),
                    role: "analyst".to_owned(),
                    entity: "ACME".to_owned(),
                    difficulty_factors: vec!["basename".to_owned()],
                },
            )]),
            documents: BTreeMap::from([(
                "entities/acme/report.pdf".to_owned(),
                b"%PDF fixture".to_vec(),
            )]),
        };

        let result = materialize_edith_fixture(temp.path(), &fixture, EdithOptions::default())
            .expect("materialize");
        assert_eq!(result.selected_questions, 1);
        assert!(
            result
                .corpus_root
                .join("entities/acme/report.pdf")
                .is_file()
        );
        assert!(
            fs::read_to_string(result.eval_set_path)
                .expect("eval")
                .contains("Which report?")
        );
    }

    #[test]
    fn publicdata_fixture_is_deterministic_and_builds_test_suite_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let docs = (0..10)
            .map(|idx| PublicDataDocument {
                id: format!("dataset-{idx}"),
                filename: format!("서울 현황 {idx}.xlsx"),
                title: format!("서울 현황 {idx}"),
                description: "공공 데이터".to_owned(),
                xlsx_text: vec![format!("희귀값{idx}")],
                source_url: format!("https://example.test/{idx}"),
                license_note: "KOGL 1".to_owned(),
                bytes: format!("xlsx-{idx}").into_bytes(),
            })
            .collect::<Vec<_>>();
        let options = PublicDataOptions::default();

        let first = build_publicdata_fixture(temp.path(), &docs, options).expect("first");
        let first_eval = fs::read(&first.eval_set_path).expect("first eval");
        let second = build_publicdata_fixture(temp.path(), &docs, options).expect("second");
        assert_eq!(
            first_eval,
            fs::read(&second.eval_set_path).expect("second eval")
        );
        assert_eq!(second.suite_input().cases, second.eval_cases);
        assert!(second.test_docs > 0);
    }

    #[test]
    fn workspacebench_fixture_uses_required_files_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let task = WorkspaceTask {
            absolute_id: 7,
            persona: "financial analyst".to_owned(),
            task: "prepare the forecast".to_owned(),
            task_diff: Value::Null,
            tested_capabilities: vec!["files".to_owned()],
            output_files: vec!["forecast.xlsx".to_owned()],
            data_manifest: vec![
                WorkspaceFile {
                    filename: "input.csv".to_owned(),
                    stored_relpath: "data/input.csv".to_owned(),
                    bytes: b"input".to_vec(),
                },
                WorkspaceFile {
                    filename: "notes.txt".to_owned(),
                    stored_relpath: "notes.txt".to_owned(),
                    bytes: b"notes".to_vec(),
                },
            ],
            required_filenames: vec!["input.csv".to_owned()],
        };

        let result = build_workspacebench_fixture(temp.path(), &[task]).expect("build");
        let eval = fs::read_to_string(&result.eval_set_path).expect("eval");
        assert!(eval.contains("task_7/data/input.csv"));
        assert!(!eval.contains("task_7/notes.txt"));
        assert_eq!(result.suite_input().cases, 1);
    }

    #[test]
    fn hardbench_fixture_materializes_binary_docs_and_extreme_decoys() {
        let temp = tempfile::tempdir().expect("tempdir");
        let docs = (0..10)
            .map(|idx| HardBenchDocument {
                id: format!("hard-{idx}"),
                filename: format!("정책보고서_{idx}.pdf"),
                extension: ".pdf".to_owned(),
                page_title: format!("정책 보고서 {idx}"),
                text_excerpt: format!("희귀한 본문 단서 {idx}가 포함된 문장입니다"),
                doc_type: "report".to_owned(),
                source_url: format!("https://example.test/hard/{idx}"),
                bytes: format!("%PDF fixture {idx}").into_bytes(),
            })
            .collect::<Vec<_>>();

        let result = build_hardbench_fixture(
            temp.path(),
            &docs,
            HardBenchOptions {
                difficulty: HardBenchDifficulty::Extreme,
                ..HardBenchOptions::default()
            },
        )
        .expect("build");
        assert!(result.test_docs > 0);
        assert!(
            fs::read_to_string(&result.eval_set_path)
                .expect("eval")
                .contains("decoy_note_resistant")
        );
        let decoy_count = count_files_with(&result.test_root, "후보목록");
        assert_eq!(decoy_count, result.test_docs);
    }

    #[test]
    fn fixture_paths_cannot_escape_destination() {
        let temp = tempfile::tempdir().expect("tempdir");
        let task = WorkspaceTask {
            absolute_id: 1,
            persona: String::new(),
            task: String::new(),
            task_diff: Value::Null,
            tested_capabilities: Vec::new(),
            output_files: Vec::new(),
            data_manifest: vec![WorkspaceFile {
                filename: "escape".to_owned(),
                stored_relpath: "../escape.txt".to_owned(),
                bytes: Vec::new(),
            }],
            required_filenames: Vec::new(),
        };
        let error = build_workspacebench_fixture(temp.path(), &[task]).expect_err("unsafe path");
        assert!(error.to_string().contains("unsafe fixture path"));
    }

    #[test]
    fn hardbench_fixture_sanitizes_untrusted_path_components() {
        let temp = tempfile::tempdir().expect("tempdir");
        let docs = (0..10)
            .map(|idx| HardBenchDocument {
                id: format!("unsafe-{idx}"),
                filename: format!("report-{idx}.pdf"),
                extension: "/../../escape".to_owned(),
                page_title: String::new(),
                text_excerpt: "safe clue".to_owned(),
                doc_type: "../../outside".to_owned(),
                source_url: String::new(),
                bytes: b"fixture".to_vec(),
            })
            .collect::<Vec<_>>();
        let result = build_hardbench_fixture(temp.path(), &docs, HardBenchOptions::default())
            .expect("build");
        assert!(result.test_root.is_dir());
        assert!(
            !temp
                .path()
                .parent()
                .expect("parent")
                .join("outside")
                .exists()
        );
    }

    #[test]
    fn workspacebench_fixture_rejects_duplicate_task_ids() {
        let temp = tempfile::tempdir().expect("tempdir");
        let task = WorkspaceTask {
            absolute_id: 7,
            persona: String::new(),
            task: String::new(),
            task_diff: Value::Null,
            tested_capabilities: Vec::new(),
            output_files: Vec::new(),
            data_manifest: Vec::new(),
            required_filenames: Vec::new(),
        };
        let error = build_workspacebench_fixture(temp.path(), &[task.clone(), task])
            .expect_err("duplicate id");
        assert!(
            error
                .to_string()
                .contains("duplicate Workspace-Bench task id")
        );
    }

    #[test]
    fn edith_fixture_rejects_ambiguous_basename_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fixture = EdithFixture {
            master_index: vec![
                EdithMasterRow {
                    filename: "a/report.pdf".to_owned(),
                },
                EdithMasterRow {
                    filename: "b/report.pdf".to_owned(),
                },
            ],
            answers: BTreeMap::from([(
                "q1".to_owned(),
                EdithAnswer {
                    question: "ambiguous".to_owned(),
                    ground_truth: json!(["report.pdf"]),
                    role: String::new(),
                    entity: String::new(),
                    difficulty_factors: Vec::new(),
                },
            )]),
            documents: BTreeMap::new(),
        };
        let error = materialize_edith_fixture(temp.path(), &fixture, EdithOptions::default())
            .expect_err("ambiguous basename");
        assert!(error.to_string().contains("ambiguous EDiTh basename"));
    }

    fn count_files_with(root: &Path, needle: &str) -> usize {
        fs::read_dir(root)
            .expect("read root")
            .map(|entry| {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    count_files_with(&path, needle)
                } else {
                    usize::from(path.to_string_lossy().contains(needle))
                }
            })
            .sum()
    }
}
