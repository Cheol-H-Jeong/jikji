use std::fs;
use std::path::Path;

use serde_json::Value;

use super::{json_cmd, json_file, root_str, run, temp_root};

#[test]
fn stale_lock_media_policy_and_clean_safety_classes_are_covered() {
    let root = temp_root("safety");
    fs::write(root.join("photo.png"), minimal_png()).expect("write png");
    fs::create_dir_all(root.join(".jikji")).expect("create sidecar");
    fs::write(
        root.join(".jikji/.lock"),
        r#"{"pid":1,"started_at_unix":1}"#,
    )
    .expect("stale lock");
    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--enable-media-index",
        "--json",
    ]);
    assert_eq!(prepared["files"], 1);
    let manifest = json_file(root.join(".jikji/manifest.json"));
    assert_eq!(manifest["media_index"]["enabled"], true);
    assert_eq!(manifest["media_index"]["status"], "enabled_bounded");
    assert!(!root.join(".jikji/.lock").exists());

    let outside = temp_root("outside");
    fs::write(outside.join("outside.txt"), "outside").expect("write outside");
    replace_manifest_root(&root, &outside);
    let refused = run(["clean", root_str(&root).as_str(), "--json"]);
    assert!(!refused.status.success());
    assert!(outside.join("outside.txt").exists());
}

#[test]
fn clean_preserves_user_only_jikji_dir_without_verified_manifest() {
    let root = temp_root("user-only");
    fs::create_dir(root.join(".jikji")).expect("create sidecar");
    fs::write(root.join(".jikji/user-created-note.txt"), "user").expect("write note");

    let output = run(["clean", root_str(&root).as_str(), "--json"]);

    assert!(!output.status.success());
    assert!(root.join(".jikji/user-created-note.txt").exists());
}

fn replace_manifest_root(root: &Path, outside: &Path) {
    let path = root.join(".jikji/manifest.json");
    let mut manifest = json_file(path.clone());
    manifest["root"] = Value::String(outside.display().to_string());
    fs::write(
        path,
        serde_json::to_string_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

fn minimal_png() -> &'static [u8] {
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00IEND\xaeB`\x82"
}
