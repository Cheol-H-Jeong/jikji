use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jikji_core::Result;
use rusqlite::{Connection, params};

use crate::indexer::field_weight;
use crate::io::sqlite_error;
use crate::tokenizer::filename_lookup_keys;

pub(crate) type ScoreMap = BTreeMap<i64, f64>;
pub(crate) type TermMap = BTreeMap<i64, BTreeSet<String>>;

pub(crate) fn score_filename_hits(
    con: &Connection,
    terms: &BTreeSet<String>,
    scores: &mut ScoreMap,
    matched: &mut TermMap,
    reasons: &mut TermMap,
) -> Result<()> {
    for term in terms {
        let mut keys = filename_lookup_keys(term);
        keys.push(term.clone());
        for key in keys {
            if !is_filename_anchor_key(&key) {
                continue;
            }
            score_filename_key(con, term, &key, scores, matched, reasons)?;
        }
    }
    Ok(())
}

fn is_filename_anchor_key(key: &str) -> bool {
    let chars = key.chars().collect::<Vec<_>>();
    if chars.len() >= 3 {
        return true;
    }
    chars.iter().any(|ch| !ch.is_ascii_alphabetic())
}

fn score_filename_key(
    con: &Connection,
    term: &str,
    key: &str,
    scores: &mut ScoreMap,
    matched: &mut TermMap,
    reasons: &mut TermMap,
) -> Result<()> {
    let like = format!("%{key}%");
    let mut stmt = con
        .prepare("SELECT doc_id,key FROM filename_keys WHERE key=? OR key LIKE ? LIMIT 200")
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    let rows = stmt
        .query_map(params![key, like], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    for row in rows {
        let (doc_id, hit_key) =
            row.map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
        let boost = if hit_key == key { 140.0 } else { 80.0 };
        *scores.entry(doc_id).or_insert(0.0) += boost + key.len() as f64;
        matched.entry(doc_id).or_default().insert(term.to_owned());
        reasons
            .entry(doc_id)
            .or_default()
            .insert("filename-anchor".to_owned());
    }
    Ok(())
}

pub(crate) fn score_field_hits(
    con: &Connection,
    terms: &BTreeSet<String>,
    scores: &mut ScoreMap,
    matched: &mut TermMap,
    reasons: &mut TermMap,
) -> Result<()> {
    let avg = field_avg(con)?;
    for term in terms {
        let idf = field_idf(con, term);
        let mut stmt = con
            .prepare(
                "SELECT field_terms.field, field_terms.doc_id, field_terms.tf,
                        COALESCE(field_lengths.length, 1)
                 FROM field_terms
                 LEFT JOIN field_lengths
                   ON field_lengths.doc_id = field_terms.doc_id
                  AND field_lengths.field = field_terms.field
                 WHERE field_terms.term = ?
                 LIMIT 5000",
            )
            .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
        let rows = stmt
            .query_map(params![term], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
        for row in rows {
            let row =
                row.map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
            score_field_row(row, term, idf, &avg, scores, matched, reasons);
        }
    }
    Ok(())
}

fn score_field_row(
    row: (String, i64, i64, i64),
    term: &str,
    idf: f64,
    avg: &BTreeMap<String, f64>,
    scores: &mut ScoreMap,
    matched: &mut TermMap,
    reasons: &mut TermMap,
) {
    let (field, doc_id, tf, length) = row;
    let len = length.max(1) as f64;
    let avg_len = avg.get(&field).copied().unwrap_or(1.0).max(1.0);
    let tf64 = tf.max(1) as f64;
    let denom = tf64 + 1.2 * (1.0 - 0.75 + 0.75 * (len / avg_len));
    let bm25 = idf * ((tf64 * 2.2) / denom);
    *scores.entry(doc_id).or_insert(0.0) += bm25 * field_weight(&field) * 100.0;
    matched.entry(doc_id).or_default().insert(term.to_owned());
    reasons
        .entry(doc_id)
        .or_default()
        .insert("fielded-bm25".to_owned());
}

fn field_idf(con: &Connection, term: &str) -> f64 {
    con.query_row(
        "SELECT value FROM field_idf WHERE term=?",
        params![term],
        |row| row.get(0),
    )
    .unwrap_or(1.0)
}

fn field_avg(con: &Connection) -> Result<BTreeMap<String, f64>> {
    let mut stmt = con
        .prepare("SELECT field,value FROM field_avg")
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })
        .map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (field, value) =
            row.map_err(|source| sqlite_error(Path::new("search_index.sqlite"), source))?;
        out.insert(field, value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_field_hits_joins_lengths_in_one_query() {
        let con = Connection::open_in_memory().expect("memory sqlite");
        con.execute_batch(
            "CREATE TABLE field_terms(term TEXT, field TEXT, doc_id INTEGER, tf INTEGER);
             CREATE TABLE field_lengths(doc_id INTEGER, field TEXT, length INTEGER);
             CREATE TABLE field_idf(term TEXT PRIMARY KEY, value REAL);
             CREATE TABLE field_avg(field TEXT PRIMARY KEY, value REAL);
             INSERT INTO field_terms VALUES ('hwp','name',1,2);
             INSERT INTO field_lengths VALUES (1,'name',4);
             INSERT INTO field_idf VALUES ('hwp', 1.5);
             INSERT INTO field_avg VALUES ('name', 4.0);",
        )
        .expect("schema");
        let terms = BTreeSet::from(["hwp".to_owned()]);
        let mut scores = ScoreMap::new();
        let mut matched = TermMap::new();
        let mut reasons = TermMap::new();
        score_field_hits(&con, &terms, &mut scores, &mut matched, &mut reasons).expect("score");
        assert!(
            scores.get(&1).copied().unwrap_or(0.0) > 0.0,
            "joined BM25 should score the matching doc: {scores:?}"
        );
        assert!(
            reasons
                .get(&1)
                .is_some_and(|set| set.contains("fielded-bm25"))
        );
    }
}
