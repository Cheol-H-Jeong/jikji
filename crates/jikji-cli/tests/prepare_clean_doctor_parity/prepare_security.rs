#[cfg(unix)]
use std::os::unix::fs as unix_fs;

#[cfg(unix)]
#[test]
fn prepare_replaces_symlinked_doc_cache_leaf_files_without_touching_targets() {
    use std::fs;

    use super::{json_cmd, json_file, root_str, temp_root};

    let root = temp_root("doc-cache-leaf-symlink");
    fs::write(root.join("probe.pdf"), "probe document\n").expect("write probe");
    fs::create_dir_all(root.join(".jikji/doc_text")).expect("create doc text");
    fs::create_dir_all(root.join(".jikji/doc_meta")).expect("create doc meta");
    let outside = temp_root("doc-cache-leaf-outside");
    let outside_text = outside.join("outside.txt");
    let outside_meta = outside.join("outside.json");
    fs::write(&outside_text, "outside text sentinel").expect("write outside text");
    fs::write(&outside_meta, "outside meta sentinel").expect("write outside meta");
    let digest = "a0633ba1c684a9fdde66555a13bfe75e7054791ce95e4b46161c70c4995e738d";
    let text_leaf = root.join(format!(".jikji/doc_text/sha256_{digest}.txt"));
    let meta_leaf = root.join(format!(".jikji/doc_meta/sha256_{digest}.json"));
    unix_fs::symlink(&outside_text, &text_leaf).expect("symlink text leaf");
    unix_fs::symlink(&outside_meta, &meta_leaf).expect("symlink meta leaf");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--no-agent-rules",
        "--json",
    ]);

    assert_eq!(prepared["files"], 1);
    assert_eq!(
        fs::read_to_string(outside_text).expect("read outside text"),
        "outside text sentinel"
    );
    assert_eq!(
        fs::read_to_string(outside_meta).expect("read outside meta"),
        "outside meta sentinel"
    );
    assert!(
        !fs::symlink_metadata(&text_leaf)
            .expect("text metadata")
            .file_type()
            .is_symlink()
    );
    assert!(
        !fs::symlink_metadata(&meta_leaf)
            .expect("meta metadata")
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_to_string(text_leaf)
            .expect("read generated text")
            .contains("# Source: probe.pdf")
    );
    assert_eq!(json_file(meta_leaf)["path"], "probe.pdf");
}

#[cfg(unix)]
#[test]
fn prepare_replaces_symlinked_doc_cache_chunk_dirs_without_touching_targets() {
    use std::fs;

    use super::{json_cmd, root_str, temp_root};

    let root = temp_root("doc-cache-dir-symlink");
    let body = format!("{{\\rtf1 {}}}", "needle ".repeat(60));
    fs::write(root.join("long.rtf"), &body).expect("write rtf");
    fs::create_dir_all(root.join(".jikji/doc_text")).expect("create doc text");
    let outside = temp_root("doc-cache-dir-outside");
    fs::write(outside.join("sentinel.txt"), "outside sentinel").expect("write outside sentinel");
    let digest = "89e1f0d17570b22d5d1345341b6c201e315fcbffc51649938be66c8d320b5feb";
    let cache_dir = root.join(format!(".jikji/doc_text/sha256_{digest}"));
    unix_fs::symlink(&outside, &cache_dir).expect("symlink cache dir");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--doc-text-max-chars",
        "120",
        "--doc-text-chunk-chars",
        "40",
        "--no-agent-rules",
        "--json",
    ]);

    assert_eq!(prepared["files"], 1);
    assert_eq!(
        fs::read_to_string(outside.join("sentinel.txt")).expect("read outside sentinel"),
        "outside sentinel"
    );
    assert!(
        !fs::symlink_metadata(&cache_dir)
            .expect("cache metadata")
            .file_type()
            .is_symlink()
    );
    assert!(cache_dir.join("chunk_0001.txt").exists());
    assert!(!outside.join("chunk_0001.txt").exists());
}

#[cfg(unix)]
#[test]
fn search_background_refresh_refuses_symlinked_jikji_dir_without_external_log_write() {
    use std::fs;

    use super::{json_cmd, root_str, run, temp_root};

    let root = temp_root("search-refresh-jikji-symlink");
    fs::write(root.join("needle.txt"), "needle").expect("write needle");
    json_cmd(["prepare", root_str(&root).as_str(), "--json"]);
    let backup = root.join(".jikji_real");
    fs::rename(root.join(".jikji"), &backup).expect("move real index");
    let outside = temp_root("search-refresh-outside");
    unix_fs::symlink(&outside, root.join(".jikji")).expect("symlink .jikji");

    let output = run(["search", root_str(&root).as_str(), "needle", "--json"]);

    if output.status.success() {
        let payload: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
        assert_eq!(payload["background_refresh_started"], false);
    }
    assert!(!outside.join("background_refresh.log").exists());
    assert!(backup.join("search_index.sqlite").exists());
}

#[cfg(unix)]
#[test]
fn search_background_refresh_does_not_follow_symlinked_log_leaf() {
    use std::fs;

    use super::{json_cmd, root_str, temp_root};

    let root = temp_root("search-refresh-log-leaf-symlink");
    fs::write(root.join("needle.txt"), "needle").expect("write needle");
    json_cmd(["prepare", root_str(&root).as_str(), "--json"]);
    fs::write(root.join("changed.txt"), "needle changed").expect("change tree");
    let outside = temp_root("search-refresh-log-leaf-outside");
    let outside_log = outside.join("external.log");
    fs::write(&outside_log, "outside sentinel").expect("write outside log");
    unix_fs::symlink(&outside_log, root.join(".jikji/background_refresh.log"))
        .expect("symlink log leaf");

    let payload = json_cmd(["search", root_str(&root).as_str(), "needle", "--json"]);

    assert_eq!(payload["background_refresh_started"], true);
    assert_eq!(
        fs::read_to_string(outside_log).expect("read outside log"),
        "outside sentinel"
    );
}
