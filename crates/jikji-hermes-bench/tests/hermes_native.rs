#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use jikji_hermes_bench::{
        HermesBenchOptions, read_session_usage, recover_json_object, run_hermes_benchmark,
    };
    use rusqlite::Connection;
    use serde_json::Value;
    use tempfile::TempDir;

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn fixture(script: &str, timeout: Duration) -> (TempDir, HermesBenchOptions) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("corpus");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("answer.txt"), "answer").unwrap();
        let eval = temp.path().join("eval.jsonl");
        fs::write(&eval, "{\"id\":\"case-1\",\"scenario\":\"fixture\",\"query\":\"find answer\",\"expected_paths\":[\"answer.txt\"]}\n").unwrap();
        let hermes = temp.path().join("fake-hermes");
        write_executable(&hermes, script);
        let out = temp.path().join("report.json");
        let home = temp.path().join("hermes-home");
        fs::create_dir(&home).unwrap();
        let options = HermesBenchOptions {
            root,
            eval_set: eval,
            out,
            modes: vec!["raw".to_owned()],
            cases_limit: None,
            hermes_bin: hermes,
            model: String::new(),
            provider: String::new(),
            timeout,
            max_turns: 2,
            fast_max_turns: 1,
            skills: String::new(),
            candidate_top_k: 20,
            retries: 0,
            allow_leak: false,
            yolo: false,
            hermes_home: Some(home),
        };
        (temp, options)
    }

    fn report(options: &HermesBenchOptions) -> Value {
        serde_json::from_slice(&fs::read(&options.out).unwrap()).unwrap()
    }

    #[test]
    fn fake_hermes_success_recovers_json_and_usage() {
        let (_temp, options) = fixture(
            "printf '%s\\n' 'noise before {\"paths\":[\"answer.txt\"]} noise after'\nprintf '%s\\n' 'session_id: sess-1' >&2",
            Duration::from_secs(2),
        );
        let home = options.hermes_home.as_ref().unwrap();
        let connection = Connection::open(home.join("state.db")).unwrap();
        connection.execute_batch("CREATE TABLE sessions (id TEXT PRIMARY KEY, input_tokens INTEGER, output_tokens INTEGER, reasoning_tokens INTEGER, message_count INTEGER, tool_call_count INTEGER); INSERT INTO sessions VALUES ('sess-1', 10, 4, 2, 5, 1);").unwrap();
        fs::create_dir(home.join("sessions")).unwrap();
        fs::write(
            home.join("sessions/session_sess-1.json"),
            r#"{"messages":[{"role":"user"},{"role":"assistant"},{"role":"assistant"}]}"#,
        )
        .unwrap();

        let result = run_hermes_benchmark(&options).unwrap();
        let data = report(&options);
        assert_eq!(result.metrics["raw"]["accuracy"], 1.0);
        assert_eq!(
            data["modes"]["raw"]["details"][0]["predicted_paths"][0],
            "answer.txt"
        );
        assert_eq!(
            data["modes"]["raw"]["details"][0]["usage"]["total_tokens"],
            16
        );
        assert_eq!(data["modes"]["raw"]["details"][0]["llm_calls"], 2);
        assert_eq!(read_session_usage(home, "sess-1").prompt_tokens, 10);
        assert_eq!(
            recover_json_object("prefix {\"paths\":[\"a\"]} suffix").unwrap()["paths"][0],
            "a"
        );
    }

    #[test]
    fn timeout_kills_fake_hermes_and_reports_failure() {
        let (_temp, options) = fixture(
            "sleep 2\nprintf '%s\\n' '{\"paths\":[\"answer.txt\"]}'",
            Duration::from_millis(50),
        );
        run_hermes_benchmark(&options).unwrap();
        let data = report(&options);
        let detail = &data["modes"]["raw"]["details"][0];
        assert_eq!(detail["timeout"], true);
        assert_eq!(detail["returncode"], -1);
        assert_eq!(detail["hit"], false);
    }

    #[test]
    fn corpus_mutation_invalidates_an_otherwise_correct_hit() {
        let (_temp, options) = fixture(
            "printf 'changed' >> answer.txt\nprintf '%s\\n' '{\"paths\":[\"answer.txt\"]}'",
            Duration::from_secs(2),
        );
        run_hermes_benchmark(&options).unwrap();
        let data = report(&options);
        let detail = &data["modes"]["raw"]["details"][0];
        assert_eq!(detail["mutated_paths"][0], "answer.txt");
        assert_eq!(detail["rank"], Value::Null);
        assert_eq!(detail["hit"], false);
    }

    #[test]
    fn generated_jikji_artifacts_are_not_corpus_mutations() {
        let (_temp, options) = fixture(
            "mkdir -p .jikji\nprintf artifact > .jikji/manifest.json\nprintf map > .jikji_agent_map.md\nprintf '%s\\n' '{\"paths\":[\"answer.txt\"]}'",
            Duration::from_secs(2),
        );
        run_hermes_benchmark(&options).unwrap();
        let data = report(&options);
        assert_eq!(
            data["modes"]["raw"]["details"][0]["mutated_paths"],
            Value::Array(vec![])
        );
        assert_eq!(data["modes"]["raw"]["details"][0]["hit"], true);
    }

    #[test]
    fn generated_jikji_eval_and_report_paths_are_allowed() {
        let (_temp, mut options) = fixture(
            "printf '%s\\n' '{\"paths\":[\"answer.txt\"]}'",
            Duration::from_secs(2),
        );
        let root = options.root.clone();
        let eval_dir = root.join(".jikji/eval");
        fs::create_dir_all(&eval_dir).unwrap();
        options.eval_set = eval_dir.join("cases.jsonl");
        fs::write(
            &options.eval_set,
            "{\"id\":\"case-1\",\"query\":\"answer\",\"expected_paths\":[\"answer.txt\"]}\n",
        )
        .unwrap();
        options.out = eval_dir.join("report.json");

        run_hermes_benchmark(&options).unwrap();
        assert!(options.out.is_file());
    }

    #[allow(dead_code)]
    fn _path(_: PathBuf) {}
}
