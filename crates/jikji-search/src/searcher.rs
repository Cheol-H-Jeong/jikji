use std::collections::BTreeMap;
use std::path::Path;

use jikji_core::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::SEARCH_INDEX_SCHEMA_VERSION;
use crate::io::sqlite_error;
use crate::map_rescore;
use crate::scoring::{ScoreMap, TermMap, score_field_hits, score_filename_hits};
use crate::tokenizer::{query_terms, tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub top_k: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self { top_k: 10 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchCandidate {
    pub path: String,
    pub name: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub matched_terms: Vec<String>,
    pub matched_intents: Vec<String>,
    pub duplicate_group_id: String,
    pub evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discover_score: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub queries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_query_rank: Option<usize>,
}

pub fn search(root: &Path, query: &str, options: SearchOptions) -> Result<Vec<SearchCandidate>> {
    let index_path = root.join(".jikji/search_index.sqlite");
    let con = Connection::open(&index_path).map_err(|source| sqlite_error(&index_path, source))?;
    if !schema_matches(&con)? {
        return Ok(Vec::new());
    }
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut scores = BTreeMap::<i64, f64>::new();
    let mut matched = TermMap::new();
    let mut reasons = TermMap::new();
    score_filename_hits(&con, &terms, &mut scores, &mut matched, &mut reasons)?;
    score_field_hits(&con, &terms, &mut scores, &mut matched, &mut reasons)?;
    let mut out = candidates_from_scores(&con, query, scores, matched, reasons)?;
    if out.is_empty() {
        out = fallback_scan_docs(&con, query)?;
    }
    out.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(out.into_iter().take(options.top_k.max(1)).collect())
}

fn schema_matches(con: &Connection) -> Result<bool> {
    match con.query_row(
        "SELECT value FROM meta WHERE key='schema_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(schema) => Ok(schema == SEARCH_INDEX_SCHEMA_VERSION.to_string()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(source) => Err(sqlite_error(Path::new("search_index.sqlite"), source)),
    }
}

fn candidates_from_scores(
    con: &Connection,
    query: &str,
    scores: ScoreMap,
    mut matched: TermMap,
    mut reasons: TermMap,
) -> Result<Vec<SearchCandidate>> {
    let mut out = Vec::new();
    for (doc_id, base_score) in scores {
        if base_score <= 0.0 {
            continue;
        }
        let doc = load_doc(con, doc_id)?;
        let (map_score, map_reasons, map_terms) =
            map_rescore::rescore(query, &doc.row_json, base_score);
        let score = base_score + map_score;
        let mut doc_reasons = reasons.remove(&doc_id).unwrap_or_default();
        doc_reasons.extend(map_reasons);
        let mut doc_matched = matched.remove(&doc_id).unwrap_or_default();
        doc_matched.extend(map_terms);
        out.push(SearchCandidate {
            path: doc.path,
            name: doc.name,
            score: round3(score),
            reasons: doc_reasons.into_iter().collect(),
            matched_terms: doc_matched.into_iter().take(16).collect(),
            matched_intents: Vec::new(),
            duplicate_group_id: doc.duplicate_group_id,
            evidence: doc.evidence,
            discover_score: None,
            queries: Vec::new(),
            best_query_rank: None,
        });
    }
    Ok(out)
}

struct DocRecord {
    path: String,
    name: String,
    duplicate_group_id: String,
    evidence: Vec<String>,
    row_json: Value,
}

fn load_doc(con: &Connection, doc_id: i64) -> Result<DocRecord> {
    con.query_row(
        "SELECT path,name,duplicate_group_id,row_json FROM docs WHERE id=?",
        params![doc_id],
        |row| {
            let raw: String = row.get(3)?;
            let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            let evidence = value
                .get("evidence")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            Ok(DocRecord {
                path: row.get(0)?,
                name: row.get(1)?,
                duplicate_group_id: row.get(2)?,
                evidence,
                row_json: value,
            })
        },
    )
    .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))
}

fn fallback_scan_docs(con: &Connection, query: &str) -> Result<Vec<SearchCandidate>> {
    let query_tokens = tokens(query, 32);
    let mut stmt = con
        .prepare("SELECT path,name,duplicate_group_id,row_json FROM docs")
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    let mut out = Vec::new();
    for row in rows {
        let (path, name, duplicate_group_id, raw) =
            row.map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
        let haystack = raw.to_lowercase();
        let hits = query_tokens
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if hits.is_empty() {
            continue;
        }
        out.push(SearchCandidate {
            path,
            name,
            score: hits.len() as f64 * 10.0,
            reasons: vec!["body-coverage".to_owned()],
            matched_terms: hits,
            matched_intents: Vec::new(),
            duplicate_group_id,
            evidence: Vec::new(),
            discover_score: None,
            queries: Vec::new(),
            best_query_rank: None,
        });
    }
    Ok(out)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
