use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jikji_core::Result;
use rusqlite::{Connection, params};

use crate::SEARCH_INDEX_SCHEMA_VERSION;
use crate::index_rows::{IndexRow, fielded_terms, row_terms};
use crate::io::sqlite_error;

pub(crate) fn write_sqlite(path: &Path, rows: &[IndexRow]) -> Result<()> {
    let mut con = Connection::open(path).map_err(|source| sqlite_error(path, source))?;
    con.execute_batch(
        "PRAGMA journal_mode=OFF;
         PRAGMA synchronous=OFF;
         CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE docs(id INTEGER PRIMARY KEY, path TEXT NOT NULL, name TEXT NOT NULL, ext TEXT NOT NULL, duplicate_group_id TEXT NOT NULL, row_json TEXT NOT NULL);
         CREATE TABLE terms(term TEXT NOT NULL, doc_id INTEGER NOT NULL);
         CREATE TABLE filename_keys(key TEXT NOT NULL, doc_id INTEGER NOT NULL);
         CREATE TABLE idf(term TEXT PRIMARY KEY, value REAL NOT NULL);
         CREATE TABLE field_terms(term TEXT NOT NULL, field TEXT NOT NULL, doc_id INTEGER NOT NULL, tf INTEGER NOT NULL);
         CREATE TABLE field_lengths(doc_id INTEGER NOT NULL, field TEXT NOT NULL, length INTEGER NOT NULL);
         CREATE TABLE field_idf(term TEXT PRIMARY KEY, value REAL NOT NULL);
         CREATE TABLE field_avg(field TEXT PRIMARY KEY, value REAL NOT NULL);",
    )
    .map_err(|source| sqlite_error(path, source))?;
    let tx = con
        .transaction()
        .map_err(|source| sqlite_error(path, source))?;
    let mut stats = SqliteStats::default();
    for (doc_idx, row) in rows.iter().enumerate() {
        let doc_id = i64::try_from(doc_idx + 1).unwrap_or(i64::MAX);
        insert_doc(path, &tx, doc_id, row)?;
        insert_terms(path, &tx, doc_id, row, &mut stats)?;
        insert_filename_keys(path, &tx, doc_id, row)?;
        insert_field_terms(path, &tx, doc_id, row, &mut stats)?;
    }
    insert_stats(path, &tx, rows, stats)?;
    tx.commit().map_err(|source| sqlite_error(path, source))?;
    con.execute_batch(
        "CREATE INDEX idx_terms_term ON terms(term);
         CREATE INDEX idx_filename_keys_key ON filename_keys(key);
         CREATE INDEX idx_field_terms_term ON field_terms(term);
         CREATE INDEX idx_field_terms_doc ON field_terms(doc_id);",
    )
    .map_err(|source| sqlite_error(path, source))
}

#[derive(Default)]
struct SqliteStats {
    df: BTreeMap<String, usize>,
    field_df: BTreeMap<String, usize>,
    field_len_totals: BTreeMap<String, usize>,
    term_rows: usize,
}

fn insert_doc(
    path: &Path,
    tx: &rusqlite::Transaction<'_>,
    doc_id: i64,
    row: &IndexRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO docs(id,path,name,ext,duplicate_group_id,row_json) VALUES(?,?,?,?,?,?)",
        params![
            doc_id,
            row.path,
            row.name,
            row.ext,
            row.duplicate_group_id,
            serde_json::to_string(&row.row_json).unwrap_or_default()
        ],
    )
    .map_err(|source| sqlite_error(path, source))?;
    Ok(())
}

fn insert_terms(
    path: &Path,
    tx: &rusqlite::Transaction<'_>,
    doc_id: i64,
    row: &IndexRow,
    stats: &mut SqliteStats,
) -> Result<()> {
    for term in row_terms(row) {
        stats.term_rows += 1;
        *stats.df.entry(term.clone()).or_insert(0) += 1;
        tx.execute(
            "INSERT INTO terms(term,doc_id) VALUES(?,?)",
            params![term, doc_id],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    Ok(())
}

fn insert_filename_keys(
    path: &Path,
    tx: &rusqlite::Transaction<'_>,
    doc_id: i64,
    row: &IndexRow,
) -> Result<()> {
    for key in &row.filename_keys {
        tx.execute(
            "INSERT INTO filename_keys(key,doc_id) VALUES(?,?)",
            params![key, doc_id],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    Ok(())
}

fn insert_field_terms(
    path: &Path,
    tx: &rusqlite::Transaction<'_>,
    doc_id: i64,
    row: &IndexRow,
    stats: &mut SqliteStats,
) -> Result<()> {
    let mut seen_field_terms = BTreeSet::new();
    for (field, counts) in fielded_terms(row) {
        let field_len = counts.iter().map(|(_, count)| *count).sum::<usize>();
        *stats.field_len_totals.entry(field.to_owned()).or_insert(0) += field_len;
        tx.execute(
            "INSERT INTO field_lengths(doc_id,field,length) VALUES(?,?,?)",
            params![doc_id, field, i64::try_from(field_len).unwrap_or(i64::MAX)],
        )
        .map_err(|source| sqlite_error(path, source))?;
        for (term, tf) in counts {
            seen_field_terms.insert(term.clone());
            tx.execute(
                "INSERT INTO field_terms(term,field,doc_id,tf) VALUES(?,?,?,?)",
                params![term, field, doc_id, i64::try_from(tf).unwrap_or(i64::MAX)],
            )
            .map_err(|source| sqlite_error(path, source))?;
        }
    }
    for term in seen_field_terms {
        *stats.field_df.entry(term).or_insert(0) += 1;
    }
    Ok(())
}

fn insert_stats(
    path: &Path,
    tx: &rusqlite::Transaction<'_>,
    rows: &[IndexRow],
    stats: SqliteStats,
) -> Result<()> {
    let total = rows.len().max(1) as f64;
    for (term, freq) in stats.df {
        let value = ((1.0 + total) / (1.0 + freq as f64)).ln() + 1.0;
        tx.execute(
            "INSERT INTO idf(term,value) VALUES(?,?)",
            params![term, value],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    for (term, freq) in stats.field_df {
        let value = ((total - freq as f64 + 0.5) / (freq as f64 + 0.5) + 1.0).ln();
        tx.execute(
            "INSERT INTO field_idf(term,value) VALUES(?,?)",
            params![term, value],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    for (field, total_len) in stats.field_len_totals {
        tx.execute(
            "INSERT INTO field_avg(field,value) VALUES(?,?)",
            params![field, total_len as f64 / total],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    for (key, value) in [
        ("schema_version", SEARCH_INDEX_SCHEMA_VERSION.to_string()),
        ("rows", rows.len().to_string()),
        ("terms", stats.term_rows.to_string()),
    ] {
        tx.execute(
            "INSERT INTO meta(key,value) VALUES(?,?)",
            params![key, value],
        )
        .map_err(|source| sqlite_error(path, source))?;
    }
    Ok(())
}
