use std::path::Path;

use jikji_core::Result;
use serde_json::Value;

use crate::indexer::{build_graph_artifacts, build_sqlite_index, rows_from_cards};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchArtifactStats {
    pub rows: usize,
    pub terms: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
}

pub fn build_search_artifacts(
    index_dir: &Path,
    file_cards: &[Value],
    chunk_rows: &[Value],
    folder_profiles: &[Value],
) -> Result<SearchArtifactStats> {
    let sqlite = build_sqlite_index(index_dir, file_cards, chunk_rows)?;
    let rows = rows_from_cards(index_dir, file_cards, chunk_rows);
    let (graph_nodes, graph_edges) = build_graph_artifacts(index_dir, &rows, folder_profiles)?;
    Ok(SearchArtifactStats {
        rows: sqlite.rows,
        terms: sqlite.terms,
        graph_nodes,
        graph_edges,
    })
}
