use std::fs;

use serde_json::Value;

use super::fixture::{json_cmd, root_arg, run_ok, temp_root};

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
        root.join(".jikji/search_index.sqlite")
            .metadata()
            .expect("search sqlite")
            .len()
            > 0
    );
    assert!(
        root.join(".jikji/graph_routes.jsonl")
            .metadata()
            .expect("routes")
            .len()
            > 0
    );

    assert_search_contract(&root_arg);
    assert_brief_contract(&root_arg);
    assert_graph_contract(&root_arg);
    assert_find_contract(&root_arg);
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
    assert!(
        brief["artifacts"]["search_index"]
            .as_str()
            .expect("search artifact")
            .ends_with(".jikji/search_index.sqlite")
    );

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
    assert_eq!(graph_status["prepared"], true);
    assert!(graph_status["stats"]["nodes"].as_u64().expect("nodes") > 0);

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
