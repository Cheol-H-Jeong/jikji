use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use jikji_core::storage::{load_artifact, replace_artifacts};
use serde_json::Value;

use super::{json_cmd, jsonl, root_str, run_ok, temp_root};

#[test]
fn prepare_search_find_brief_options_and_map_max_chars_follow_python_surface() {
    let root = temp_root("python-options");
    fs::create_dir_all(root.join("docs")).expect("create docs");
    fs::write(root.join("docs/needle.txt"), "needle alpha").expect("write target");
    fs::write(root.join("docs/ignored.skip"), "needle skip").expect("write ignored");
    fs::write(root.join(".secret.txt"), "needle hidden").expect("write hidden");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--exclude",
        "*.skip",
        "--max-hash-bytes",
        "1024",
        "--parse-timeout",
        "0.2",
        "--doc-text-max-chars",
        "120",
        "--doc-text-chunk-chars",
        "40",
        "--json",
    ]);
    assert_eq!(prepared["files"], 1);

    let search = json_cmd([
        "search",
        root_str(&root).as_str(),
        "needle",
        "--auto-prepare",
        "--max-files",
        "20",
        "--include-hidden",
        "--include-sensitive",
        "--exclude",
        "*.skip",
        "--max-hash-bytes",
        "1024",
        "--parse-timeout",
        "0.2",
        "--no-background-refresh",
        "--json",
    ]);
    assert_candidate_paths_exclude(&search["candidates"], "ignored.skip");

    let find = json_cmd([
        "find",
        root_str(&root).as_str(),
        "needle",
        "--include-hidden",
        "--include-sensitive",
        "--exclude",
        "*.skip",
        "--max-hash-bytes",
        "1024",
        "--parse-timeout",
        "0.2",
        "--no-background-refresh",
        "--json",
    ]);
    assert_candidate_paths_exclude(&find["candidates"], "ignored.skip");

    let brief = json_cmd([
        "brief",
        root_str(&root).as_str(),
        "needle",
        "--include-hidden",
        "--include-sensitive",
        "--exclude",
        "*.skip",
        "--max-hash-bytes",
        "1024",
        "--parse-timeout",
        "0.2",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(brief["root"], root_str(&root));

    let discover = json_cmd([
        "discover",
        root_str(&root).as_str(),
        "needle",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(discover["background_refresh_started"], false);

    let map = run_ok(["map", root_str(&root).as_str(), "--max-chars", "80"]);
    let map_text = String::from_utf8(map.stdout).expect("utf8 map");
    assert!(map_text.chars().count() <= 80);
    assert!(map_text.contains("Jikji"));
}

#[test]
fn search_and_brief_start_background_refresh_for_changed_indexes_by_default() {
    let root = temp_root("background-refresh");
    fs::write(root.join("needle.txt"), "needle alpha").expect("write target");
    json_cmd(["prepare", root_str(&root).as_str(), "--json"]);
    fs::write(root.join("new.txt"), "needle changed").expect("change tree");

    let search = json_cmd(["search", root_str(&root).as_str(), "needle", "--json"]);
    assert_eq!(search["index_status"], "changed_using_previous_index");
    assert_eq!(search["background_refresh_started"], true);
    assert!(!root.join(".jikji/background_refresh.log").exists());

    let quiet_root = temp_root("background-refresh-disabled");
    fs::write(quiet_root.join("needle.txt"), "needle alpha").expect("write target");
    json_cmd(["prepare", root_str(&quiet_root).as_str(), "--json"]);
    fs::write(quiet_root.join("new.txt"), "needle changed").expect("change tree");
    let quiet = json_cmd([
        "brief",
        root_str(&quiet_root).as_str(),
        "needle",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(quiet["background_refresh_started"], false);
}

#[test]
fn find_and_discover_start_background_refresh_by_default() {
    let root = temp_root("find-discover-background-refresh");
    fs::write(root.join("needle.txt"), "needle alpha").expect("write target");
    json_cmd(["prepare", root_str(&root).as_str(), "--json"]);
    fs::write(root.join("changed.txt"), "needle changed").expect("change tree");

    let find = json_cmd(["find", root_str(&root).as_str(), "needle", "--json"]);
    assert_eq!(find["index_status"], "changed_using_previous_index");
    assert_eq!(find["background_refresh_started"], true);

    let second = temp_root("discover-background-refresh");
    fs::write(second.join("needle.txt"), "needle alpha").expect("write target");
    json_cmd(["prepare", root_str(&second).as_str(), "--json"]);
    fs::write(second.join("changed.txt"), "needle changed").expect("change tree");
    let discover = json_cmd(["discover", root_str(&second).as_str(), "needle", "--json"]);
    assert_eq!(discover["index_status"], "changed_using_previous_index");
    assert_eq!(discover["background_refresh_started"], true);
}

#[test]
fn search_ttl_defaults_to_24_hours_and_accepts_override() {
    let root = temp_root("search-stale-ttl");
    fs::write(root.join("needle.txt"), "needle alpha").expect("write target");
    json_cmd(["prepare", root_str(&root).as_str(), "--json"]);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let mut manifest = load_artifact(&root, "manifest")
        .expect("load manifest")
        .expect("manifest");
    manifest["generated_at"] = serde_json::json!(now - 23 * 60 * 60);
    replace_artifacts(&root, &[("manifest", manifest.clone())]).expect("store manifest");

    let fresh = json_cmd([
        "search",
        root_str(&root).as_str(),
        "needle",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(fresh["index_status"], "ready");

    let overridden = json_cmd([
        "search",
        root_str(&root).as_str(),
        "needle",
        "--stale-after-seconds",
        "3600",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(overridden["index_status"], "stale_using_previous_index");

    manifest["generated_at"] = serde_json::json!(now - 25 * 60 * 60);
    replace_artifacts(&root, &[("manifest", manifest)]).expect("store stale manifest");
    let stale = json_cmd([
        "search",
        root_str(&root).as_str(),
        "needle",
        "--no-background-refresh",
        "--json",
    ]);
    assert_eq!(stale["index_status"], "stale_using_previous_index");
}

#[test]
fn prepare_doc_text_limits_and_chunk_map_follow_python_artifact_contract() {
    let root = temp_root("doc-chunks");
    let body = format!("{{\\rtf1 {}}}", "needle ".repeat(60));
    fs::write(root.join("long.rtf"), body).expect("write rtf");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--doc-text-max-chars",
        "120",
        "--doc-text-chunk-chars",
        "40",
        "--json",
    ]);
    assert_eq!(prepared["docs_parsed"], 1);

    let document_rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let cache_path = document_rows[0]["text_cache_path"]
        .as_str()
        .expect("text cache path");
    let cache_abs = root.join(cache_path);
    assert!(cache_abs.is_dir());
    assert!(
        fs::read_to_string(cache_abs.join("chunk_0001.txt"))
            .expect("read chunk")
            .contains("# Source: long.rtf")
    );

    let chunks = jsonl(root.join(".jikji/chunk_map.jsonl"));
    assert_eq!(
        chunks.len(),
        1,
        "chunk_map should chunk body text, not cache headers"
    );
    let row = &chunks[0];
    assert_eq!(row["path"], "long.rtf");
    assert_eq!(row["schema_version"], 1);
    assert_eq!(row["text_cache_path"], cache_path);
    assert_eq!(row["char_start"], 0);
    assert!(row["char_end"].as_u64().is_some_and(|value| value <= 120));
    assert!(row["chunk_id"].as_str().unwrap_or_default().contains(':'));
}

#[test]
fn prepare_doc_text_cache_path_uses_bounded_body_text_when_choosing_chunking() {
    let root = temp_root("doc-cache-bounded-path");
    let body = format!("{{\\rtf1 {}}}", "needle ".repeat(60));
    fs::write(root.join("bounded.rtf"), body).expect("write rtf");

    json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--doc-text-max-chars",
        "50",
        "--doc-text-chunk-chars",
        "80",
        "--json",
    ]);

    let document_rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let cache_path = document_rows[0]["text_cache_path"]
        .as_str()
        .expect("text cache path");
    assert!(
        cache_path.ends_with(".txt"),
        "bounded body should fit in a single cache file: {cache_path}"
    );
    assert!(root.join(cache_path).is_file());
    assert!(!root.join(cache_path.trim_end_matches(".txt")).exists());
}

#[test]
fn clean_removes_chunked_doc_text_cache_directories() {
    let root = temp_root("clean-doc-chunks");
    fs::write(
        root.join("long.rtf"),
        format!("{{\\rtf1 {}}}", "needle ".repeat(60)),
    )
    .expect("write rtf");
    json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--doc-text-max-chars",
        "120",
        "--doc-text-chunk-chars",
        "40",
        "--json",
    ]);
    let document_rows = jsonl(root.join(".jikji/document_index.jsonl"));
    let cache_path = document_rows[0]["text_cache_path"]
        .as_str()
        .expect("text cache path");
    assert!(root.join(cache_path).join("chunk_0001.txt").exists());

    let cleaned = json_cmd(["clean", root_str(&root).as_str(), "--json"]);

    assert_eq!(cleaned["ok"], true);
    assert!(!root.join(cache_path).exists());
}

#[test]
fn prepare_respects_max_hash_bytes_for_parser_required_documents() {
    let root = temp_root("doc-hash-limit");
    fs::write(
        root.join("oversize.rtf"),
        format!("{{\\rtf1 {}}}", "x".repeat(64)),
    )
    .expect("write rtf");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--max-hash-bytes",
        "1",
        "--json",
    ]);
    assert_eq!(prepared["docs_parsed"], 0);
    assert_eq!(prepared["docs_failed"], 1);

    let document_rows = jsonl(root.join(".jikji/document_index.jsonl"));
    assert_eq!(document_rows[0]["path"], "oversize.rtf");
    assert_eq!(document_rows[0]["sha256"], "");
    assert_eq!(document_rows[0]["parse_status"], "hash_oversize");
    assert_eq!(document_rows[0]["text_cache_path"], "");
    assert_eq!(jsonl(root.join(".jikji/chunk_map.jsonl")).len(), 0);
}

#[test]
fn prepare_enforces_parse_timeout_for_parser_required_documents() {
    let root = temp_root("doc-parse-timeout");
    fs::write(root.join("timeout.rtf"), r"{\rtf1 timeout body}").expect("write rtf");

    let prepared = json_cmd([
        "prepare",
        root_str(&root).as_str(),
        "--parse-timeout",
        "0",
        "--json",
    ]);
    assert_eq!(prepared["docs_parsed"], 0);
    assert_eq!(prepared["docs_failed"], 1);

    let document_rows = jsonl(root.join(".jikji/document_index.jsonl"));
    assert_eq!(document_rows[0]["path"], "timeout.rtf");
    assert_eq!(document_rows[0]["parse_status"], "failed");
    assert_eq!(document_rows[0]["parser"], "timeout");
}

fn assert_candidate_paths_exclude(candidates: &Value, suffix: &str) {
    assert!(
        candidates
            .as_array()
            .expect("candidates array")
            .iter()
            .all(|candidate| !candidate["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with(suffix)),
        "candidate paths should exclude {suffix}: {candidates}"
    );
}
