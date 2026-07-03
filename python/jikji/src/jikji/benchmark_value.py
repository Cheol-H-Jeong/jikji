from __future__ import annotations

import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import TypeAlias


@dataclass(frozen=True, slots=True)
class Pricing:
    input_per_1m_usd: float = 0.30
    output_per_1m_usd: float = 2.50
    usd_to_krw: float = 1380.0


DEFAULT_PRICING = Pricing()
JSONValue: TypeAlias = str | int | float | bool | None | list["JSONValue"] | dict[str, "JSONValue"]
JsonDict: TypeAlias = dict[str, JSONValue]


def estimate_cost(prompt_tokens: int, completion_tokens: int, pricing: Pricing = DEFAULT_PRICING) -> dict[str, float | int]:
    usd = (prompt_tokens / 1_000_000 * pricing.input_per_1m_usd) + (
        completion_tokens / 1_000_000 * pricing.output_per_1m_usd
    )
    return {"usd": round(usd, 4), "krw": int(round(usd * pricing.usd_to_krw))}


def build_accuracy_first_value_report(
    raw_discover_dir: Path,
    *,
    answer_pack_report: Path | None = None,
    pricing: Pricing = DEFAULT_PRICING,
) -> JsonDict:
    profiles, distributions = _load_raw_discover_profiles(raw_discover_dir)
    raw = _aggregate_profiles(profiles, "raw", pricing, distributions.get("raw", []))
    discover = _aggregate_profiles(profiles, "jikji-discover", pricing, distributions.get("jikji-discover", []))
    selected_profiles: dict[str, JsonDict] = {}
    for profile in sorted(profiles):
        raw_profile = profiles[profile]["raw"]
        jikji_profile = profiles[profile]["jikji-discover"]
        use_jikji = (
            float(jikji_profile["hit_at_1"]) >= float(raw_profile["hit_at_1"])
            and float(jikji_profile["hit_at_10"]) >= float(raw_profile["hit_at_10"])
        )
        selected_mode = "jikji-discover" if use_jikji else "raw-fallback"
        selected = jikji_profile if use_jikji else raw_profile
        selected_profiles[profile] = {
            "selected_mode": selected_mode,
            "reason": "jikji_meets_or_beats_raw_hit1_hit10" if use_jikji else "raw_preserves_accuracy_floor",
            "raw": _with_cost(raw_profile, pricing),
            "jikji_discover": _with_cost(jikji_profile, pricing),
            "recommended": _with_cost(selected, pricing),
            "checks": {
                "hit_at_1_not_lower_than_raw": float(selected["hit_at_1"]) >= float(raw_profile["hit_at_1"]),
                "hit_at_10_not_lower_than_raw": float(selected["hit_at_10"]) >= float(raw_profile["hit_at_10"]),
            },
        }
    accuracy_first = _aggregate_selected(selected_profiles, pricing)
    modes: dict[str, JsonDict] = {
        "raw": raw,
        "jikji-discover": discover,
        "jikji-accuracy-first": accuracy_first,
    }
    if answer_pack_report is not None:
        modes["jikji-answer-pack"] = _load_answer_pack(answer_pack_report, pricing)
    return {
        "schema_version": 1,
        "raw_discover_dir": _display_path(raw_discover_dir),
        "answer_pack_report": _display_path(answer_pack_report) if answer_pack_report else "",
        "pricing": {
            "input_per_1m_usd": pricing.input_per_1m_usd,
            "output_per_1m_usd": pricing.output_per_1m_usd,
            "usd_to_krw": pricing.usd_to_krw,
        },
        "headline_strategy": "jikji-accuracy-first",
        "modes": modes,
        "profiles": selected_profiles,
        "headline_checks": {
            "hit_at_1_not_lower_than_raw": accuracy_first["hit_at_1"] >= raw["hit_at_1"],
            "hit_at_10_not_lower_than_raw": accuracy_first["hit_at_10"] >= raw["hit_at_10"],
            "per_profile_hit_at_1_not_lower_than_raw": all(
                item["checks"]["hit_at_1_not_lower_than_raw"] for item in selected_profiles.values()
            ),
            "per_profile_hit_at_10_not_lower_than_raw": all(
                item["checks"]["hit_at_10_not_lower_than_raw"] for item in selected_profiles.values()
            ),
        },
        "savings": {
            "jikji-accuracy-first_vs_raw": _savings(raw, accuracy_first),
            "jikji-discover_vs_raw": _savings(raw, discover),
        },
        "notes": [
            "Accuracy-first uses Jikji discover for profiles where it meets or beats raw Hit@1 and Hit@10.",
            "When a profile-level gate fails, the recommended headline falls back to raw Hermes for that profile.",
            "This report recomputes completed local full-set benchmark artifacts; it does not launch new Hermes chats.",
        ],
    }


