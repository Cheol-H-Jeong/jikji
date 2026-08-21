use std::collections::BTreeMap;
use std::path::Path;

use jikji_core::Result;
use jikji_core::storage::{load_artifact, load_artifacts};
use serde_json::{Value, json};

use crate::tokenizer::query_terms as tokenize_query_terms;

pub fn graph_status(root: &Path) -> Value {
    let graph = load_artifact(root, "graph")
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let manifest = load_artifact(root, "manifest")
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    json!({
        "available": !graph.as_object().is_none_or(serde_json::Map::is_empty),
        "nodes": graph.pointer("/stats/nodes").and_then(Value::as_u64).unwrap_or(0),
        "edges": graph.pointer("/stats/edges").and_then(Value::as_u64).unwrap_or(0),
        "generated_at": manifest.get("generated_at").cloned().unwrap_or(Value::Null),
    })
}

pub fn graph_query(root: &Path, query: &str, top_k: usize) -> Result<Vec<Value>> {
    let query_terms = tokenize_query_terms(query);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut ranked = Vec::new();
    for row in load_artifacts(root, "graph_routes")? {
        let fields = ["path", "folder", "preview", "ext"]
            .iter()
            .filter_map(|key| row.get(key).and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        let route_terms = tokenize_query_terms(&format!(
            "{} {} {}",
            fields,
            array_text(&row, "terms"),
            array_text(&row, "intents")
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
        ranked.push(json!({"path":path,"source_id":row.get("source_id"),"wiki_path":row.get("wiki_path"),"preview":row.get("preview"),"matched_terms":overlap,"score":overlap.len() * 10 + path_hits * 5}));
    }
    ranked.sort_by(|left, right| {
        right["score"]
            .as_u64()
            .cmp(&left["score"].as_u64())
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    ranked.truncate(top_k.max(1));
    Ok(ranked)
}

pub fn explain_source(root: &Path, source_path: &str) -> Value {
    let route = load_artifacts(root, "graph_routes")
        .unwrap_or_default()
        .into_iter()
        .find(|row| row.get("path").and_then(Value::as_str) == Some(source_path))
        .unwrap_or_else(|| json!({}));
    let source_id = route.get("source_id").and_then(Value::as_str).unwrap_or("");
    let graph = load_artifact(root, "graph")
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let mut neighbors = BTreeMap::<String, Vec<Value>>::new();
    if let Some(edges) = graph.get("edges").and_then(Value::as_array) {
        for edge in edges {
            let src = edge.get("src").and_then(Value::as_str).unwrap_or("");
            let dst = edge.get("dst").and_then(Value::as_str).unwrap_or("");
            if src == source_id {
                neighbors
                    .entry("outgoing".to_owned())
                    .or_default()
                    .push(edge.clone());
            }
            if dst == source_id {
                neighbors
                    .entry("incoming".to_owned())
                    .or_default()
                    .push(edge.clone());
            }
        }
    }
    json!({"found": !route.as_object().is_none_or(serde_json::Map::is_empty), "route":route,"neighbors":neighbors})
}

fn array_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}
