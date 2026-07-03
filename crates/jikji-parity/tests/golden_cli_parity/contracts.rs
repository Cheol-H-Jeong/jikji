use std::fs;

use super::fixture::{
    assert_golden_manifest, golden_command, root_arg, run, run_json, temp_root,
    write_answer_pack_fixture, write_ascii_cjk_fixture,
};

#[test]
fn rust_search_and_find_match_python_golden_contract_fields() {
    assert_golden_manifest();
    let root = temp_root("ascii-cjk");
    write_ascii_cjk_fixture(&root);
    let root_arg = root_arg(&root);
    run_json(&["prepare", &root_arg, "--json"]);

    let expected_ascii = golden_command("ascii_cjk_paths", "search_ascii");
    let search_ascii = run_json(&["search", &root_arg, "direct-answer-771", "--json"]);
    assert_eq!(
        search_ascii["candidates"][0]["path"],
        expected_ascii["candidates"][0]["path"]
    );
    assert_eq!(search_ascii["index_status"], expected_ascii["index_status"]);
    assert_eq!(
        search_ascii["foreground_prepared"],
        expected_ascii["foreground_prepared"]
    );
    assert_eq!(
        search_ascii["background_refresh_started"],
        expected_ascii["background_refresh_started"]
    );

    let expected_cjk = golden_command("ascii_cjk_paths", "search_cjk");
    let search_cjk = run_json(&["search", &root_arg, "서울 연구소", "--json"]);
    assert_eq!(
        search_cjk["candidates"][0]["path"],
        expected_cjk["candidates"][0]["path"]
    );

    let expected_find = golden_command("ascii_cjk_paths", "find_direct");
    let found = run_json(&["find", &root_arg, "ACME indemnity marker", "--json"]);
    assert_eq!(found["mode"], expected_find["mode"]);
    assert_eq!(
        found["answer_pack_version"],
        expected_find["answer_pack_version"]
    );
    assert_eq!(found["answer_paths"][0], expected_find["answer_paths"][0]);
    assert_eq!(found["handoff_action"], expected_find["handoff_action"]);
    assert_eq!(
        found["tool_call_policy"]["stop_after_find"],
        expected_find["tool_call_policy"]["stop_after_find"]
    );
}

#[test]
fn rust_answer_pack_shell_and_stale_contracts_match_python_golden() {
    assert_golden_manifest();
    let shell_root = temp_root("answer-pack");
    write_answer_pack_fixture(&shell_root);
    let shell_arg = root_arg(&shell_root);
    run_json(&["prepare", &shell_arg, "--json"]);

    let expected_shell = golden_command("answer_pack_shell", "find_shell_noise");
    let shell = run_json(&[
        "find",
        &shell_arg,
        "zzzznohit \"semi; rm -rf /\" $(echo nope)",
        "--json",
    ]);
    assert_eq!(shell["confidence"], expected_shell["confidence"]);
    assert_eq!(shell["handoff_action"], expected_shell["handoff_action"]);
    assert_eq!(shell["answerability"], expected_shell["answerability"]);
    assert_eq!(
        shell["raw_fallback_allowed"],
        expected_shell["raw_fallback_allowed"]
    );
    assert_eq!(
        shell["max_raw_fallback_commands"],
        expected_shell["max_raw_fallback_commands"]
    );
    assert_eq!(shell["paths"].as_array().expect("paths").len(), 0);
    assert!(!shell["query_variants"]
        .to_string()
        .to_ascii_lowercase()
        .contains("rm"));

    let expected_forged = golden_command("answer_pack_shell", "find_shell_retry_forged");
    let forged = run_json(&[
        "find",
        &shell_arg,
        "zzzznohit \"semi; rm -rf /\" $(echo nope)",
        "--after-jikji-retry",
        "--retry-proof",
        "forged",
        "--json",
    ]);
    assert_eq!(forged["handoff_action"], expected_forged["handoff_action"]);
    assert_eq!(
        forged["raw_fallback_allowed"],
        expected_forged["raw_fallback_allowed"]
    );

    assert_stale_find_matches_python_golden();
}

#[test]
fn rust_brief_and_graph_contracts_are_available_on_golden_fixture() {
    assert_golden_manifest();
    let root = temp_root("brief-graph");
    write_ascii_cjk_fixture(&root);
    let root_arg = root_arg(&root);
    run_json(&["prepare", &root_arg, "--json"]);

    let brief = run_json(&["brief", &root_arg, "direct-answer-771", "--json"]);
    assert_eq!(brief["schema_version"], 1);
    assert_eq!(brief["index_status"], "ready");
    assert_eq!(brief["candidates"][0]["path"], "docs/acme-contract.txt");
    assert!(brief["artifacts"]["search_index"]
        .as_str()
        .expect("search artifact")
        .ends_with(".jikji/search_index.sqlite"));

    let graph_status = run_json(&["graph", &root_arg, "status", "--json"]);
    assert_eq!(graph_status["prepared"], true);
    assert!(graph_status["stats"]["nodes"].as_u64().expect("nodes") > 0);
    let graph_query = run_json(&["graph", &root_arg, "query", "direct-answer-771", "--json"]);
    assert_eq!(
        graph_query["candidates"][0]["path"],
        "docs/acme-contract.txt"
    );
}

fn assert_stale_find_matches_python_golden() {
    let stale_root = temp_root("stale");
    fs::write(
        stale_root.join("notes.txt"),
        "stable stale-index target token stale-old-101",
    )
    .expect("write stale");
    let stale_arg = root_arg(&stale_root);
    run_json(&["prepare", &stale_arg, "--json"]);
    fs::write(stale_root.join("new-file.txt"), "new stale content").expect("mutate stale");

    let expected = golden_command("stale_index_find", "find_stale_previous");
    let stale = run_json(&[
        "find",
        &stale_arg,
        "stale-old-101",
        "--stale-after-seconds",
        "-1",
        "--json",
    ]);
    assert_eq!(stale["index_status"], expected["index_status"]);
    assert_eq!(stale["answer_paths"][0], expected["answer_paths"][0]);
    assert!(!stale["paths"].to_string().contains("new-file.txt"));
}

#[test]
fn corrupted_sqlite_search_index_is_a_controlled_malformed_input_failure() {
    assert_golden_manifest();
    let root = temp_root("corrupt");
    fs::write(root.join("note.txt"), "ACME agreement").expect("write note");
    let root_arg = root_arg(&root);
    run_json(&["prepare", &root_arg, "--json"]);
    fs::write(
        root.join(".jikji/search_index.sqlite"),
        b"not a sqlite database",
    )
    .expect("corrupt sqlite");

    let output = run(&["find", &root_arg, "ACME", "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("search_index.sqlite"));
    assert!(!stderr.contains("rm -rf"));
}
