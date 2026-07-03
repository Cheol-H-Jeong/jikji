from __future__ import annotations

import json

from jikji.agent_index import build_agent_index
from jikji.config import Config


def test_hermes_answer_pack_mode_reports_jikji_find_without_chat(tmp_path):
    from jikji.hermes_bench import run_hermes_benchmark

    root = tmp_path / "root"
    root.mkdir()
    (root / "contracts").mkdir()
    (root / "contracts" / "acme-renewal.txt").write_text(
        "unique renewal indemnity clause", encoding="utf-8"
    )
    build_agent_index(root, Config())
    eval_set = tmp_path / "cases.jsonl"
    eval_set.write_text(
        json.dumps(
            {
                "id": "c001",
                "scenario": "answer_pack",
                "query": "unique renewal indemnity clause",
                "expected_paths": ["contracts/acme-renewal.txt"],
            }
        )
        + "\n",
        encoding="utf-8",
    )
    report_path = tmp_path / "report.json"

    result = run_hermes_benchmark(
        root,
        eval_set=eval_set,
        out=report_path,
        modes=("jikji-answer-pack",),
        hermes_bin="/missing/hermes",
        candidate_top_k=5,
        retries=0,
    )

    report = json.loads(result.report_path.read_text(encoding="utf-8"))
    metrics = report["modes"]["jikji-answer-pack"]["metrics"]
    detail = report["modes"]["jikji-answer-pack"]["details"][0]
    assert metrics["usage_status"] == "not_applicable_zero_chat"
    assert detail["hit"] is True
    assert detail["predicted_paths"] == ["contracts/acme-renewal.txt"]
    assert detail["agent_chat_turns"] == 0
    assert detail["llm_calls"] == 0
    assert detail["attempts"][0]["tool"] == "jikji find"
    assert detail["handoff_action"] == "direct_use"
    assert detail["raw_fallback_allowed"] is False


def test_hermes_answer_pack_mode_aliases():
    from jikji.hermes_bench import _mode_family

    assert _mode_family("jikji-answer-pack") == "jikji-answer-pack"
    assert _mode_family("answer-pack") == "jikji-answer-pack"
    assert _mode_family("discover-direct") == "jikji-answer-pack"


def test_answer_pack_attempt_preserves_supporting_paths_for_hitk(monkeypatch, tmp_path):
    from jikji import hermes_answer_pack

    def discover_with_supporting_paths(*args, **kwargs):
        return {
            "handoff_action": "direct_use",
            "answer_paths": ["first.txt"],
            "supporting_paths": ["second.txt", "third.txt"],
            "candidates": [{"p": "first.txt"}, {"p": "second.txt"}, {"p": "third.txt"}],
        }

    monkeypatch.setattr(hermes_answer_pack, "discover", discover_with_supporting_paths)

    attempt = hermes_answer_pack.run_answer_pack_attempt(
        tmp_path,
        "find the supporting target",
        top_k=10,
    )

    assert attempt.predicted == ["first.txt", "second.txt", "third.txt"]
    assert attempt.candidates == [
        {"path": "first.txt"},
        {"path": "second.txt"},
        {"path": "third.txt"},
    ]


def test_hermes_answer_pack_failure_is_not_clean_zero_chat(tmp_path, monkeypatch):
    from jikji import hermes_answer_pack
    from jikji.hermes_bench import run_hermes_benchmark

    def fail_discover(*args, **kwargs):
        raise RuntimeError("simulated discover failure")

    root = tmp_path / "root"
    root.mkdir()
    (root / "contracts").mkdir()
    (root / "contracts" / "acme-renewal.txt").write_text(
        "unique renewal indemnity clause", encoding="utf-8"
    )
    build_agent_index(root, Config())
    eval_set = tmp_path / "cases.jsonl"
    eval_set.write_text(
        json.dumps(
            {
                "id": "c001",
                "scenario": "answer_pack",
                "query": "unique renewal indemnity clause",
                "expected_paths": ["contracts/acme-renewal.txt"],
            }
        )
        + "\n",
        encoding="utf-8",
    )
    report_path = tmp_path / "report.json"
    monkeypatch.setattr(hermes_answer_pack, "discover", fail_discover)

    result = run_hermes_benchmark(
        root,
        eval_set=eval_set,
        out=report_path,
        modes=("jikji-answer-pack",),
        hermes_bin="/missing/hermes",
        candidate_top_k=5,
        retries=0,
    )

    report = json.loads(result.report_path.read_text(encoding="utf-8"))
    metrics = report["modes"]["jikji-answer-pack"]["metrics"]
    detail = report["modes"]["jikji-answer-pack"]["details"][0]
    assert metrics["usage_status"] == "answer_pack_failed"
    assert detail["usage_status"] == "answer_pack_failed"
    assert detail["attempts"][0]["returncode"] == -1
    assert detail["llm_calls"] == 0


def test_hermes_usage_status_requires_token_accounting(tmp_path):
    from jikji.hermes_bench import _metrics

    metrics = _metrics(
        [
            {
                "hit": True,
                "rank": 1,
                "duplicate_rank": 1,
                "scenario": "usage",
                "llm_calls": 1,
                "usage_status": "missing_usage",
                "usage": {
                    "llm_calls": 1,
                    "prompt_tokens": 0,
                    "completion_tokens": 0,
                    "reasoning_tokens": 0,
                    "total_tokens": 0,
                },
            }
        ],
        0.1,
    )

    assert metrics["usage_status"] == "missing_usage"
