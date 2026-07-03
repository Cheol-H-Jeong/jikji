from __future__ import annotations

import json
import shutil
from pathlib import Path


def test_accuracy_first_profile_floor_uses_raw_only_when_jikji_loses(tmp_path):
    from jikji.benchmark_value import Pricing, build_accuracy_first_value_report

    raw_discover_dir = tmp_path / "chunks"
    raw_discover_dir.mkdir()
    chunk = {
        "modes": {
            "raw": {
                "metrics": {
                    "cases": 4,
                    "hit_at_1": 0.5,
                    "hit_at_10": 0.75,
                    "llm_calls": 40,
                    "prompt_tokens": 1000,
                    "completion_tokens": 200,
                    "total_tokens": 1200,
                    "seconds": 100.0,
                },
                "details": [{"llm_calls": 45}, {"llm_calls": 10}, {"llm_calls": 3}, {"llm_calls": 2}],
            },
            "jikji-discover": {
                "metrics": {
                    "cases": 4,
                    "hit_at_1": 0.25,
                    "hit_at_10": 1.0,
                    "llm_calls": 12,
                    "prompt_tokens": 300,
                    "completion_tokens": 50,
                    "total_tokens": 350,
                    "seconds": 40.0,
                },
                "details": [{"llm_calls": 4}, {"llm_calls": 3}, {"llm_calls": 3}, {"llm_calls": 2}],
            },
        }
    }
    (raw_discover_dir / "Victoria_001_raw_discover.json").write_text(json.dumps(chunk), encoding="utf-8")

    report = build_accuracy_first_value_report(raw_discover_dir, pricing=Pricing(1.0, 2.0, 1000.0))

    recommended = report["modes"]["jikji-accuracy-first"]
    assert report["profiles"]["Victoria"]["selected_mode"] == "raw-fallback"
    assert recommended["hit_at_1"] == report["modes"]["raw"]["hit_at_1"]
    assert recommended["hit_at_10"] == report["modes"]["raw"]["hit_at_10"]
    assert report["headline_checks"]["hit_at_1_not_lower_than_raw"] is True
    assert report["headline_checks"]["hit_at_10_not_lower_than_raw"] is True
    assert report["modes"]["raw"]["call_distribution"]["max"] == 45
    assert report["modes"]["raw"]["estimated_cost"]["usd"] == 0.0014


def test_accuracy_first_profile_floor_keeps_jikji_savings_when_it_beats_raw(tmp_path):
    from jikji.benchmark_value import build_accuracy_first_value_report

    raw_discover_dir = tmp_path / "chunks"
    raw_discover_dir.mkdir()
    chunk = {
        "modes": {
            "raw": {
                "metrics": {
                    "cases": 2,
                    "hit_at_1": 0.5,
                    "hit_at_10": 0.5,
                    "llm_calls": 20,
                    "prompt_tokens": 1000,
                    "completion_tokens": 100,
                    "total_tokens": 1100,
                    "seconds": 50.0,
                },
                "details": [{"llm_calls": 12}, {"llm_calls": 8}],
            },
            "jikji-discover": {
                "metrics": {
                    "cases": 2,
                    "hit_at_1": 1.0,
                    "hit_at_10": 1.0,
                    "llm_calls": 2,
                    "prompt_tokens": 100,
                    "completion_tokens": 10,
                    "total_tokens": 110,
                    "seconds": 5.0,
                },
                "details": [{"llm_calls": 1}, {"llm_calls": 1}],
            },
        }
    }
    (raw_discover_dir / "Adam_001_raw_discover.json").write_text(json.dumps(chunk), encoding="utf-8")

    report = build_accuracy_first_value_report(raw_discover_dir)

    assert report["profiles"]["Adam"]["selected_mode"] == "jikji-discover"
    assert report["modes"]["jikji-accuracy-first"]["llm_calls"] == 2
    assert report["savings"]["jikji-accuracy-first_vs_raw"]["llm_calls_saved"] == 18


def test_accuracy_first_report_merges_zero_chat_answer_pack(tmp_path):
    from jikji.benchmark_value import build_accuracy_first_value_report

    raw_discover_dir = tmp_path / "chunks"
    raw_discover_dir.mkdir()
    chunk = {
        "modes": {
            "raw": {
                "metrics": {
                    "cases": 1,
                    "hit_at_1": 1.0,
                    "hit_at_10": 1.0,
                    "llm_calls": 5,
                    "prompt_tokens": 50,
                    "completion_tokens": 5,
                    "total_tokens": 55,
                    "seconds": 10.0,
                },
                "details": [{"llm_calls": 5}],
            },
            "jikji-discover": {
                "metrics": {
                    "cases": 1,
                    "hit_at_1": 1.0,
                    "hit_at_10": 1.0,
                    "llm_calls": 1,
                    "prompt_tokens": 10,
                    "completion_tokens": 1,
                    "total_tokens": 11,
                    "seconds": 1.0,
                },
                "details": [{"llm_calls": 1}],
            },
        }
    }
    (raw_discover_dir / "Adam_001_raw_discover.json").write_text(json.dumps(chunk), encoding="utf-8")
    answer_pack = tmp_path / "answer_pack.json"
    answer_pack.write_text(
        json.dumps(
            {
                "summary": {
                    "jikji-answer-pack": {
                        "cases": 1,
                        "hit_at_1": 0.0,
                        "hit_at_10": 1.0,
                        "calls": 0,
                        "prompt": 0,
                        "completion": 0,
                        "total": 0,
                        "seconds": 0.25,
                    }
                }
            }
        ),
        encoding="utf-8",
    )

    report = build_accuracy_first_value_report(raw_discover_dir, answer_pack_report=answer_pack)

    assert report["modes"]["jikji-answer-pack"]["llm_calls"] == 0
    assert report["modes"]["jikji-answer-pack"]["hit_at_10"] == 1.0


def test_value_report_uses_relative_paths_for_repo_local_artifacts():
    from jikji.benchmark_value import build_accuracy_first_value_report

    raw_discover_dir = Path(".pytest-artifacts/value-report-relative").resolve()
    shutil.rmtree(raw_discover_dir, ignore_errors=True)
    raw_discover_dir.mkdir(parents=True)
    try:
        chunk = {
            "modes": {
                "raw": {
                    "metrics": {
                        "cases": 1,
                        "hit_at_1": 1.0,
                        "hit_at_10": 1.0,
                        "llm_calls": 5,
                        "prompt_tokens": 50,
                        "completion_tokens": 5,
                        "total_tokens": 55,
                        "seconds": 10.0,
                    },
                    "details": [{"llm_calls": 5}],
                },
                "jikji-discover": {
                    "metrics": {
                        "cases": 1,
                        "hit_at_1": 1.0,
                        "hit_at_10": 1.0,
                        "llm_calls": 1,
                        "prompt_tokens": 10,
                        "completion_tokens": 1,
                        "total_tokens": 11,
                        "seconds": 1.0,
                    },
                    "details": [{"llm_calls": 1}],
                },
            }
        }
        (raw_discover_dir / "Adam_001_raw_discover.json").write_text(json.dumps(chunk), encoding="utf-8")

        report = build_accuracy_first_value_report(raw_discover_dir)

        assert report["raw_discover_dir"] == ".pytest-artifacts/value-report-relative"
    finally:
        shutil.rmtree(raw_discover_dir, ignore_errors=True)
