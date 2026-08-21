use std::path::Path;

use jikji_core::Result;
use serde_json::Value;

pub(crate) use crate::graph_artifacts::build_graph_artifacts;
use crate::index_rows::row_terms;
pub(crate) use crate::index_rows::{field_weight, rows_from_cards};
use crate::sqlite_index::write_sqlite;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuildStats {
    pub rows: usize,
    pub terms: usize,
}

pub(crate) fn build_sqlite_index(
    root: &Path,
    file_cards: &[Value],
    chunk_rows: &[Value],
) -> Result<BuildStats> {
    let rows = rows_from_cards(root, file_cards, chunk_rows);
    write_sqlite(root, &rows)?;
    Ok(BuildStats {
        rows: rows.len(),
        terms: rows.iter().map(row_terms).map(|set| set.len()).sum(),
    })
}
