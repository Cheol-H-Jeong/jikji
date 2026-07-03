use std::fs;
use std::path::Path;

use jikji_core::{Result, io_error};
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
    index_dir: &Path,
    file_cards: &[Value],
    chunk_rows: &[Value],
) -> Result<BuildStats> {
    fs::create_dir_all(index_dir).map_err(|source| io_error(index_dir, source))?;
    let rows = rows_from_cards(index_dir, file_cards, chunk_rows);
    let path = index_dir.join("search_index.sqlite");
    let tmp = path.with_extension("sqlite.tmp");
    remove_previous_index_files(&tmp, &path)?;
    write_sqlite(&tmp, &rows)?;
    fs::rename(&tmp, &path).map_err(|source| io_error(&path, source))?;
    Ok(BuildStats {
        rows: rows.len(),
        terms: rows.iter().map(row_terms).map(|set| set.len()).sum(),
    })
}

fn remove_previous_index_files(tmp: &Path, path: &Path) -> Result<()> {
    for candidate in [tmp, path] {
        match fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error(candidate, source)),
        }
    }
    Ok(())
}
