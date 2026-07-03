use std::path::Path;

use serde_json::{Value, json};

use crate::graph::graph_query;
use crate::searcher::SearchCandidate;

pub(crate) fn confidence_for(query_type: &str, candidates: &[SearchCandidate]) -> &'static str {
    if candidates.is_empty() {
        return "low";
    }
    if query_type == "evidence_set" && candidates.len() >= 2 {
        return "medium";
    }
    let top = candidates[0].discover_score.unwrap_or(candidates[0].score);
    let second = candidates
        .get(1)
        .map(|candidate| candidate.discover_score.unwrap_or(candidate.score))
        .unwrap_or(0.0);
    let has_anchor = candidates[0]
        .reasons
        .iter()
        .any(|reason| reason == "filename-anchor" || reason == "fielded-bm25");
    if has_anchor && top > second * 1.3 {
        "high"
    } else {
        "medium_high"
    }
}

pub(crate) fn next_commands(
    root: &Path,
    retry_query: &str,
    confidence: &str,
    top_k: usize,
    retry_proof: &str,
    verified_retry: bool,
) -> Vec<String> {
    if confidence != "low" || verified_retry {
        return Vec::new();
    }
    vec![format!(
        "jikji discover '{}' '{}' --top-k {} --after-jikji-retry --retry-proof '{}' --json",
        root.display(),
        retry_query.replace('\'', "'\\''"),
        top_k,
        retry_proof
    )]
}

pub(crate) fn confidence_factors(candidates: &[SearchCandidate], confidence: &str) -> Value {
    if candidates.is_empty() {
        return json!({"score_margin":0.0,"variant_agreement":0.0,"family_coherence":0.0,"evidence_coverage":0.0,"duplicate_or_anchor_signal":0.0});
    }
    let value = if confidence == "high" { 1.0 } else { 0.5 };
    json!({"score_margin":value,"variant_agreement":value,"family_coherence":value,"evidence_coverage":value,"duplicate_or_anchor_signal":value})
}

pub(crate) fn recommended_action(query_type: &str, confidence: &str) -> &'static str {
    match (query_type, confidence) {
        ("single_file", "high") => "return_top1_after_light_verification",
        ("evidence_set", "medium" | "medium_high" | "high") => "return_top5_to_top10_evidence_set",
        (_, "low") => "rewrite_query_and_fallback_search",
        _ => "verify_top_candidates",
    }
}

pub(crate) fn search_plan(root: &Path, variants: &[String], top_k: usize) -> Value {
    json!({
        "mode": "deterministic_multi_search",
        "routes": [
            {"route":"lexical_file_map","source":".jikji/file_cards.jsonl","query_variants":variants,"per_route_top_k":top_k.max(60)},
            {"route":"graph_route","source":".jikji/knowledge_graph.json","query_variants":variants,"per_route_top_k":top_k.max(60)},
            {"route":"wiki_cache","source":".jikji/wiki/","query_variants":variants,"per_route_top_k":top_k.max(60)},
            {"route":"metadata","source":".jikji/file_cards.jsonl","query_variants":variants,"per_route_top_k":top_k.max(60)}
        ],
        "merge":"dedupe_by_path_then_rank_by_discover_score",
        "candidate_top_k":top_k,
        "root": root.display().to_string(),
    })
}

pub(crate) fn judge_slate(root: &Path, candidates: &[SearchCandidate]) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let graph_hits = graph_query(root, &item.path, 1).unwrap_or_default();
            json!({
                "rank": idx + 1,
                "path": item.path,
                "score": item.discover_score.unwrap_or(item.score),
                "routes": if graph_hits.is_empty() { vec!["lexical_file_map", "metadata"] } else { vec!["lexical_file_map", "graph_route", "wiki_cache", "metadata"] },
                "queries": item.queries.iter().take(3).collect::<Vec<_>>(),
                "evidence": item.evidence.iter().take(2).collect::<Vec<_>>(),
                "next_read": {"kind":"original","path":item.path},
            })
        })
        .collect()
}
