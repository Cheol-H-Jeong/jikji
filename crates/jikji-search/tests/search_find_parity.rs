use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use jikji_search::{SearchOptions, search};

#[test]
fn search_reports_corrupted_sqlite_index_as_malformed_input() {
    let root = temp_root("corrupt-sqlite");
    let index_dir = root.join(".jikji");
    fs::create_dir_all(&index_dir).expect("create index dir");
    fs::write(
        index_dir.join("search_index.sqlite"),
        b"not a sqlite database",
    )
    .expect("write corrupted sqlite");

    let error = search(&root, "ACME", SearchOptions { top_k: 3 }).expect_err("malformed sqlite");
    assert!(error.to_string().contains("search_index.sqlite"));
}

struct TempRoot {
    path: PathBuf,
}

impl std::ops::Deref for TempRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn temp_root(label: &str) -> TempRoot {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "jikji-task5-search-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    TempRoot { path: root }
}
