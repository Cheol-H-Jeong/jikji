"""Regression gates for Hermes raw-vs-Jikji benchmark reports."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

_ZERO_CHAT_MODES = {"jikji-answer-pack", "answer-pack", "discover-direct"}


def _usage_status_ok(mode: str, status: str) -> bool:
    if status == "ok":
        return True
    return status == "not_applicable_zero_chat" and mode in _ZERO_CHAT_MODES


def _read_metrics(path: Path, mode: str | None = None) -> dict[str, Any]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    modes = data.get("modes") if isinstance(data.get("modes"), dict) else {}
    if mode is None:
        if len(modes) != 1:
            raise ValueError(f"mode required for {path}; available={sorted(modes)}")
        mode = next(iter(modes))
    if mode not in modes:
        raise ValueError(f"mode {mode!r} not found in {path}; available={sorted(modes)}")
    metrics = modes[mode].get("metrics") if isinstance(modes[mode], dict) else None
    if not isinstance(metrics, dict):
        raise ValueError(f"missing metrics for mode {mode!r} in {path}")
    return metrics


def compare_benchmark_reports(
    raw_report: Path,
    jikji_report: Path,
    *,
    raw_mode: str = "raw",
    jikji_mode: str = "jikji-discover",
    max_token_ratio: float = 0.75,
    max_call_ratio: float = 0.75,
    max_seconds_ratio: float = 1.0,
    max_avg_llm_calls: float | None = None,
    max_p95_llm_calls: int | None = None,
) -> dict[str, Any]:
    raw = _read_metrics(raw_report, raw_mode)
    jikji = _read_metrics(jikji_report, jikji_mode)
    raw_usage_status = str(raw.get("usage_status") or "missing_usage")
    jikji_usage_status = str(jikji.get("usage_status") or "missing_usage")
    checks = {
        "hit_at_1_not_lower": float(jikji.get("hit_at_1") or 0.0) >= float(raw.get("hit_at_1") or 0.0),
        "hit_at_10_not_lower": float(jikji.get("hit_at_10") or 0.0) >= float(raw.get("hit_at_10") or 0.0),
        "total_tokens_below_ratio": float(jikji.get("total_tokens") or 0.0) <= float(raw.get("total_tokens") or 0.0) * max_token_ratio,
        "llm_calls_below_ratio": float(jikji.get("llm_calls") or 0.0) <= float(raw.get("llm_calls") or 0.0) * max_call_ratio,
        "seconds_not_slower": float(jikji.get("seconds") or 0.0) <= float(raw.get("seconds") or 0.0) * max_seconds_ratio,
        "usage_accounting_ok": _usage_status_ok(raw_mode, raw_usage_status) and _usage_status_ok(jikji_mode, jikji_usage_status),
    }
    if max_avg_llm_calls is not None:
        checks["avg_llm_calls_below_budget"] = float(jikji.get("avg_llm_calls") or 0.0) <= max_avg_llm_calls
    if max_p95_llm_calls is not None:
        checks["p95_llm_calls_below_budget"] = int(jikji.get("p95_llm_calls") or 0) <= max_p95_llm_calls
    ratios = {
        "total_tokens": (float(jikji.get("total_tokens") or 0.0) / max(1.0, float(raw.get("total_tokens") or 0.0))),
        "llm_calls": (float(jikji.get("llm_calls") or 0.0) / max(1.0, float(raw.get("llm_calls") or 0.0))),
        "seconds": (float(jikji.get("seconds") or 0.0) / max(1.0, float(raw.get("seconds") or 0.0))),
    }
    return {
        "ok": all(checks.values()),
        "raw_mode": raw_mode,
        "jikji_mode": jikji_mode,
        "checks": checks,
        "ratios": {k: round(v, 4) for k, v in ratios.items()},
        "raw": raw,
        "jikji": jikji,
        "thresholds": {
            "max_token_ratio": max_token_ratio,
            "max_call_ratio": max_call_ratio,
            "max_seconds_ratio": max_seconds_ratio,
            "max_avg_llm_calls": max_avg_llm_calls,
            "max_p95_llm_calls": max_p95_llm_calls,
            "allowed_usage_statuses": {
                "all_modes": ["ok"],
                "zero_chat_modes": sorted(_ZERO_CHAT_MODES),
                "zero_chat_status": "not_applicable_zero_chat",
            },
        },
    }
