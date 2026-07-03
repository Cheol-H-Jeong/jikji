from __future__ import annotations

import sys
from pathlib import Path

TOOLS_PARITY = Path(__file__).resolve().parents[2] / "tools" / "parity"
sys.path.insert(0, str(TOOLS_PARITY))

from run_rust_vs_python import (  # noqa: E402  # noqa: E402
    CommandPair,
    CommandRun,
    Json,
    _artifact_diff_summary,
    _command_failures,
)


def _artifact(path: str, digest: str = "same") -> dict[str, str | int]:
    return {"path": path, "sha256": digest, "bytes": 1}


def _command_run(name: str, stdout_json: dict[str, Json]) -> CommandRun:
    return CommandRun(
        name=name,
        args=(),
        exit_code=0,
        stdout="",
        stderr="",
        stdout_json=stdout_json,
        seconds=0.0,
        retry_proof="",
    )


def _write_required_artifacts(root: Path) -> None:
    jikji = root / ".jikji"
    (jikji / "wiki" / "sources").mkdir(parents=True)
    (jikji / "doc_text").mkdir()
    (jikji / "doc_meta").mkdir()
    (root / ".jikji_agent_map.md").write_text("# map\n", encoding="utf-8")
    for path in (
        "search_index.sqlite",
        "agent_map.md",
        "agent_routes.md",
        "agent_skill_context.md",
        "human_guide.md",
        "llm_wiki_schema.md",
        "wiki/index.md",
    ):
        (jikji / path).write_text("generated\n", encoding="utf-8")
    (jikji / "manifest.json").write_text(
        "{"
        '"schema_version":1,"generated_at":"x","root":".","files":1,"folders":0,'
        '"documents":0,"docs_parsed":0,"docs_reused":0,"docs_failed":0,'
        '"parse_errors":0,"deleted_since_last_index":0,"mode":"agent_search",'
        '"non_destructive":true,"cache_key_policy":"x","owned_paths":[],'
        '"retired_cleanup_paths":[],"parser_required_extensions":[],'
        '"native_text_extensions":[],"source_tree_signature":{'
        '"algorithm":"x","digest":"x","files":1,"folders":0,"total_size":1,"max_mtime_ns":1'
        "}}",
        encoding="utf-8",
    )
    (jikji / "corpus_profile.json").write_text("{}", encoding="utf-8")
    (jikji / "intent_taxonomy.json").write_text("{}", encoding="utf-8")
    (jikji / "autorag_manifest.json").write_text("{}", encoding="utf-8")
    (jikji / "knowledge_graph.json").write_text(
        '{"schema_version":1,"nodes":[],"edges":[]}', encoding="utf-8"
    )
    jsonl_rows = {
        "file_index.jsonl": '{"status":"present","path":"notes.txt","name":"notes.txt","ext":".txt","mime":"text/plain","size":1,"mtime":"x","mtime_ns":1,"created":"x","modified":"x","sha256":"x","parser_required":false,"parse_status":"native","text_cache_path":"","doc_meta_path":"","keywords":[],"summary":"x","indexed_at":"x"}\n',
        "folder_index.jsonl": '{"folder_id":"root","path":".","name":".","depth":0,"file_count_direct":1,"subfolder_count_direct":0,"total_size_direct":1,"top_extensions_direct":[],"child_folders":[],"keywords":[],"summary":"x"}\n',
        "document_index.jsonl": "",
        "file_cards.jsonl": "{}\n",
        "chunk_map.jsonl": "{}\n",
        "duplicate_map.jsonl": "",
        "folder_profile.jsonl": "{}\n",
        "graph_routes.jsonl": '{"path":"notes.txt","source_id":"source:1","wiki_path":".jikji/wiki/sources/notes-aaaaaaaaaaaa.md","folder":".","terms":[],"intents":[],"ext":".txt","parse_status":"native","text_cache_path":"","preview":"x"}\n',
        "parse_errors.jsonl": "",
    }
    for name, text in jsonl_rows.items():
        (jikji / name).write_text(text, encoding="utf-8")


def test_source_wiki_hash_suffix_mismatch_is_intentional_when_stems_match(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)

    summary = _artifact_diff_summary(
        (
            _artifact(".jikji/manifest.json"),
            _artifact(".jikji/wiki/sources/notes-aaaaaaaaaaaa.md"),
        ),
        (
            _artifact(".jikji/manifest.json"),
            _artifact(".jikji/wiki/sources/notes-bbbbbbbbbbbb.md"),
        ),
        python_root,
        rust_root,
    )

    assert summary["contract_failures"] == []
    assert summary["intentional_non_parity"] == [
        "wiki source slug hash differs for stems ['notes']"
    ]


def test_missing_required_artifact_class_still_fails(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)
    (rust_root / ".jikji" / "manifest.json").unlink()

    summary = _artifact_diff_summary(
        (_artifact(".jikji/manifest.json"),),
        (),
        python_root,
        rust_root,
    )

    assert ".jikji/manifest.json" in summary["contract_failures"]


