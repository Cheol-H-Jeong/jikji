use std::collections::BTreeMap;
use std::path::Path;

use jikji_core::Result;
use serde_json::{Value, json};

use crate::answer_pack::{answer_pack_for, handoff_budget, handoff_policy, tool_call_policy};
use crate::discover_contract::{
    confidence_factors, confidence_for, judge_slate, next_commands, recommended_action, search_plan,
};
use crate::discover_query::{
    anchor_tokens, classify_query, query_variants, retry_proof_for, strip_shell_noise,
};
use crate::searcher::{SearchCandidate, SearchOptions, search};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverOptions {
    pub top_k: usize,
    pub retry_exhausted: bool,
    pub retry_proof: String,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            top_k: 20,
            retry_exhausted: false,
            retry_proof: String::new(),
        }
    }
}

pub fn discover(root: &Path, query: &str, options: DiscoverOptions) -> Result<Value> {
    let request = DiscoverRequest::from(root, query, &options);
    let candidates = if request.retrieval_query.is_empty() {
        Vec::new()
    } else {
        merge_candidates(root, &request.variants, options.top_k)?
    };
    let confidence = confidence_for(&request.query_type, &candidates);
    let action = handoff_action(confidence, request.verified_retry);
    let answer_pack = answer_pack_for(&request.query_type, confidence, &candidates);
    let budget = handoff_budget(action);
    let raw_fallback = budget["raw_fallback_allowed"].as_bool().unwrap_or(false);
    let should_not_rerank = answer_pack["agent_should_not_rerank"]
        .as_bool()
        .unwrap_or(false);
    Ok(json!({
        "mode": "discover",
        "answer_pack_version": 1,
        "root": root.display().to_string(),
        "query": query,
        "query_type": request.query_type,
        "confidence": confidence,
        "confidence_score": if confidence == "high" { 0.9 } else if confidence == "medium_high" { 0.6 } else { 0.0 },
        "confidence_factors": confidence_factors(&candidates, confidence),
        "recommended_action": recommended_action(&request.query_type, confidence),
        "handoff_action": action,
        "handoff_policy": handoff_policy(&request.query_type, action),
        "retry_proof": if confidence == "low" && !request.verified_retry { request.retry_command_proof.clone() } else { String::new() },
        "next_commands": next_commands(root, &request.retry_query, confidence, options.top_k, &request.retry_command_proof, request.verified_retry),
        "paths": candidate_paths(&candidates),
        "answer_paths": answer_pack["answer_paths"].clone(),
        "supporting_paths": answer_pack["supporting_paths"].clone(),
        "requires_llm_rerank": answer_pack["requires_llm_rerank"].clone(),
        "agent_should_not_rerank": answer_pack["agent_should_not_rerank"].clone(),
        "answerability": budget["answerability"].clone(),
        "tool_call_policy": tool_call_policy(action, should_not_rerank, raw_fallback),
        "allowed_agent_tool_calls": budget["allowed_agent_tool_calls"].clone(),
        "allowed_llm_calls": budget["allowed_llm_calls"].clone(),
        "max_jikji_retries": budget["max_jikji_retries"].clone(),
        "max_raw_fallback_commands": budget["max_raw_fallback_commands"].clone(),
        "max_verification_reads": budget["max_verification_reads"].clone(),
        "raw_fallback_allowed": budget["raw_fallback_allowed"].clone(),
        "query_variants": request.variants,
        "llm_search_plan": {
            "mode": "one_call_multi_search_judge",
            "calls_per_cycle": 1,
            "judge": "choose_best_file_from_merged_candidate_slate",
            "rewrite_cycle": "none",
            "candidate_top_k": options.top_k,
            "token_accounting": "query_variants_plus_merged_candidate_slate",
        },
        "search_plan": search_plan(root, &request.variants, options.top_k),
        "judge_candidate_slate": judge_slate(root, &candidates),
        "evidence_pack": answer_pack["evidence_pack"].clone(),
        "candidates": compact_candidates(&candidates),
    }))
}

struct DiscoverRequest {
    retrieval_query: String,
    query_type: String,
    variants: Vec<String>,
    retry_query: String,
    retry_command_proof: String,
    verified_retry: bool,
}

