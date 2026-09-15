use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use jikji_search::{SearchOptions, search};

static DATA_DIR_LOCK: Mutex<()> = Mutex::new(());
#[test]
fn search_skips_root_local_legacy_sqlite_for_registered_roots() {
    let _data = IsolatedData::new("corrupt-sqlite");
    let root = temp_root("corrupt-sqlite");
    let index_dir = root.join(".jikji");
    fs::create_dir_all(&index_dir).expect("create index dir");
    fs::write(
        index_dir.join("search_index.sqlite"),
        b"not a sqlite database",
    )
    .expect("write corrupted sqlite");

    let hits = search(&root, "ACME", SearchOptions { top_k: 3 })
        .expect("search must not attach root-local sqlite");
    assert!(hits.is_empty(), "{hits:?}");
    let _ = jikji_core::storage::delete_root(&root);
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

struct IsolatedData {
    path: PathBuf,
    previous: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl IsolatedData {
    fn new(label: &str) -> Self {
        let lock = DATA_DIR_LOCK.lock().expect("data dir lock");
        let previous = std::env::var_os("JIKJI_DATA_DIR");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "jikji-search-data-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create isolated data dir");
        unsafe {
            std::env::set_var("JIKJI_DATA_DIR", &path);
        }
        Self {
            path,
            previous,
            _lock: lock,
        }
    }
}

impl Drop for IsolatedData {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var("JIKJI_DATA_DIR", value),
                None => std::env::remove_var("JIKJI_DATA_DIR"),
            }
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}
