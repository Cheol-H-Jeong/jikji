use std::fs;
use std::sync::{Mutex, MutexGuard};

use jikji_core::PrepareOptions;
use jikji_core::storage::{clear_artifact, load_artifacts, open_database};
use jikji_index::{CleanOptions, clean, doctor, prepare, reindex_search};
use jikji_search::{IndexStatus, search_index_status};

static DATA_DIR_LOCK: Mutex<()> = Mutex::new(());

fn isolate_data_dir() -> (MutexGuard<'static, ()>, tempfile::TempDir) {
    let guard = DATA_DIR_LOCK.lock().expect("data dir lock");
    let data = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("JIKJI_DATA_DIR", data.path()) };
    (guard, data)
}

#[test]
fn library_prepare_persists_documents_and_clean_removes_central_root() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock, _data) = isolate_data_dir();
    fs::write(temp.path().join("document.txt"), "parser body marker").unwrap();

    let result = prepare(temp.path(), &PrepareOptions::default()).unwrap();
    assert_eq!(result.files, 1);
    assert!(!temp.path().join(".jikji").exists());
    let files = load_artifacts(temp.path(), "files").unwrap();
    assert_eq!(files[0]["path"], "document.txt");
    assert!(doctor(temp.path()).unwrap().ok);

    let cleaned = clean(
        temp.path(),
        CleanOptions {
            dry_run: false,
            force: false,
        },
    )
    .unwrap();
    assert!(cleaned.ok);
    let connection = open_database().unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM roots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert!(temp.path().join("document.txt").is_file());
}

#[test]
fn search_only_rebuilds_search_docs_from_existing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock, _data) = isolate_data_dir();
    fs::write(temp.path().join("resume.txt"), "정철현 이력서 body").unwrap();

    prepare(temp.path(), &PrepareOptions::default()).unwrap();
    let connection = open_database().unwrap();
    let root_id: i64 = connection
        .query_row("SELECT id FROM roots LIMIT 1", [], |row| row.get(0))
        .unwrap();
    connection
        .execute("DELETE FROM search_docs WHERE root_id=?1", [root_id])
        .unwrap();
    let empty: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_docs WHERE root_id=?1",
            [root_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(empty, 0);

    let stats = reindex_search(temp.path()).unwrap();
    assert!(stats.rows >= 1, "search rows {}", stats.rows);
    let restored: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_docs WHERE root_id=?1",
            [root_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(restored >= 1, "restored search_docs {restored}");
    clear_artifact(temp.path(), "manifest").unwrap();
    let status = search_index_status(temp.path(), 86_400);
    assert_eq!(status.status, IndexStatus::Ready);
    assert!(!status.should_prepare);
}

#[test]
fn search_only_refuses_empty_files_artifacts_without_wiping_search_docs() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock, _data) = isolate_data_dir();
    fs::write(temp.path().join("resume.txt"), "정철현 이력서 body").unwrap();

    prepare(temp.path(), &PrepareOptions::default()).unwrap();
    let connection = open_database().unwrap();
    let root_id: i64 = connection
        .query_row("SELECT id FROM roots LIMIT 1", [], |row| row.get(0))
        .unwrap();
    let before: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_docs WHERE root_id=?1",
            [root_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(before >= 1, "prepared search_docs {before}");

    clear_artifact(temp.path(), "files").unwrap();
    let error = reindex_search(temp.path()).expect_err("empty files artifacts");
    assert!(
        error
            .to_string()
            .contains("search-only requires existing files artifacts"),
        "{error}"
    );
    let after: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_docs WHERE root_id=?1",
            [root_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn search_index_status_ready_without_canonicalizing_missing_registered_root() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock, _data) = isolate_data_dir();
    fs::write(temp.path().join("resume.txt"), "정철현 이력서 body").unwrap();

    prepare(temp.path(), &PrepareOptions::default()).unwrap();
    let connection = open_database().unwrap();
    let missing = temp.path().join("missing-GoogleDrive/마커");
    assert!(!missing.exists());
    connection
        .execute(
            "UPDATE roots SET canonical_root=?1",
            [missing.to_string_lossy().as_ref()],
        )
        .unwrap();
    clear_artifact(&missing, "manifest").unwrap();
    let status = search_index_status(&missing, 86_400);
    assert_eq!(status.status, IndexStatus::Ready);
    assert!(!status.should_prepare);
}

#[cfg(unix)]
#[test]
fn search_only_finds_registered_root_via_symlink_without_canonicalizing() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock, _data) = isolate_data_dir();
    fs::write(temp.path().join("resume.txt"), "정철현 이력서 body").unwrap();

    prepare(temp.path(), &PrepareOptions::default()).unwrap();
    let connection = open_database().unwrap();
    let root_id: i64 = connection
        .query_row("SELECT id FROM roots LIMIT 1", [], |row| row.get(0))
        .unwrap();
    let target = temp.path().join("resolved-GoogleDrive/마커");
    let link = temp.path().join("Drive");
    assert!(!target.exists());
    std::os::unix::fs::symlink(&target, &link).unwrap();
    connection
        .execute(
            "UPDATE roots SET canonical_root=?1",
            [target.to_string_lossy().as_ref()],
        )
        .unwrap();
    clear_artifact(&link, "manifest").unwrap();
    let status = search_index_status(&link, 86_400);
    assert_eq!(status.status, IndexStatus::Ready);
    assert!(!status.should_prepare);

    let stats = reindex_search(&link).unwrap();
    assert!(stats.rows >= 1, "search rows {}", stats.rows);
    let same_id: i64 = connection
        .query_row(
            "SELECT id FROM roots WHERE canonical_root=?1",
            [target.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(same_id, root_id);
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM roots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
