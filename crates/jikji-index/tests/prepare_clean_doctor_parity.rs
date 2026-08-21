use std::fs;

use jikji_core::PrepareOptions;
use jikji_core::storage::{load_artifacts, open_database};
use jikji_index::{CleanOptions, clean, doctor, prepare};

#[test]
fn library_prepare_persists_documents_and_clean_removes_central_root() {
    let temp = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("JIKJI_DATA_DIR", data.path()) };
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
