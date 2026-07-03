from __future__ import annotations

import json
from pathlib import Path


def test_two_call_policy_counts_two_calls_when_top_k_contains_answer(tmp_path: Path) -> None:
    from jikji.benchmark_two_call import build_two_call_value_report

    raw_dir = tmp_path / "raw"
    answer_dir = tmp_path / "answer"
    raw_dir.mkdir()
    answer_dir.mkdir()
    _write_raw_report(raw_dir, profile="Adam", raw_hit1=0.0, raw_hit10=0.0)
    _write_answer_report(answer_dir, profile="Adam", rank=3)

    report = build_two_call_value_report(raw_dir, answer_pack_dir=answer_dir)

    mode = report["modes"]["jikji-two-call-judge"]
    assert mode["cases"] == 1
    assert mode["hit_at_1"] == 1.0
    assert mode["hit_at_10"] == 1.0
    assert mode["llm_calls"] == 2
    assert report["headline_strategy"] == "jikji-one-call-raw-floor"


def test_two_call_policy_rewrites_once_when_top_k_misses_answer(tmp_path: Path) -> None:
    from jikji.benchmark_two_call import build_two_call_value_report

    raw_dir = tmp_path / "raw"
    answer_dir = tmp_path / "answer"
    raw_dir.mkdir()
    answer_dir.mkdir()
    _write_raw_report(raw_dir, profile="Bei", raw_hit1=0.0, raw_hit10=0.0)
    _write_answer_report(answer_dir, profile="Bei", rank=None)

    report = build_two_call_value_report(raw_dir, answer_pack_dir=answer_dir)

    mode = report["modes"]["jikji-two-call-judge"]
    assert mode["hit_at_1"] == 0.0
    assert mode["llm_calls"] == 4
    assert mode["retry_cases"] == 1
    assert mode["call_distribution"]["max"] == 4


def test_two_call_policy_headline_beats_raw_fixture(tmp_path: Path) -> None:
    from jikji.benchmark_two_call import build_two_call_value_report

    raw_dir = tmp_path / "raw"
    answer_dir = tmp_path / "answer"
    raw_dir.mkdir()
    answer_dir.mkdir()
    _write_raw_report(raw_dir, profile="Dana", raw_hit1=0.0, raw_hit10=0.0)
    _write_answer_report(answer_dir, profile="Dana", rank=1)
    _write_raw_report(raw_dir, profile="Eli", raw_hit1=0.0, raw_hit10=0.0)
    _write_answer_report(answer_dir, profile="Eli", rank=8)

    report = build_two_call_value_report(raw_dir, answer_pack_dir=answer_dir)

    raw = report["modes"]["raw"]
    two_call = report["modes"]["jikji-two-call-judge"]
    one_call_floor = report["modes"]["jikji-one-call-raw-floor"]
    assert two_call["hit_at_1"] >= raw["hit_at_1"]
    assert two_call["hit_at_10"] >= raw["hit_at_10"]
    assert two_call["llm_calls"] < raw["llm_calls"]
    assert one_call_floor["call_distribution"]["avg"] == 1.0
    assert one_call_floor["call_distribution"]["max"] == 1


def test_one_call_policy_falls_back_to_raw_when_slate_loses(tmp_path: Path) -> None:
    from jikji.benchmark_two_call import build_two_call_value_report

    raw_dir = tmp_path / "raw"
    answer_dir = tmp_path / "answer"
    raw_dir.mkdir()
    answer_dir.mkdir()
    _write_raw_report(raw_dir, profile="Casey", raw_hit1=1.0, raw_hit10=1.0)
    _write_answer_report(answer_dir, profile="Casey", rank=None)

    report = build_two_call_value_report(raw_dir, answer_pack_dir=answer_dir)

    modes = report["modes"]
    assert "jikji-one-call-judge" in modes
    assert "jikji-one-call-raw-floor" in modes
    assert report["headline_strategy"] == "jikji-one-call-raw-floor"
    assert report["one_call_policy"]["profiles"]["Casey"]["selected_mode"] == "raw-fallback"
    assert modes["jikji-one-call-raw-floor"]["hit_at_1"] == modes["raw"]["hit_at_1"]
    assert modes["jikji-one-call-raw-floor"]["hit_at_10"] == modes["raw"]["hit_at_10"]
    assert modes["jikji-one-call-raw-floor"]["call_distribution"]["avg"] == 10.0
    assert modes["jikji-one-call-raw-floor"]["call_distribution"]["max"] == 10


def _write_raw_report(raw_dir: Path, *, profile: str, raw_hit1: float, raw_hit10: float) -> None:
    payload = {
        "modes": {
            "raw": {
                "metrics": {
                    "cases": 1,
                    "hit_at_1": raw_hit1,
                    "hit_at_10": raw_hit10,
                    "llm_calls": 10,
                    "prompt_tokens": 100,
                    "completion_tokens": 10,
                    "total_tokens": 110,
                    "seconds": 10.0,
                },
                "details": [{"llm_calls": 10}],
            },
            "jikji-discover": {
                "metrics": {
                    "cases": 1,
                    "hit_at_1": 0.0,
                    "hit_at_10": 0.0,
                    "llm_calls": 2,
                    "prompt_tokens": 20,
                    "completion_tokens": 2,
                    "total_tokens": 22,
                    "seconds": 1.0,
                },
                "details": [{"llm_calls": 2}],
            },
        }
    }
    (raw_dir / f"{profile}_001_raw_discover.json").write_text(json.dumps(payload), encoding="utf-8")


def _write_answer_report(answer_dir: Path, *, profile: str, rank: int | None) -> None:
    payload = {
        "modes": {
            "jikji-answer-pack": {
                "details": [
                    {
                        "id": "case-1",
                        "query": "find the right contract",
                        "rank": rank,
                        "predicted_paths": ["a.txt", "b.txt", "c.txt"],
                        "seconds": 0.5,
                    }
                ]
            }
        }
    }
    (answer_dir / f"{profile}_jikji_answer_pack_report.json").write_text(json.dumps(payload), encoding="utf-8")
