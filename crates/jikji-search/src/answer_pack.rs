use serde_json::{Value, json};

use crate::searcher::SearchCandidate;

const MAX_EVIDENCE_CHARS: usize = 240;

pub(crate) fn answer_pack_for(
    query_type: &str,
    confidence: &str,
    candidates: &[SearchCandidate],
) -> Value {
    let originals = candidates
        .iter()
        .filter(|candidate| !is_generated_path(&candidate.path))
        .collect::<Vec<_>>();
    let answer_limit = match (query_type, confidence) {
        ("single_file", "high") => 1,
        ("single_file", "medium_high") => 3,
        ("evidence_set", _) => 10,
        _ => 5,
    };
    let answer_paths = originals
        .iter()
        .take(answer_limit)
        .map(|candidate| candidate.path.clone())
        .collect::<Vec<_>>();
    let supporting_paths = originals
        .iter()
        .skip(answer_limit)
        .take(10usize.saturating_sub(answer_limit))
        .map(|candidate| candidate.path.clone())
        .filter(|path| !answer_paths.contains(path))
        .collect::<Vec<_>>();
    let direct = !answer_paths.is_empty() && (confidence == "high" || query_type == "evidence_set");
    let evidence_pack = originals
        .iter()
        .take(answer_limit.clamp(1, 5))
        .map(|candidate| {
            json!({
                "path": candidate.path,
                "why": candidate.reasons.iter().take(5).collect::<Vec<_>>(),
                "matched_terms": candidate.matched_terms.iter().take(8).collect::<Vec<_>>(),
                "evidence": candidate.evidence.iter().take(2).map(|value| truncate(value, MAX_EVIDENCE_CHARS)).collect::<Vec<_>>(),
                "next_read": next_read(candidate),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "answer_paths": answer_paths,
        "supporting_paths": supporting_paths,
        "evidence_pack": evidence_pack,
        "requires_llm_rerank": !direct,
        "agent_should_not_rerank": direct,
        "allowed_llm_calls": if answer_paths.is_empty() { 2 } else { 1 },
    })
}

pub(crate) fn handoff_policy(query_type: &str, action: &str) -> Value {
    match action {
        "jikji_retry" => json!({
            "agent_budget": "run_exactly_1_more_jikji_query_before_raw_fallback",
            "use_payload_directly": false,
            "raw_fallback_allowed": "only_after_one_jikji_retry_fails",
            "verification": "inspect_original_top_1_to_3_only",
        }),
        "raw_fallback_after_retry" => json!({
            "agent_budget": "raw_fallback_allowed_after_failed_jikji_retry",
            "use_payload_directly": false,
            "raw_fallback_allowed": "yes_after_one_jikji_retry_failed",
            "verification": "inspect_original_top_1_to_3_only_or_raw_fallback",
        }),
        _ if query_type == "evidence_set" => json!({
            "agent_budget": "zero_extra_discovery_calls",
            "use_payload_directly": true,
            "raw_fallback_allowed": "no",
            "verification": "return_top_5_to_10_or_inspect_original_top_1_to_3_only",
        }),
        _ => json!({
            "agent_budget": "zero_extra_discovery_calls",
            "use_payload_directly": true,
            "raw_fallback_allowed": "no",
            "verification": "return_top_path_or_inspect_original_top_1_only",
        }),
    }
}

pub(crate) fn handoff_budget(action: &str) -> Value {
    match action {
        "jikji_retry" => json!({
            "answerability": "needs_one_jikji_retry",
            "allowed_agent_tool_calls": 1,
            "allowed_llm_calls": 1,
            "max_jikji_retries": 1,
            "max_raw_fallback_commands": 0,
            "max_verification_reads": 0,
            "raw_fallback_allowed": false,
        }),
        "raw_fallback_after_retry" => json!({
            "answerability": "needs_raw_fallback_after_retry",
            "allowed_agent_tool_calls": 3,
            "allowed_llm_calls": 2,
            "max_jikji_retries": 0,
            "max_raw_fallback_commands": 2,
            "max_verification_reads": 3,
            "raw_fallback_allowed": true,
        }),
        _ => json!({
            "answerability": "answerable_from_payload",
            "allowed_agent_tool_calls": 0,
            "allowed_llm_calls": 0,
            "max_jikji_retries": 0,
            "max_raw_fallback_commands": 0,
            "max_verification_reads": 3,
            "raw_fallback_allowed": false,
        }),
    }
}

pub(crate) fn tool_call_policy(
    action: &str,
    agent_should_not_rerank: bool,
    raw_fallback: bool,
) -> Value {
    if action == "direct_use" && !raw_fallback {
        return json!({
            "stop_after_find": true,
            "allowed_followups": ["verify_top_1_path", "return_answer_paths_to_user"],
            "forbidden_tools": ["read_file","search","grep","rg","find","fd","ls","cat","tree","glob","skills_list"],
            "rerank_locked": agent_should_not_rerank,
            "reason": "jikji_find_result_is_answerable_from_payload",
            "escape_hatch": "none_unless_handoff_action_allows_jikji_retry_or_raw_fallback_after_retry",
        });
    }
    let allowed = match action {
        "jikji_retry" => vec!["run_one_sharper_jikji_find_retry"],
        "raw_fallback_after_retry" => vec!["verify_top_1_to_3_paths", "raw_fallback_after_retry"],
        _ => vec!["verify_top_1_to_3_paths"],
    };
    json!({
        "stop_after_find": false,
        "allowed_followups": allowed,
        "forbidden_tools": Vec::<String>::new(),
        "rerank_locked": agent_should_not_rerank,
        "reason": format!("jikji_find_result_requires_{action}"),
        "escape_hatch": action,
    })
}

pub(crate) fn next_read(candidate: &SearchCandidate) -> Value {
    json!({"kind": "original", "path": candidate.path})
}

fn is_generated_path(path: &str) -> bool {
    path.split('/').any(|part| part == ".jikji")
        || matches!(path, ".jikji_agent_map.md" | "000_JIKJI_AGENT_MAP.md")
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
