use std::collections::BTreeMap;
use std::path::Path;

use jikji_core::Result;
use serde_json::{Value, json};

use crate::io::{read_json_optional, read_jsonl_optional};
use crate::tokenizer::query_terms as tokenize_query_terms;

pub fn graph_status(root: &Path) -> Value {
    let index_dir = root.join(".jikji");
    let graph = read_json_optional(&index_dir.join("knowledge_graph.json"));
    let manifest = read_json_optional(&index_dir.join("manifest.json"));
    json!({
        "root": root.display().to_string(),
        "prepared": graph.as_object().is_some_and(|object| !object.is_empty()),
        "manifest": {
            "files": manifest.get("files").cloned().unwrap_or(Value::Null),
            "folders": manifest.get("folders").cloned().unwrap_or(Value::Null),
            "documents": manifest.get("documents").cloned().unwrap_or(Value::Null),
            "llm_wiki_sources": manifest.get("llm_wiki_sources").cloned().unwrap_or(Value::Null),
            "knowledge_graph_nodes": manifest.get("knowledge_graph_nodes").cloned().unwrap_or(Value::Null),
            "knowledge_graph_edges": manifest.get("knowledge_graph_edges").cloned().unwrap_or(Value::Null),
        },
        "stats": graph.get("stats").cloned().unwrap_or_else(|| json!({})),
        "artifacts": {
            "wiki_index": index_dir.join("wiki/index.md").display().to_string(),
            "knowledge_graph": index_dir.join("knowledge_graph.json").display().to_string(),
            "graph_routes": index_dir.join("graph_routes.jsonl").display().to_string(),
        },
    })
}

pub fn graph_query(root: &Path, query: &str, top_k: usize) -> Result<Vec<Value>> {
    let query_terms = tokenize_query_terms(query);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut ranked = Vec::new();
    for row in read_jsonl_optional(&root.join(".jikji/graph_routes.jsonl")) {
        let fields = [
            row.get("path").and_then(Value::as_str).unwrap_or(""),
            row.get("folder").and_then(Value::as_str).unwrap_or(""),
            row.get("preview").and_then(Value::as_str).unwrap_or(""),
        ]
        .join(" ");
        let route_terms = tokenize_query_terms(&format!(
            "{} {} {}",
            fields,
            row.get("terms").cloned().unwrap_or(Value::Null),
            row.get("intents").cloned().unwrap_or(Value::Null)
        ));
        let overlap = query_terms
            .intersection(&route_terms)
            .cloned()
            .collect::<Vec<_>>();
        if overlap.is_empty() {
            continue;
        }
        let path = row.get("path").and_then(Value::as_str).unwrap_or("");
        let path_hits = overlap
            .iter()
            .filter(|term| path.to_lowercase().contains(term.as_str()))
            .count();
        ranked.push(json!({
            "path": path,
            "score": overlap.len() * 100 + path_hits * 20,
            "matched_terms": overlap.into_iter().take(16).collect::<Vec<_>>(),
            "wiki_path": row.get("wiki_path").cloned().unwrap_or(Value::String(String::new())),
            "text_cache_path": row.get("text_cache_path").cloned().unwrap_or(Value::String(String::new())),
            "folder": row.get("folder").cloned().unwrap_or(Value::String(String::new())),
            "intents": row.get("intents").cloned().unwrap_or_else(|| json!([])),
            "preview": row.get("preview").cloned().unwrap_or(Value::String(String::new())),
        }));
    }
    ranked.sort_by(|left, right| {
        right["score"]
            .as_i64()
            .cmp(&left["score"].as_i64())
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    ranked.truncate(top_k.max(1));
    Ok(ranked)
}

pub fn explain_source(root: &Path, source_path: &str) -> Value {
    let route = read_jsonl_optional(&root.join(".jikji/graph_routes.jsonl"))
        .into_iter()
        .find(|row| row.get("path").and_then(Value::as_str) == Some(source_path))
        .unwrap_or_else(|| json!({}));
    let source_id = route.get("source_id").and_then(Value::as_str).unwrap_or("");
    let graph = read_json_optional(&root.join(".jikji/knowledge_graph.json"));
    let mut neighbors = BTreeMap::<String, Vec<Value>>::new();
    if let Some(edges) = graph.get("edges").and_then(Value::as_array) {
        for edge in edges {
            let src = edge.get("src").and_then(Value::as_str).unwrap_or("");
            let dst = edge.get("dst").and_then(Value::as_str).unwrap_or("");
            if src == source_id || dst == source_id {
                let kind = edge
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("edge")
                    .to_owned();
                neighbors.entry(kind).or_default().push(edge.clone());
            }
        }
    }
    json!({
        "root": root.display().to_string(),
        "path": source_path,
        "found": route.as_object().is_some_and(|object| !object.is_empty()),
        "route": route,
        "neighbors": neighbors,
    })
}
