use std::fs;

use serde_json::Value;

use super::support::{json_cmd, root_arg, temp_root};

#[test]
fn clean_success_json_uses_python_key_set() {
    let root = temp_root("task8-clean");
    fs::write(root.join("keep.txt"), "original").expect("write keep");
    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    let dry = json_cmd(&["clean", &root_arg, "--dry-run", "--json"]);
    assert!(dry.get("error").is_none());
    assert_eq!(dry["agent_rules_edited"], Value::Array(Vec::new()));

    let cleaned = json_cmd(&["clean", &root_arg, "--json"]);
    assert!(cleaned.get("error").is_none());
    assert_eq!(cleaned["agent_rules_edited"], Value::Array(Vec::new()));
}

#[test]
fn clean_removes_owned_generated_artifacts_and_preserves_user_jikji_files() {
    let root = temp_root("task8-clean-owned-artifacts");
    fs::write(root.join("keep.txt"), "original").expect("write keep");
    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    let user_file = root.join(".jikji/user-note.txt");
    fs::write(&user_file, "do not remove").expect("write user file");

    let cleaned = json_cmd(&["clean", &root_arg, "--json"]);
    assert_eq!(cleaned["ok"], true);
    assert!(user_file.exists());
    assert!(root.join("keep.txt").exists());
    assert!(!root.join(".jikji/manifest.json").exists());
    assert!(!root.join(".jikji/wiki/sources").exists());
    assert!(!root.join(".jikji/doc_text").exists());
    assert!(!root.join(".jikji/doc_meta").exists());
}

#[test]
#[cfg(unix)]
fn prepare_replaces_symlinked_jikji_dir_without_touching_target() {
    use std::os::unix::fs as unix_fs;

    let root = temp_root("task8-symlink-root");
    let outside = temp_root("task8-symlink-root-outside");
    fs::write(root.join("note.txt"), "symlink root marker").expect("write note");
    unix_fs::symlink(&outside, root.join(".jikji")).expect("symlink .jikji");

    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    assert!(root.join(".jikji/manifest.json").exists());
    assert!(
        !fs::symlink_metadata(root.join(".jikji"))
            .expect("metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(outside.read_dir().expect("outside read").count(), 0);
}

#[test]
#[cfg(unix)]
fn prepare_replaces_symlinked_generated_subdirs_without_external_write_or_delete() {
    use std::os::unix::fs as unix_fs;

    let root = temp_root("task8-symlink-subdirs");
    let outside_text = temp_root("task8-symlink-text-outside");
    let outside_meta = temp_root("task8-symlink-meta-outside");
    let outside_sources = temp_root("task8-symlink-sources-outside");
    fs::create_dir_all(root.join(".jikji/wiki")).expect("create wiki");
    fs::write(outside_text.join("sentinel.txt"), "keep").expect("text sentinel");
    fs::write(outside_meta.join("sentinel.json"), "keep").expect("meta sentinel");
    fs::write(outside_sources.join("sentinel.md"), "keep").expect("source sentinel");
    unix_fs::symlink(&outside_text, root.join(".jikji/doc_text")).expect("doc_text symlink");
    unix_fs::symlink(&outside_meta, root.join(".jikji/doc_meta")).expect("doc_meta symlink");
    unix_fs::symlink(&outside_sources, root.join(".jikji/wiki/sources")).expect("sources symlink");
    fs::write(root.join("mail.eml"), "Subject: Safe\n\nbody token").expect("write eml");

    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    for rel in [".jikji/doc_text", ".jikji/doc_meta", ".jikji/wiki/sources"] {
        assert!(
            !fs::symlink_metadata(root.join(rel))
                .expect("metadata")
                .file_type()
                .is_symlink()
        );
    }
    assert!(outside_text.join("sentinel.txt").exists());
    assert!(outside_meta.join("sentinel.json").exists());
    assert!(outside_sources.join("sentinel.md").exists());
    assert_eq!(outside_text.read_dir().expect("text read").count(), 1);
    assert_eq!(outside_meta.read_dir().expect("meta read").count(), 1);
    assert_eq!(outside_sources.read_dir().expect("sources read").count(), 1);
    assert!(
        root.join(".jikji/doc_text")
            .read_dir()
            .expect("text cache")
            .count()
            > 0
    );
    assert!(
        root.join(".jikji/doc_meta")
            .read_dir()
            .expect("meta cache")
            .count()
            > 0
    );
    assert!(
        root.join(".jikji/wiki/sources")
            .read_dir()
            .expect("sources")
            .count()
            > 0
    );
}