def test_cleaned_reference_without_generated_artifacts_skips_prepare_schema_checks(
    tmp_path: Path,
) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    for root in (python_root, rust_root):
        (root / ".jikji" / "wiki").mkdir(parents=True)
        (root / ".jikji" / "doc_text").mkdir()
        (root / ".jikji" / "doc_meta").mkdir()
    python_file = python_root / ".jikji" / "user-created-note.txt"
    rust_file = rust_root / ".jikji" / "user-created-note.txt"
    python_file.write_text("user", encoding="utf-8")
    rust_file.write_text("user", encoding="utf-8")

    summary = _artifact_diff_summary(
        (_artifact(".jikji/user-created-note.txt"),),
        (_artifact(".jikji/user-created-note.txt"),),
        python_root,
        rust_root,
    )

    assert summary["contract_failures"] == []


def test_doc_text_digest_mismatch_is_intentional_when_cache_exists(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)

    summary = _artifact_diff_summary(
        (_artifact(".jikji/doc_text/sha256_abc.txt", "python"),),
        (_artifact(".jikji/doc_text/sha256_abc.txt", "rust"),),
        python_root,
        rust_root,
    )

    assert ".jikji/doc_text/sha256_abc.txt" not in summary["contract_failures"]
    assert ".jikji/doc_text/sha256_abc.txt" in summary["intentional_non_parity"]


def test_missing_doc_text_cache_still_fails(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)

    summary = _artifact_diff_summary(
        (_artifact(".jikji/doc_text/sha256_abc.txt", "python"),),
        (),
        python_root,
        rust_root,
    )

    assert ".jikji/doc_text/sha256_abc.txt" in summary["contract_failures"]


def test_empty_doc_text_cache_still_fails_when_python_cache_has_content(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)

    summary = _artifact_diff_summary(
        (_artifact(".jikji/doc_text/sha256_abc.txt", "python"),),
        ({"path": ".jikji/doc_text/sha256_abc.txt", "sha256": "rust", "bytes": 0},),
        python_root,
        rust_root,
    )

    assert ".jikji/doc_text/sha256_abc.txt: doc_text cache is empty" in summary[
        "contract_failures"
    ]


def test_parse_valid_generated_json_digest_mismatch_is_intentional(tmp_path: Path) -> None:
    python_root = tmp_path / "python"
    rust_root = tmp_path / "rust"
    _write_required_artifacts(python_root)
    _write_required_artifacts(rust_root)

    summary = _artifact_diff_summary(
        (_artifact(".jikji/corpus_profile.json", "python"),),
        (_artifact(".jikji/corpus_profile.json", "rust"),),
        python_root,
        rust_root,
    )

    assert ".jikji/corpus_profile.json" not in summary["contract_failures"]
    assert ".jikji/corpus_profile.json" in summary["intentional_non_parity"]


def test_json_key_and_candidate_order_failures_remain_contract_failures() -> None:
    python = _command_run(
        "find_example",
        {
            "answer_paths": [],
            "candidates": [{"path": "a.txt"}, {"path": "b.txt"}],
            "handoff_action": "open",
            "paths": ["a.txt", "b.txt"],
            "query_variants": [],
            "raw_fallback_allowed": False,
            "retry_proof": "",
            "tool_call_policy": {},
        },
    )
    rust = _command_run("find_example", {"candidates": [{"path": "b.txt"}, {"path": "a.txt"}]})

    failures = _command_failures(CommandPair("scenario", python, rust))

    assert any("missing JSON keys" in failure for failure in failures)
    assert any("ranking mismatch" in failure for failure in failures)


def test_empty_rust_candidates_fail_when_python_candidates_are_nonempty() -> None:
    python = _command_run(
        "find_example",
        {
            "answer_paths": [],
            "candidates": [{"path": "a.txt"}],
            "handoff_action": "open",
            "paths": ["a.txt"],
            "query_variants": [],
            "raw_fallback_allowed": False,
            "retry_proof": "",
            "tool_call_policy": {},
        },
    )
    rust = _command_run(
        "find_example",
        {
            "answer_paths": [],
            "candidates": [],
            "handoff_action": "open",
            "paths": [],
            "query_variants": [],
            "raw_fallback_allowed": False,
            "retry_proof": "",
            "tool_call_policy": {},
        },
    )

    failures = _command_failures(CommandPair("scenario", python, rust))

    assert any("ranking mismatch" in failure for failure in failures)


def test_extra_rust_command_json_key_remains_contract_failure() -> None:
    python = _command_run("doctor", {"ok": True})
    rust = _command_run("doctor", {"ok": True, "rust_only": True})

    failures = _command_failures(CommandPair("scenario", python, rust))

    assert "scenario/doctor: Rust extra JSON keys ['rust_only']" in failures
