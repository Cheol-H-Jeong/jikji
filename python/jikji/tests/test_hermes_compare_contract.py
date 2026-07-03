from __future__ import annotations

import json


def _write_report(path, mode: str, metrics: dict) -> None:
    path.write_text(json.dumps({"modes": {mode: {"metrics": metrics}}}), encoding="utf-8")


def test_hermes_compare_supports_inference_budget_gates(tmp_path):
    from jikji.hermes_compare import compare_benchmark_reports

    raw_report = tmp_path / "raw.json"
    jikji_report = tmp_path / "jikji.json"
    _write_report(
        raw_report,
        "raw",
        {
            "hit_at_1": 0.5,
            "hit_at_10": 0.7,
            "total_tokens": 1000,
            "llm_calls": 100,
            "seconds": 10,
            "usage_status": "ok",
        },
    )
    _write_report(
        jikji_report,
        "jikji-one-shot",
        {
            "hit_at_1": 0.6,
            "hit_at_10": 0.8,
            "total_tokens": 200,
            "llm_calls": 10,
            "seconds": 5,
            "usage_status": "ok",
            "avg_llm_calls": 1.2,
            "p95_llm_calls": 3,
        },
    )

    payload = compare_benchmark_reports(
        raw_report,
        jikji_report,
        jikji_mode="jikji-one-shot",
        max_avg_llm_calls=1.5,
        max_p95_llm_calls=3,
    )

    assert payload["ok"] is True
    assert payload["checks"]["avg_llm_calls_below_budget"] is True
    assert payload["checks"]["p95_llm_calls_below_budget"] is True


def test_hermes_compare_fails_missing_usage_accounting(tmp_path):
    from jikji.hermes_compare import compare_benchmark_reports

    raw_report = tmp_path / "raw.json"
    jikji_report = tmp_path / "jikji.json"
    _write_report(
        raw_report,
        "raw",
        {
            "hit_at_1": 1.0,
            "hit_at_10": 1.0,
            "total_tokens": 100,
            "llm_calls": 10,
            "seconds": 10,
            "usage_status": "ok",
        },
    )
    _write_report(
        jikji_report,
        "jikji-discover",
        {
            "hit_at_1": 1.0,
            "hit_at_10": 1.0,
            "total_tokens": 0,
            "llm_calls": 0,
            "seconds": 1,
            "usage_status": "missing_usage",
        },
    )

    payload = compare_benchmark_reports(raw_report, jikji_report)

    assert payload["ok"] is False
    assert payload["checks"]["usage_accounting_ok"] is False


def test_hermes_compare_restricts_zero_chat_usage_status_to_answer_pack(tmp_path):
    from jikji.hermes_compare import compare_benchmark_reports

    raw_report = tmp_path / "raw.json"
    jikji_report = tmp_path / "jikji.json"
    base_metrics = {
        "hit_at_1": 1.0,
        "hit_at_10": 1.0,
        "total_tokens": 0,
        "llm_calls": 0,
        "seconds": 1,
        "usage_status": "not_applicable_zero_chat",
    }
    _write_report(raw_report, "raw", base_metrics)
    _write_report(jikji_report, "jikji-discover", base_metrics)

    payload = compare_benchmark_reports(raw_report, jikji_report)

    assert payload["ok"] is False
    assert payload["checks"]["usage_accounting_ok"] is False


def test_hermes_compare_accepts_zero_chat_for_answer_pack_only_when_raw_has_usage(tmp_path):
    from jikji.hermes_compare import compare_benchmark_reports

    raw_report = tmp_path / "raw.json"
    jikji_report = tmp_path / "jikji.json"
    _write_report(
        raw_report,
        "raw",
        {
            "hit_at_1": 1.0,
            "hit_at_10": 1.0,
            "total_tokens": 100,
            "llm_calls": 10,
            "seconds": 10,
            "usage_status": "ok",
        },
    )
    _write_report(
        jikji_report,
        "jikji-answer-pack",
        {
            "hit_at_1": 1.0,
            "hit_at_10": 1.0,
            "total_tokens": 0,
            "llm_calls": 0,
            "seconds": 1,
            "usage_status": "not_applicable_zero_chat",
            "avg_llm_calls": 0,
            "p95_llm_calls": 0,
        },
    )

    payload = compare_benchmark_reports(
        raw_report,
        jikji_report,
        jikji_mode="jikji-answer-pack",
        max_avg_llm_calls=0,
        max_p95_llm_calls=0,
    )

    assert payload["ok"] is True
    assert payload["checks"]["usage_accounting_ok"] is True