impl DiscoverRequest {
    fn from(root: &Path, query: &str, options: &DiscoverOptions) -> Self {
        let retrieval_query = strip_shell_noise(query);
        let query_type = classify_query(&retrieval_query);
        let variants = if retrieval_query.is_empty() {
            vec![String::new()]
        } else {
            query_variants(&retrieval_query)
        };
        let retry_query = variants
            .get(1)
            .cloned()
            .unwrap_or_else(|| retrieval_query.clone());
        let current_proof = retry_proof_for(root, &retrieval_query, options.top_k);
        let retry_command_proof = retry_proof_for(root, &retry_query, options.top_k);
        Self {
            retrieval_query,
            query_type,
            variants,
            retry_query,
            retry_command_proof,
            verified_retry: options.retry_exhausted && options.retry_proof == current_proof,
        }
    }
}

fn handoff_action(confidence: &str, verified_retry: bool) -> &'static str {
    if confidence == "low" {
        if verified_retry {
            "raw_fallback_after_retry"
        } else {
            "jikji_retry"
        }
    } else {
        "direct_use"
    }
}

fn compact_candidates(candidates: &[SearchCandidate]) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            json!({
                "p": item.path,
                "s": item.discover_score.unwrap_or(item.score),
                "rank": item.best_query_rank.unwrap_or(idx + 1),
                "why": item.reasons.iter().take(5).collect::<Vec<_>>(),
                "terms": item.matched_terms.iter().take(8).collect::<Vec<_>>(),
                "queries": item.queries.iter().take(3).collect::<Vec<_>>(),
                "ev": item.evidence.iter().take(2).cloned().collect::<Vec<_>>().join(" | "),
                "next_read": {"kind":"original","path":item.path},
            })
        })
        .collect()
}

fn candidate_paths(candidates: &[SearchCandidate]) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| !candidate.path.is_empty())
        .map(|candidate| candidate.path.clone())
        .collect()
}

fn merge_candidates(
    root: &Path,
    variants: &[String],
    top_k: usize,
) -> Result<Vec<SearchCandidate>> {
    let mut merged = BTreeMap::<String, SearchCandidate>::new();
    let anchors = anchor_tokens(variants.first().map_or("", String::as_str));
    for (variant_idx, variant) in variants.iter().enumerate() {
        for (rank, item) in search(
            root,
            variant,
            SearchOptions {
                top_k: top_k.max(20) * 3,
            },
        )?
        .into_iter()
        .enumerate()
        {
            merge_candidate(&mut merged, item, variant, variant_idx, rank, &anchors);
        }
    }
    let mut out = merged.into_values().collect::<Vec<_>>();
    out.sort_by(|left, right| {
        right
            .discover_score
            .unwrap_or(right.score)
            .partial_cmp(&left.discover_score.unwrap_or(left.score))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.best_query_rank.cmp(&right.best_query_rank))
            .then_with(|| left.path.cmp(&right.path))
    });
    out.truncate(top_k.max(1));
    Ok(out)
}

fn merge_candidate(
    merged: &mut BTreeMap<String, SearchCandidate>,
    item: SearchCandidate,
    variant: &str,
    variant_idx: usize,
    rank: usize,
    anchors: &[String],
) {
    let weighted = weighted_score(&item, variant_idx, rank, anchors);
    merged
        .entry(item.path.clone())
        .and_modify(|existing| {
            existing.discover_score =
                Some(existing.discover_score.unwrap_or(existing.score) + weighted * 0.35);
            if !existing.queries.iter().any(|query| query == variant) {
                existing.queries.push(variant.to_owned());
            }
            existing.best_query_rank =
                Some(existing.best_query_rank.unwrap_or(rank + 1).min(rank + 1));
        })
        .or_insert_with(|| {
            let mut cloned = item;
            cloned.discover_score = Some(weighted);
            cloned.queries = vec![variant.to_owned()];
            cloned.best_query_rank = Some(rank + 1);
            cloned
        });
}

fn weighted_score(
    item: &SearchCandidate,
    variant_idx: usize,
    rank: usize,
    anchors: &[String],
) -> f64 {
    let mut weighted = item.score / ((rank + 1) as f64).powf(0.35);
    if variant_idx > 0 {
        weighted *= 3.0;
    }
    let path_folded = item.path.to_lowercase();
    let anchor_hits = anchors
        .iter()
        .filter(|anchor| path_folded.contains(anchor.as_str()))
        .count();
    if anchor_hits >= 2 {
        weighted * (12.0 + anchor_hits as f64) + 150_000.0 * anchor_hits as f64
    } else if anchor_hits == 1 {
        weighted * 8.0 + 50_000.0
    } else {
        weighted
    }
}
