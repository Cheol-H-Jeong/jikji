use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::support::{json_cmd, root_arg, temp_root};

#[test]
fn prepare_agent_rule_flag_matches_python_contract() {
    let default_root = temp_root("task8-agent-rules-default");
    fs::write(default_root.join("note.txt"), "default rules").expect("write note");
    let default_arg = root_arg(&default_root);
    json_cmd(&["prepare", &default_arg, "--json"]);
    assert!(default_root.join("AGENTS.md").exists());

    let disabled_root = temp_root("task8-agent-rules-disabled");
    fs::write(disabled_root.join("note.txt"), "disabled rules").expect("write note");
    let disabled_arg = root_arg(&disabled_root);
    json_cmd(&["prepare", &disabled_arg, "--no-agent-rules", "--json"]);
    assert!(!disabled_root.join("AGENTS.md").exists());
    assert!(!disabled_root.join("CLAUDE.md").exists());
    assert!(!disabled_root.join(".cursorrules").exists());
}

#[test]
fn cli_help_exposes_required_parity_commands_and_prepare_flag() {
    let prepare = Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(["prepare", "--help"])
        .output()
        .expect("prepare help");
    assert!(prepare.status.success());
    let prepare_help = String::from_utf8_lossy(&prepare.stdout);
    assert!(prepare_help.contains("--no-agent-rules"));

    let help = Command::new(env!("CARGO_BIN_EXE_jikji"))
        .arg("--help")
        .output()
        .expect("top help");
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    for command in [
        "hippocamp-fetch",
        "hermes-bench",
        "hermes-compare",
        "benchmark-value-report",
    ] {
        assert!(text.contains(command), "missing {command}");
    }

    let hidden = Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(["post-install-prepare", "--help"])
        .output()
        .expect("hidden help");
    assert!(hidden.status.success());
}

#[test]
fn agent_skill_install_queues_default_common_and_document_roots() {
    let home = temp_root("task8-post-install-home");
    let documents = home.join("Documents");
    let client_docs = home.join("Projects/ClientDocs");
    fs::create_dir_all(&documents).expect("documents");
    fs::create_dir_all(&client_docs).expect("client docs");
    fs::write(documents.join("brief.pdf"), "common root").expect("brief");
    for name in ["a.pdf", "b.hwpx", "c.xlsx"] {
        fs::write(client_docs.join(name), "document heavy").expect("doc");
    }
    let dest = home.join("agent/SKILL.md");
    let output = Command::new(env!("CARGO_BIN_EXE_jikji"))
        .env("JIKJI_POST_INSTALL_HOME", &home)
        .args([
            "agent-skill-install",
            "--dest",
            dest.to_str().expect("dest"),
            "--json",
        ])
        .output()
        .expect("agent install");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(payload["post_install_prepare"]["mode"], "background");
    assert_eq!(payload["post_install_prepare"]["started"], true);
    assert!(payload["post_install_prepare"]["pid"].as_u64().is_some());
    assert!(
        Path::new(
            payload["post_install_prepare"]["log"]
                .as_str()
                .expect("background log")
        )
        .exists()
    );
    let roots = payload["post_install_prepare"]["roots"]
        .as_array()
        .expect("roots");
    assert!(queued_roots_contain_path(roots, &documents));
    assert!(queued_roots_contain_path(roots, &client_docs));
    assert_eq!(
        payload["post_install_prepare"]["selection"]["source"],
        "auto_common_and_document_roots"
    );
}

#[test]
fn post_install_prepare_foreground_applies_parse_timeout() {
    let root = temp_root("task8-post-install-parse-timeout");
    fs::write(root.join("timeout.rtf"), r"{\rtf1 timeout body}").expect("write rtf");
    let root_arg = root_arg(&root);

    let payload = json_cmd(&[
        "post-install-prepare",
        &root_arg,
        "--parse-timeout",
        "0",
        "--json",
    ]);

    assert_eq!(payload["mode"], "foreground");
    assert_eq!(payload["parse_timeout"], 0.0);
    let rows =
        std::fs::read_to_string(root.join(".jikji/document_index.jsonl")).expect("document index");
    assert!(rows.contains(r#""parser":"timeout""#), "{rows}");
}

fn queued_roots_contain_path(roots: &[Value], expected: &Path) -> bool {
    let expected = expected.to_string_lossy().replace('\\', "/");
    roots.iter().any(|item| {
        item.get("root")
            .and_then(Value::as_str)
            .is_some_and(|root| root.replace('\\', "/") == expected)
            && item.get("status").and_then(Value::as_str) == Some("queued")
    })
}
