use std::fs;

#[path = "task3_prepare_clean_doctor_map_contract/helpers.rs"]
mod helpers;

use helpers::{
    assert_file_index_contract, assert_manifest_contract, assert_path_list_contains,
    assert_path_list_excludes, assert_prepare_core_fields,
    assert_task3_artifacts_match_python_names, golden_artifact_paths, golden_command, json_cmd,
    json_file, minimal_png, path_string_ends_with, root_arg, run, run_ok, temp_root,
    write_ascii_cjk_fixture,
};

#[test]
fn prepare_refresh_doctor_and_map_match_python_task_three_contract_fields() {
    let root = temp_root("task3-golden-ascii-cjk");
    write_ascii_cjk_fixture(&root);

    let expected_prepare = golden_command("ascii_cjk_paths", "prepare")["stdout_json"].clone();
    let root_arg = root_arg(&root);
    let prepared = json_cmd(&["prepare", &root_arg, "--json"]);

    assert_prepare_core_fields(&prepared, &expected_prepare);
    assert!(
        prepared["index_dir"]
            .as_str()
            .expect("index_dir")
            .ends_with(".jikji")
    );
    assert!(path_string_ends_with(
        prepared["agent_map"].as_str().expect("agent_map"),
        ".jikji/agent_map.md"
    ));
    assert_task3_artifacts_match_python_names(&root, "ascii_cjk_paths");
    assert_manifest_contract(&root);
    assert_file_index_contract(&root);

    let expected_doctor = golden_command("ascii_cjk_paths", "doctor")["stdout_json"].clone();
    let doctor = json_cmd(&["doctor", &root_arg, "--json"]);
    assert_eq!(doctor["ok"], expected_doctor["ok"]);
    assert_eq!(doctor["errors"], expected_doctor["errors"]);
    assert_eq!(
        doctor["manifest"]["schema_version"],
        expected_doctor["manifest"]["schema_version"]
    );
    assert_eq!(
        doctor["manifest"]["search_index_schema_version"],
        expected_doctor["manifest"]["search_index_schema_version"]
    );
    assert_eq!(doctor["manifest"]["non_destructive"], true);

    let expected_map = golden_command("ascii_cjk_paths", "map");
    let map = run_ok(&["map", &root_arg]);
    let map_text = String::from_utf8(map.stdout).expect("map utf8");
    assert!(map_text.contains("# Jikji Agent Map"));
    assert!(map_text.contains("docs/acme-contract.txt"));
    assert!(
        expected_map["stdout"]
            .as_str()
            .expect("golden map stdout")
            .contains("# Jikji Agent Map")
    );

    fs::remove_file(root.join("docs/acme-contract.txt")).expect("delete source");
    let refreshed = json_cmd(&["refresh", &root_arg, "--json"]);
    assert_eq!(refreshed["deleted"], 1);
    assert!(
        root.join("\u{c790}\u{b8cc}/\u{d68c}\u{c758}\u{b85d}.txt")
            .exists()
    );
}

#[test]
fn clean_matches_python_safety_contract_and_preserves_user_files() {
    let root = temp_root("task3-golden-clean");
    fs::write(
        root.join("keep.txt"),
        "original file must survive clean safety",
    )
    .expect("write keep");
    let root_arg = root_arg(&root);

    let expected_prepare = golden_command("clean_safety", "prepare")["stdout_json"].clone();
    let prepared = json_cmd(&["prepare", &root_arg, "--json"]);
    assert_prepare_core_fields(&prepared, &expected_prepare);

    fs::write(
        root.join(".jikji/user-created-note.txt"),
        "user file inside .jikji must survive",
    )
    .expect("write user note");
    let dry = json_cmd(&["clean", &root_arg, "--dry-run", "--json"]);
    assert_eq!(dry["ok"], true);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["reason"], "manifest_verified");
    assert_eq!(dry["preserved_original_files"], true);
    assert_path_list_contains(&dry["would_remove"], ".jikji/manifest.json");
    assert_path_list_contains(&dry["would_remove"], ".jikji/agent_map.md");
    assert_path_list_excludes(&dry["would_remove"], "user-created-note.txt");

    let cleaned = json_cmd(&["clean", &root_arg, "--json"]);
    assert_eq!(cleaned["ok"], true);
    assert_eq!(cleaned["dry_run"], false);
    assert!(root.join("keep.txt").exists());
    assert!(root.join(".jikji/user-created-note.txt").exists());
    assert!(!root.join(".jikji/manifest.json").exists());
    assert_eq!(
        golden_artifact_paths("clean_safety"),
        [".jikji/user-created-note.txt".to_owned()].into()
    );
}

#[test]
fn max_files_sensitive_skip_stale_lock_and_user_only_clean_contracts_hold() {
    let no_cap = temp_root("task3-no-cap");
    for idx in 0..5001 {
        fs::write(no_cap.join(format!("bulk_{idx:04}.bin")), []).expect("write bulk");
    }
    let no_cap_arg = root_arg(&no_cap);
    let prepared = json_cmd(&["prepare", &no_cap_arg, "--json"]);
    assert_eq!(prepared["files"], 5001);

    let capped = temp_root("task3-capped");
    for idx in 0..4 {
        fs::write(capped.join(format!("bulk_{idx}.txt")), "x").expect("write capped");
    }
    let capped_arg = root_arg(&capped);
    let partial = json_cmd(&["prepare", &capped_arg, "--max-files", "3", "--json"]);
    assert_eq!(partial["files"], 3);
    assert_eq!(json_file(capped.join(".jikji/manifest.json"))["files"], 3);

    let locked = temp_root("task3-stale-lock");
    fs::write(locked.join("photo.png"), minimal_png()).expect("write png");
    fs::create_dir(locked.join(".jikji")).expect("create sidecar");
    fs::write(
        locked.join(".jikji/.lock"),
        r#"{"pid":1,"started_at_unix":1}"#,
    )
    .expect("write stale lock");
    let locked_arg = root_arg(&locked);
    let media = json_cmd(&["prepare", &locked_arg, "--enable-media-index", "--json"]);
    assert_eq!(media["files"], 1);
    let manifest = json_file(locked.join(".jikji/manifest.json"));
    assert_eq!(manifest["media_index"]["enabled"], true);
    assert_eq!(manifest["media_index"]["status"], "enabled_bounded");
    assert!(!locked.join(".jikji/.lock").exists());

    let user_only = temp_root("task3-user-only-clean");
    fs::create_dir(user_only.join(".jikji")).expect("create user sidecar");
    fs::write(user_only.join(".jikji/user-created-note.txt"), "user").expect("write note");
    let user_only_arg = root_arg(&user_only);
    let refused = run(&["clean", &user_only_arg, "--json"]);
    assert!(!refused.status.success());
    assert!(user_only.join(".jikji/user-created-note.txt").exists());
}
