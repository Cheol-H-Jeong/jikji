use std::fs;

use serde_json::Value;

use super::fixture::{database_path, json_cmd, root_arg, run, run_ok, temp_root};

#[test]
fn search_brief_graph_and_find_return_python_contract_fields() {
    let root = temp_root("task5-contract");
    fs::create_dir(root.join("contracts")).expect("create contracts");
    fs::write(
        root.join("contracts/ACME_master_services_agreement.txt"),
        "ACME master services agreement renewal indemnity terms direct-answer.",
    )
    .expect("write acme");
    fs::write(
        root.join("notes.txt"),
        "ordinary meeting notes unrelated to contracts",
    )
    .expect("write notes");
    let root_arg = root_arg(&root);

    let prepared = json_cmd(&["prepare", &root_arg, "--json"]);
    assert_eq!(prepared["files"], 2);
    assert!(
        database_path(&root)
            .metadata()
            .expect("central sqlite")
            .len()
            > 0
    );
    assert!(!root.join(".jikji").exists());

    assert_search_contract(&root_arg);
    assert_brief_contract(&root_arg);
    assert_graph_contract(&root_arg);
    assert_find_contract(&root_arg);
}

#[test]
fn missing_index_requires_exactly_one_jikji_retry_before_raw_fallback() {
    let root = temp_root("missing-index-recovery-contract");
    let root_arg = root_arg(&root);
    let first = run(&["find", &root_arg, "lost contract", "--json"]);
    assert_eq!(first.status.code(), Some(1));
    let first_payload: Value = serde_json::from_slice(&first.stdout).expect("first recovery json");
    assert_eq!(first_payload["index_status"], "missing");
    assert_eq!(first_payload["handoff_action"], "jikji_retry");
    assert_eq!(first_payload["max_jikji_retries"], 1);
    assert_eq!(first_payload["raw_fallback_allowed"], false);
    assert_eq!(first_payload["max_raw_fallback_commands"], 0);
    let proof = first_payload["retry_proof"].as_str().expect("retry proof");

    let second = run(&[
        "find",
        &root_arg,
        "lost contract",
        "--after-jikji-retry",
        "--retry-proof",
        proof,
        "--json",
    ]);
    assert_eq!(second.status.code(), Some(1));
    let second_payload: Value = serde_json::from_slice(&second.stdout).expect("fallback json");
    assert_eq!(second_payload["handoff_action"], "raw_fallback_after_retry");
    assert_eq!(second_payload["max_jikji_retries"], 0);
    assert_eq!(second_payload["raw_fallback_allowed"], true);
    assert_eq!(second_payload["max_raw_fallback_commands"], 2);
}

#[test]
fn search_ignores_english_stopwords_as_filename_anchors() {
    let root = temp_root("english-stopword-anchor");
    fs::write(
        root.join("Meta_Q2_2025_Earnings_Call.mp3"),
        "earnings conference call transcript with platform revenue",
    )
    .expect("write meta");
    fs::write(
        root.join("weekly_health_records.txt"),
        "health week records running sleep dental appointment recovery",
    )
    .expect("write health");
    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    let search = json_cmd(&[
        "search",
        &root_arg,
        "What do I usually do in a week for my health records?",
        "--top-k",
        "3",
        "--json",
    ]);

    assert_eq!(search["candidates"][0]["path"], "weekly_health_records.txt");
}

fn assert_search_contract(root_arg: &str) {
    let search = json_cmd(&[
        "search",
        root_arg,
        "ACME agreement",
        "--top-k",
        "3",
        "--json",
    ]);
    assert_eq!(search["index_status"], "ready");
    assert_eq!(search["foreground_prepared"], false);
    assert_eq!(search["background_refresh_started"], false);
    assert_eq!(
        search["candidates"][0]["path"],
        "contracts/ACME_master_services_agreement.txt"
    );
    assert!(search["candidates"][0]["score"].as_f64().expect("score") > 0.0);
    assert!(
        search["candidates"][0]["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason == "fielded-bm25" || reason == "filename-anchor")
    );
}

fn assert_brief_contract(root_arg: &str) {
    let brief = json_cmd(&[
        "brief",
        root_arg,
        "ACME agreement",
        "--top-k",
        "3",
        "--json",
    ]);
    assert_eq!(brief["schema_version"], 1);
    assert_eq!(brief["index_status"], "ready");
    assert_eq!(
        brief["candidates"][0]["path"],
        "contracts/ACME_master_services_agreement.txt"
    );
    assert_eq!(brief["artifacts"]["storage"], "central_sqlite");
    assert!(brief["artifacts"]["database"].as_str().is_some());

    let compact = run_ok(&[
        "brief",
        root_arg,
        "ACME agreement",
        "--top-k",
        "3",
        "--compact",
        "--json",
    ]);
    let compact_text = String::from_utf8(compact.stdout).expect("compact utf8");
    assert!(!compact_text.contains(": "));
    let compact_json: Value = serde_json::from_str(&compact_text).expect("compact json");
    assert_eq!(compact_json["mode"], "compact_graph_brief");
    assert_eq!(
        compact_json["candidates"][0]["p"],
        "contracts/ACME_master_services_agreement.txt"
    );
}

fn assert_graph_contract(root_arg: &str) {
    let graph_status = json_cmd(&["graph", root_arg, "status", "--json"]);
    assert_eq!(graph_status["available"], true);
    assert!(graph_status["nodes"].as_u64().expect("nodes") > 0);

    let graph_query = json_cmd(&[
        "graph",
        root_arg,
        "query",
        "ACME agreement",
        "--top-k",
        "3",
        "--json",
    ]);
    assert_eq!(
        graph_query["candidates"][0]["path"],
        "contracts/ACME_master_services_agreement.txt"
    );

    let graph_explain = json_cmd(&[
        "graph",
        root_arg,
        "explain",
        "contracts/ACME_master_services_agreement.txt",
        "--json",
    ]);
    assert_eq!(graph_explain["found"], true);
    assert_eq!(
        graph_explain["route"]["path"],
        "contracts/ACME_master_services_agreement.txt"
    );
}

fn assert_find_contract(root_arg: &str) {
    let found = json_cmd(&[
        "find",
        root_arg,
        "Find the ACME master services agreement",
        "--json",
    ]);
    assert_eq!(found["mode"], "find");
    assert_eq!(found["command"], "jikji find");
    assert_eq!(found["answer_pack_version"], 1);
    assert_eq!(found["index_status"], "ready");
    assert_eq!(
        found["answer_paths"][0],
        "contracts/ACME_master_services_agreement.txt"
    );
    assert_eq!(
        found["paths"][0],
        "contracts/ACME_master_services_agreement.txt"
    );
    assert_eq!(
        found["llm_search_plan"]["mode"],
        "one_call_multi_search_judge"
    );
    assert_eq!(found["tool_call_policy"]["stop_after_find"], true);

    let first = json_cmd(&[
        "find",
        root_arg,
        "Find the ACME master services agreement",
        "--first",
        "--json",
    ]);
    assert_eq!(
        first["answer_paths"]
            .as_array()
            .expect("answer paths")
            .len(),
        1
    );
    assert_eq!(first["candidates"].as_array().expect("candidates").len(), 1);
}
