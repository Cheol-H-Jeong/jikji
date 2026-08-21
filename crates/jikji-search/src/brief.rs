use std::path::Path;

use jikji_core::Result;
use jikji_core::storage::{database_path, load_artifact, root_key};
use serde_json::{Value, json};

use crate::answer_pack::handoff_policy;
use crate::discover::DiscoverOptions;
use crate::discover::discover;
use crate::graph::graph_query;
use crate::searcher::SearchCandidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BriefOptions {
    pub top_k: usize,
    pub foreground_prepared: bool,
    pub background_refresh_started: bool,
}

pub fn brief_payload(
    root: &Path,
    query: &str,
    index_status: &str,
    options: BriefOptions,
    candidates: &[SearchCandidate],
) -> Value {
    let manifest = load_artifact(root, "manifest")
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let enriched = candidates
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            json!({
                "rank": idx + 1,
                "path": item.path,
                "score": item.score,
                "reasons": item.reasons,
                "matched_terms": item.matched_terms,
                "matched_intents": item.matched_intents,
                "duplicate_group_id": item.duplicate_group_id,
                "ext": "",
                "parse_status": "",
                "text_cache_path": "",
                "evidence": item.evidence,
                "next_reads": [{"purpose":"open original file if final verification is needed","path":root.join(&item.path).display().to_string()}],
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": 1,
        "root": root.display().to_string(),
        "query": query,
        "top_k": options.top_k,
        "index_status": index_status,
        "foreground_prepared": options.foreground_prepared,
        "background_refresh_started": options.background_refresh_started,
        "agent_policy": [
            "Use candidate paths first; avoid broad filesystem browsing when a candidate is plausible.",
            "Return relative paths exactly as listed under candidates.path.",
            "Read original files only for final verification or when evidence is insufficient.",
            "Never move, rename, delete, or reorganize source files."
        ],
        "route_order": [
            "1. Trust this brief's candidates when evidence/reasons match the user request.",
            "2. If ambiguous, run repeat_ranked_search with a sharper query or larger top-k.",
            "3. If still insufficient, rerun Jikji with a sharper query.",
            "4. Use candidate evidence stored in the central Jikji SQLite database.",
            "5. Open original text-like files only as a final verification step."
        ],
        "corpus_summary": {
            "files": manifest.get("files").cloned().unwrap_or(Value::Null),
            "folders": manifest.get("folders").cloned().unwrap_or(Value::Null),
            "documents": manifest.get("documents").cloned().unwrap_or(Value::Null),
            "chunks": manifest.get("chunks").cloned().unwrap_or(Value::Null),
            "search_index_bytes": manifest.get("search_index_bytes").cloned().unwrap_or(Value::Null),
            "top_extensions": {},
            "parse_status_counts": {}
        },
        "candidate_folders": [],
        "candidates": enriched,
        "commands": {
            "repeat_ranked_search": format!("jikji search '{}' '{}' --top-k {} --json", root.display(), query, options.top_k),
            "fallback_generated_map_rg": "",
            "fallback_doc_text_rg": "",
            "last_resort_original_rg": ""
        },
        "artifacts": {
            "storage": "central_sqlite",
            "database": database_path().ok().map(|path| path.display().to_string()),
            "root_key": root_key(root).ok(),
            "visible_map": root.join(".jikji_agent_map.md").display().to_string()
        },
    })
}

pub fn compact_brief_payload(
    root: &Path,
    query: &str,
    index_status: &str,
    options: BriefOptions,
    candidates: &[SearchCandidate],
) -> Result<Value> {
    let discover = discover(
        root,
        query,
        DiscoverOptions {
            top_k: options.top_k,
            retry_exhausted: false,
            retry_proof: String::new(),
        },
    )?;
    let compact = candidates
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let route = graph_query(root, &item.path, 1)
                .ok()
                .and_then(|mut rows| rows.pop())
                .unwrap_or_else(|| json!({}));
            json!({
                "r": idx + 1,
                "p": item.path,
                "s": item.score,
                "why": item.reasons.iter().take(4).collect::<Vec<_>>(),
                "terms": item.matched_terms.iter().take(8).collect::<Vec<_>>(),
                "intents": item.matched_intents.iter().take(4).collect::<Vec<_>>(),
                "wiki": route.get("wiki_path").cloned().unwrap_or(Value::String(String::new())),
                "cache": route.get("text_cache_path").cloned().unwrap_or(Value::String(String::new())),
                "ev": item.evidence.first().cloned().unwrap_or_default(),
                "next_read": {"kind":"original","path":item.path},
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": 1,
        "mode": "compact_graph_brief",
        "root": root.display().to_string(),
        "q": query,
        "top_k": options.top_k,
        "index": index_status,
        "prepared": options.foreground_prepared,
        "refreshing": options.background_refresh_started,
        "policy": "Use candidates[].p first. Read candidates[].wiki/cache only if ambiguous. Open original only for final verification. Do not browse whole filesystem first.",
        "handoff_action": discover["handoff_action"].clone(),
        "handoff_policy": discover.get("handoff_policy").cloned().unwrap_or_else(|| handoff_policy("adaptive", "direct_use")),
        "artifacts": {
            "storage": "central_sqlite",
            "database": database_path().ok().map(|path| path.display().to_string()),
            "root_key": root_key(root).ok()
        },
        "candidates": compact,
    }))
}
