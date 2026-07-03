"""Repeatable benchmark-improvement loop records for Jikji search quality."""
from __future__ import annotations

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .agent_index import _atomic_write_text
from .hippocamp import BenchResult, run_benchmark


@dataclass
class ImprovementLoopResult:
    report_path: Path
    iterations: int
    best_metrics: dict[str, Any]


def _load_report(path: Path | None) -> dict[str, Any] | None:
    if not path:
        return None
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def _miss_summary(report_path: Path, mode: str = "jikji", limit: int = 5) -> list[dict[str, Any]]:
    report = _load_report(report_path) or {}
    details = (((report.get("modes") or {}).get(mode) or {}).get("details") or [])
    misses = []
    for detail in details:
        if detail.get("rank") != 1:
            misses.append({
                "id": detail.get("id"),
                "rank": detail.get("rank"),
                "scenario": detail.get("scenario"),
                "query": str(detail.get("query") or "")[:160],
                "top_paths": [item.get("path") for item in (detail.get("top_results") or [])[:3]],
            })
        if len(misses) >= limit:
            break
    return misses


def run_improvement_loop(
    root: Path,
    *,
    eval_set: Path,
    iterations: int = 20,
    modes: tuple[str, ...] = ("raw", "jikji"),
    top_k: int = 5,
    out: Path | None = None,
    baseline_report: Path | None = None,
) -> ImprovementLoopResult:
    """Run at least N deterministic benchmark repeats and save a stability journal.

    This is deliberately a replay/stability harness, not an automatic optimizer:
    implementation changes must be made in code, then this command verifies that
    the same external eval set produces stable raw-vs-Jikji metrics repeatedly.
    """
    root = Path(root).expanduser().resolve()
    eval_set = Path(eval_set).expanduser().resolve()
    iterations = max(1, iterations)
    if out is None:
        out = eval_set.parent / f"jikji_improvement_loop_{root.name}.json"
    out = Path(out).expanduser().resolve()
    baseline = _load_report(baseline_report)
    baseline_metrics = (baseline or {}).get("metrics") or {}
    journal: dict[str, Any] = {
        "root": str(root),
        "eval_set": str(eval_set),
        "iterations_requested": iterations,
        "baseline_report": str(baseline_report) if baseline_report else "",
        "baseline_metrics": baseline_metrics,
        "iterations": [],
    }
    best: BenchResult | None = None
    best_score = -1.0
    target_mode = "jikji" if "jikji" in modes else modes[-1]
    for idx in range(1, iterations + 1):
        started = time.perf_counter()
        result = run_benchmark(root, eval_set=eval_set, modes=modes, top_k=top_k, prepare=False)
        elapsed = time.perf_counter() - started
        target = result.metrics.get(target_mode, {}) if isinstance(result.metrics, dict) else {}
        score = float(target.get("hit_at_1") or 0.0) + float(target.get("mrr") or 0.0) / 10.0
        if score >= best_score:
            best = result
            best_score = score
        journal["iterations"].append({
            "iteration": idx,
            "run_type": "deterministic_repeat_after_current_implementation",
            "target_mode": target_mode,
            "benchmark_report": str(result.report_path),
            "metrics": result.metrics,
            "elapsed_seconds": round(elapsed, 3),
            "misses_to_inspect_next": _miss_summary(result.report_path),
        })
        _atomic_write_text(out, json.dumps(journal, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    best_metrics = best.metrics if best else {}
    journal["best_metrics"] = best_metrics
    journal["completed_iterations"] = len(journal["iterations"])
    _atomic_write_text(out, json.dumps(journal, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    return ImprovementLoopResult(out, len(journal["iterations"]), best_metrics)
