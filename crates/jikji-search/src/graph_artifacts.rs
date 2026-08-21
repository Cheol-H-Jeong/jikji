use std::path::Path;

use jikji_core::Result;
use jikji_core::storage::replace_artifacts;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::index_rows::{IndexRow, evidence_for};
use crate::tokenizer::tokens;

pub(crate) fn build_graph_artifacts(
    root: &Path,
    rows: &[IndexRow],
    folder_profiles: &[Value],
) -> Result<(usize, usize)> {
    let mut graph = GraphArtifactRows {
        folders_linked: folder_profiles.len(),
        ..GraphArtifactRows::default()
    };
    graph
        .nodes
        .push(json!({"id":"root","kind":"corpus","label":"root"}));
    add_folder_profiles(&mut graph, folder_profiles);
    for row in rows {
        add_source(row, &mut graph);
    }
    graph
        .nodes
        .sort_by_key(|node| node["id"].as_str().unwrap_or("").to_owned());
    graph
        .nodes
        .dedup_by(|left, right| left["id"] == right["id"]);
    graph.edges.sort_by_key(edge_sort_key);
    graph
        .routes
        .sort_by_key(|row| row["path"].as_str().unwrap_or("").to_owned());
    write_graph_rows(root, rows, &graph)?;
    Ok((graph.nodes.len(), graph.edges.len()))
}

#[derive(Default)]
struct GraphArtifactRows {
    nodes: Vec<Value>,
    edges: Vec<Value>,
    routes: Vec<Value>,
    folders_linked: usize,
}

fn add_folder_profiles(graph: &mut GraphArtifactRows, folder_profiles: &[Value]) {
    for folder in folder_profiles {
        let folder_path = folder
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .unwrap_or(".");
        let folder_id = safe_id("folder", folder_path);
        graph
            .nodes
            .push(json!({"id": folder_id, "kind": "folder", "label": folder_path}));
        graph
            .edges
            .push(json!({"src":"root","dst":folder_id,"kind":"contains_folder","weight":1.0}));
    }
}

fn add_source(row: &IndexRow, graph: &mut GraphArtifactRows) {
    let source_id = safe_id("source", &row.path);
    let wiki_rel = format!("wiki/sources/{}", source_slug(&row.path));
    let terms = tokens(&format!("{} {} {}", row.path, row.summary, row.body), 24);
    graph.nodes.push(json!({
        "id": source_id,
        "kind": "source",
        "label": row.path,
        "path": row.path,
        "wiki_path": wiki_rel,
        "ext": row.ext,
        "text_cache_path": row.text_cache_path,
    }));
    graph
        .edges
        .push(json!({"src":"root","dst":source_id,"kind":"contains_source","weight":1.0}));
    for term in terms.iter().take(12) {
        let term_id = format!("term:{term}");
        graph
            .nodes
            .push(json!({"id": term_id, "kind": "term", "label": term}));
        graph
            .edges
            .push(json!({"src":source_id,"dst":term_id,"kind":"mentions","weight":0.8}));
    }
    graph.routes.push(json!({
        "schema_version": 1,
        "path": row.path,
        "source_id": source_id,
        "wiki_path": wiki_rel,
        "folder": parent_folder(&row.path),
        "terms": terms.iter().take(12).collect::<Vec<_>>(),
        "intents": Vec::<String>::new(),
        "ext": row.ext,
        "parse_status": "",
        "text_cache_path": row.text_cache_path,
        "preview": evidence_for(&row.body, &row.summary, &row.name).first().cloned().unwrap_or_default(),
    }));
}

fn edge_sort_key(edge: &Value) -> String {
    format!(
        "{}:{}:{}",
        edge["src"].as_str().unwrap_or(""),
        edge["kind"].as_str().unwrap_or(""),
        edge["dst"].as_str().unwrap_or("")
    )
}

fn write_graph_rows(root: &Path, rows: &[IndexRow], graph: &GraphArtifactRows) -> Result<()> {
    let knowledge_graph = json!({
        "schema_version": 1,
        "root": root.display().to_string(),
        "source": "jikji deterministic llm-wiki compiler",
        "nodes": graph.nodes,
        "edges": graph.edges,
        "stats": {
            "nodes": graph.nodes.len(),
            "edges": graph.edges.len(),
            "sources": rows.len(),
            "folders_linked": graph.folders_linked,
            "terms": graph.routes.len(),
            "intents": 0,
        },
    });
    let mut artifacts = vec![
        ("graph", knowledge_graph),
        ("wiki_index", json!({"markdown": wiki_index(rows)})),
    ];
    artifacts.extend(
        graph
            .routes
            .iter()
            .cloned()
            .map(|row| ("graph_routes", row)),
    );
    artifacts.extend(rows.iter().map(|row| {
        (
            "wiki",
            json!({"path": row.path, "markdown": source_page(row, "")}),
        )
    }));
    replace_artifacts(root, &artifacts)
}

fn safe_id(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{prefix}:{:x}", hasher.finalize())[..prefix.len() + 1 + 16].to_owned()
}

fn source_slug(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("source")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || crate::tokenizer::is_cjk(ch)
            {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(48)
        .collect::<String>();
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "{}-{}.md",
        if stem.is_empty() {
            "source"
        } else {
            stem.as_str()
        },
        &digest[..12]
    )
}

fn source_page(row: &IndexRow, wiki_rel: &str) -> String {
    format!(
        "---\nschema: jikji.llm_wiki.source.v1\nsource_path: \"{}\"\nwiki_path: \"{}\"\n---\n\n# {}\n\n## Agent-use summary\n- Original path: `{}`\n- File type: `{}`\n- Text cache: `{}`\n\n## Grounded preview\n{}\n",
        row.path,
        wiki_rel,
        row.path,
        row.path,
        row.ext,
        row.text_cache_path,
        evidence_for(&row.body, &row.summary, &row.name).join("\n")
    )
}

fn wiki_index(rows: &[IndexRow]) -> String {
    let mut lines = vec![
        "---".to_owned(),
        "schema: jikji.llm_wiki.index.v1".to_owned(),
        format!("sources: {}", rows.len()),
        "---".to_owned(),
        String::new(),
        "# Jikji LLM Wiki".to_owned(),
        String::new(),
    ];
    lines.extend(rows.iter().map(|row| format!("- `{}`", row.path)));
    lines.join("\n")
}

fn parent_folder(path: &str) -> String {
    let parent = Path::new(path)
        .parent()
        .map(Path::to_string_lossy)
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .unwrap_or_else(|| ".".to_owned());
    if parent.is_empty() {
        ".".to_owned()
    } else {
        parent
    }
}