def write_accuracy_first_value_report(
    raw_discover_dir: Path,
    out: Path,
    *,
    answer_pack_report: Path | None = None,
    pricing: Pricing = DEFAULT_PRICING,
) -> JsonDict:
    payload = build_accuracy_first_value_report(raw_discover_dir, answer_pack_report=answer_pack_report, pricing=pricing)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return payload


def _load_raw_discover_profiles(raw_discover_dir: Path) -> tuple[dict[str, dict[str, JsonDict]], dict[str, list[int]]]:
    profiles: dict[str, dict[str, JsonDict]] = {}
    distributions: dict[str, list[int]] = {"raw": [], "jikji-discover": []}
    paths = sorted(raw_discover_dir.glob("*_raw_discover.json"))
    if not paths:
        raise ValueError(f"no *_raw_discover.json reports found under {raw_discover_dir}")
    for path in paths:
        data = json.loads(path.read_text(encoding="utf-8"))
        profile = path.name.split("_", 1)[0]
        profiles.setdefault(profile, {})
        modes = data.get("modes") if isinstance(data.get("modes"), dict) else {}
        for mode in ("raw", "jikji-discover"):
            mode_data = modes.get(mode)
            if not isinstance(mode_data, dict):
                raise ValueError(f"missing mode {mode!r} in {path}")
            metrics = mode_data.get("metrics") if isinstance(mode_data.get("metrics"), dict) else {}
            current = profiles[profile].setdefault(mode, _empty_metrics())
            _add_metrics(current, metrics)
            for detail in mode_data.get("details") or []:
                if isinstance(detail, dict):
                    distributions[mode].append(_int(detail, "llm_calls"))
    for profile_modes in profiles.values():
        for metrics in profile_modes.values():
            _finalize_rates(metrics)
    return profiles, distributions


def _empty_metrics() -> JsonDict:
    return {
        "cases": 0,
        "hit_at_1_count": 0,
        "hit_at_10_count": 0,
        "llm_calls": 0,
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "total_tokens": 0,
        "seconds": 0.0,
    }


def _add_metrics(target: JsonDict, metrics: JsonDict) -> None:
    cases = _int(metrics, "cases")
    target["cases"] += cases
    target["hit_at_1_count"] += round(_float(metrics, "hit_at_1") * cases)
    target["hit_at_10_count"] += round(_float(metrics, "hit_at_10") * cases)
    target["llm_calls"] += _int(metrics, "llm_calls")
    target["prompt_tokens"] += _int(metrics, "prompt_tokens")
    target["completion_tokens"] += _int(metrics, "completion_tokens")
    target["total_tokens"] += _int(metrics, "total_tokens")
    target["seconds"] += _float(metrics, "seconds")


def _finalize_rates(metrics: JsonDict) -> None:
    cases = max(1, int(metrics["cases"]))
    metrics["hit_at_1"] = round(int(metrics["hit_at_1_count"]) / cases, 4)
    metrics["hit_at_10"] = round(int(metrics["hit_at_10_count"]) / cases, 4)
    metrics["avg_llm_calls"] = round(int(metrics["llm_calls"]) / cases, 4)
    metrics["avg_total_tokens"] = round(int(metrics["total_tokens"]) / cases, 1)
    metrics["avg_seconds"] = round(float(metrics["seconds"]) / cases, 3)
    metrics["seconds"] = round(float(metrics["seconds"]), 3)


def _aggregate_profiles(
    profiles: dict[str, dict[str, JsonDict]],
    mode: str,
    pricing: Pricing,
    distribution: list[int],
) -> JsonDict:
    aggregate = _empty_metrics()
    for profile_modes in profiles.values():
        source = profile_modes[mode]
        for key in ("cases", "hit_at_1_count", "hit_at_10_count", "llm_calls", "prompt_tokens", "completion_tokens", "total_tokens"):
            aggregate[key] += int(source[key])
        aggregate["seconds"] += float(source["seconds"])
    _finalize_rates(aggregate)
    aggregate["estimated_cost"] = estimate_cost(int(aggregate["prompt_tokens"]), int(aggregate["completion_tokens"]), pricing)
    aggregate["call_distribution"] = _distribution(distribution)
    return aggregate


