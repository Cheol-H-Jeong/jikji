use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jikji_core::{JIKJI_DIR, PrepareOptions, Result, ensure_generated_dir};
use jikji_search::build_search_artifacts;
use serde::Serialize;
use serde_json::Value;

use crate::artifact_rows::{deleted_rows, file_rows, folder_rows, merge_document_fields};
use crate::artifact_writer::write_static_artifacts;
use crate::doc_cache::{CacheDirs, document_rows};
use crate::doc_prune::prune_doc_caches;
use crate::file_io::{read_jsonl, write_jsonl};
use crate::lock::LockGuard;
use crate::scan::{ScanResult, rel_path, scan_root};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareResult {
    pub root: PathBuf,
    pub index_dir: PathBuf,
    pub agent_map: PathBuf,
    pub files: usize,
    pub folders: usize,
    pub docs_parsed: usize,
    pub docs_reused: usize,
    pub docs_failed: usize,
    pub deleted: usize,
}

pub fn prepare(root: &Path, options: &PrepareOptions) -> Result<PrepareResult> {
    let scan = scan_root(root, options)?;
    let index_dir = scan.root.join(JIKJI_DIR);
    ensure_generated_dir(&index_dir)?;
    let _guard = LockGuard::acquire(&index_dir)?;
    build_artifacts(scan, options)
}

fn build_artifacts(scan: ScanResult, options: &PrepareOptions) -> Result<PrepareResult> {
    let index_dir = scan.root.join(JIKJI_DIR);
    let doc_text_dir = index_dir.join("doc_text");
    let doc_meta_dir = index_dir.join("doc_meta");
    ensure_generated_dir(&doc_text_dir)?;
    ensure_generated_dir(&doc_meta_dir)?;

    let previous = read_jsonl(index_dir.join("file_index.jsonl"))?;
    let current_paths = scan
        .files
        .iter()
        .map(|path| rel_path(&scan.root, path))
        .collect::<BTreeSet<_>>();
    let mut deleted_rows = deleted_rows(&previous, &current_paths);
    let mut file_rows = file_rows(&scan, options)?;
    let folder_rows = folder_rows(&scan);
    let docs = document_rows(
        &scan,
        CacheDirs {
            text: &doc_text_dir,
            meta: &doc_meta_dir,
        },
        options,
    )?;
    prune_doc_caches(&doc_text_dir, &doc_meta_dir, &docs.live_digests)?;
    file_rows = merge_document_fields(file_rows, &docs.rows);
    file_rows.append(&mut deleted_rows);

    write_index_rows(
        &index_dir,
        RowSets {
            files: &file_rows,
            folders: &folder_rows,
            docs: &docs.rows,
            chunks: &docs.chunk_rows,
        },
    )?;
    let search_stats =
        build_search_artifacts(&index_dir, &file_rows, &docs.chunk_rows, &folder_rows)?;
    write_static_artifacts(&scan, options, &docs, search_stats)?;

    Ok(PrepareResult {
        root: scan.root.clone(),
        agent_map: index_dir.join("agent_map.md"),
        index_dir,
        files: scan.signature.files,
        folders: scan.dirs.len(),
        docs_parsed: docs.parsed,
        docs_reused: 0,
        docs_failed: docs.failed,
        deleted: file_rows
            .iter()
            .filter(|row| row.get("status").and_then(Value::as_str) == Some("deleted"))
            .count(),
    })
}

struct RowSets<'a> {
    files: &'a [Value],
    folders: &'a [Value],
    docs: &'a [Value],
    chunks: &'a [Value],
}

fn write_index_rows(index_dir: &Path, rows: RowSets<'_>) -> Result<()> {
    write_jsonl(index_dir.join("file_index.jsonl"), rows.files)?;
    write_jsonl(index_dir.join("folder_index.jsonl"), rows.folders)?;
    write_jsonl(index_dir.join("document_index.jsonl"), rows.docs)?;
    write_jsonl(index_dir.join("file_cards.jsonl"), rows.files)?;
    write_jsonl(index_dir.join("chunk_map.jsonl"), rows.chunks)?;
    write_jsonl(index_dir.join("duplicate_map.jsonl"), &Vec::<Value>::new())?;
    write_jsonl(index_dir.join("folder_profile.jsonl"), rows.folders)?;
    write_jsonl(index_dir.join("parse_errors.jsonl"), &Vec::<Value>::new())?;
    write_jsonl(index_dir.join("graph_routes.jsonl"), &Vec::<Value>::new())
}