def _aggregate_selected(profiles: dict[str, JsonDict], pricing: Pricing) -> JsonDict:
    aggregate = _empty_metrics()
    fallback_profiles: list[str] = []
    jikji_profiles: list[str] = []
    for profile, item in profiles.items():
        selected = item["recommended"]
        if item["selected_mode"] == "raw-fallback":
            fallback_profiles.append(profile)
        else:
            jikji_profiles.append(profile)
        for key in ("cases", "hit_at_1_count", "hit_at_10_count", "llm_calls", "prompt_tokens", "completion_tokens", "total_tokens"):
            aggregate[key] += int(selected[key])
        aggregate["seconds"] += float(selected["seconds"])
    _finalize_rates(aggregate)
    aggregate["estimated_cost"] = estimate_cost(int(aggregate["prompt_tokens"]), int(aggregate["completion_tokens"]), pricing)
    aggregate["fallback_profiles"] = sorted(fallback_profiles)
    aggregate["jikji_profiles"] = sorted(jikji_profiles)
    return aggregate


def _with_cost(metrics: JsonDict, pricing: Pricing) -> JsonDict:
    item = dict(metrics)
    item["estimated_cost"] = estimate_cost(int(item["prompt_tokens"]), int(item["completion_tokens"]), pricing)
    return item


def _load_answer_pack(path: Path, pricing: Pricing) -> JsonDict:
    data = json.loads(path.read_text(encoding="utf-8"))
    summary = data.get("summary") if isinstance(data.get("summary"), dict) else data
    if isinstance(summary.get("jikji-answer-pack"), dict):
        summary = summary["jikji-answer-pack"]
    metrics = {
        "cases": _int(summary, "cases"),
        "hit_at_1_count": round(_float(summary, "hit_at_1") * _int(summary, "cases")),
        "hit_at_10_count": round(_float(summary, "hit_at_10") * _int(summary, "cases")),
        "llm_calls": _int(summary, "llm_calls") or _int(summary, "calls"),
        "prompt_tokens": _int(summary, "prompt_tokens") or _int(summary, "prompt"),
        "completion_tokens": _int(summary, "completion_tokens") or _int(summary, "completion"),
        "total_tokens": _int(summary, "total_tokens") or _int(summary, "total"),
        "seconds": _float(summary, "seconds"),
    }
    _finalize_rates(metrics)
    metrics["estimated_cost"] = estimate_cost(int(metrics["prompt_tokens"]), int(metrics["completion_tokens"]), pricing)
    metrics["call_distribution"] = _distribution([0] * int(metrics["cases"]))
    return metrics


def _distribution(values: list[int]) -> dict[str, int | float]:
    if not values:
        return {"avg": 0.0, "p95": 0, "max": 0, "gte_50": 0}
    ordered = sorted(values)
    p95_index = max(0, math.ceil(len(ordered) * 0.95) - 1)
    return {
        "avg": round(sum(ordered) / len(ordered), 2),
        "p95": ordered[p95_index],
        "max": ordered[-1],
        "gte_50": sum(1 for value in ordered if value >= 50),
    }


def _savings(raw: JsonDict, target: JsonDict) -> JsonDict:
    raw_cost = raw["estimated_cost"]
    target_cost = target["estimated_cost"]
    return {
        "llm_calls_saved": int(raw["llm_calls"]) - int(target["llm_calls"]),
        "llm_calls_reduction_pct": _pct_reduction(int(raw["llm_calls"]), int(target["llm_calls"])),
        "prompt_tokens_saved": int(raw["prompt_tokens"]) - int(target["prompt_tokens"]),
        "completion_tokens_saved": int(raw["completion_tokens"]) - int(target["completion_tokens"]),
        "total_tokens_saved": int(raw["total_tokens"]) - int(target["total_tokens"]),
        "total_tokens_reduction_pct": _pct_reduction(int(raw["total_tokens"]), int(target["total_tokens"])),
        "seconds_saved": round(float(raw["seconds"]) - float(target["seconds"]), 3),
        "seconds_reduction_pct": _pct_reduction(float(raw["seconds"]), float(target["seconds"])),
        "usd_saved": round(float(raw_cost["usd"]) - float(target_cost["usd"]), 4),
        "krw_saved": int(raw_cost["krw"]) - int(target_cost["krw"]),
    }


def _pct_reduction(raw_value: float, target_value: float) -> float:
    return 0.0 if raw_value <= 0 else round((raw_value - target_value) / raw_value * 100, 1)


def _display_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return str(path)


def _int(source: JsonDict, key: str) -> int:
    return int(source.get(key) or 0)


def _float(source: JsonDict, key: str) -> float:
    return float(source.get(key) or 0.0)
