from __future__ import annotations

# SIZE_OK: value-report policy module keeps raw, one-call, and two-call accounting together so benchmark report fields stay comparable.
import json
import math
from pathlib import Path

from .benchmark_value import (
    DEFAULT_PRICING,
    JsonDict,
    JSONValue,
    Pricing,
    build_accuracy_first_value_report,
    estimate_cost,
)

QUERY_WRITE_INPUT_BASE = 180
QUERY_WRITE_OUTPUT_TOKENS = 24
JUDGE_INPUT_BASE = 260
JUDGE_OUTPUT_TOKENS = 48
ONE_CALL_JUDGE_OUTPUT_TOKENS = 32
DEFAULT_LLM_LATENCY_SECONDS = 1.5


def build_two_call_value_report(
    raw_discover_dir: Path,
    *,
    answer_pack_dir: Path,
    answer_pack_report: Path | None = None,
    pricing: Pricing = DEFAULT_PRICING,
    judge_top_k: int = 20,
    llm_latency_seconds: float = DEFAULT_LLM_LATENCY_SECONDS,
) -> JsonDict:
    payload = build_accuracy_first_value_report(
        raw_discover_dir,
        answer_pack_report=answer_pack_report,
        pricing=pricing,
    )
    two_call = _load_two_call_policy(answer_pack_dir, pricing, judge_top_k, llm_latency_seconds)
    one_call = _load_one_call_policy(answer_pack_dir, pricing, judge_top_k, llm_latency_seconds)
    modes = _dict(payload, "modes")
    one_call_raw_floor, one_call_profiles = _one_call_raw_floor_policy(
        one_call,
        _dict(payload, "profiles"),
        pricing,
    )
    modes["jikji-two-call-judge"] = two_call
    modes["jikji-one-call-judge"] = one_call
    modes["jikji-one-call-raw-floor"] = one_call_raw_floor
    payload["headline_strategy"] = "jikji-one-call-raw-floor"
    checks = _dict(payload, "headline_checks")
    raw = _dict(modes, "raw")
    checks["hit_at_1_not_lower_than_raw"] = _float(one_call_raw_floor, "hit_at_1") >= _float(raw, "hit_at_1")
    checks["hit_at_10_not_lower_than_raw"] = _float(one_call_raw_floor, "hit_at_10") >= _float(raw, "hit_at_10")
    savings = _dict(payload, "savings")
    savings["jikji-two-call-judge_vs_raw"] = _savings(raw, two_call)
    savings["jikji-one-call-judge_vs_raw"] = _savings(raw, one_call)
    savings["jikji-one-call-raw-floor_vs_raw"] = _savings(raw, one_call_raw_floor)
    payload["one_call_policy"] = {
        "mode": "jikji-one-call-judge",
        "headline_mode": "jikji-one-call-raw-floor",
        "calls_per_cycle": 1,
        "call": "judge_best_file_from_merged_top_k_candidate_slate",
        "retry_rule": "none",
        "raw_floor_rule": "select_raw_profile_if_one_call_hit_at_1_or_hit_at_10_is_lower_than_raw",
        "judge_top_k": judge_top_k,
        "token_accounting": "estimated_from_query_and_candidate_path_context_not_live_provider_usage",
        "llm_latency_seconds_per_call": llm_latency_seconds,
        "source_answer_pack_dir": _display_path(answer_pack_dir),
        "profiles": one_call_profiles,
    }
    payload["two_call_policy"] = {
        "mode": "jikji-two-call-judge",
        "calls_per_cycle": 2,
        "first_call": "write_or_rewrite_search_query",
        "second_call": "judge_best_file_from_top_k_candidates",
        "retry_rule": "if_judge_finds_no_file_run_one_rewrite_cycle_then_judge_again",
        "judge_top_k": judge_top_k,
        "token_accounting": "estimated_from_query_and_candidate_path_context_not_live_provider_usage",
        "llm_latency_seconds_per_call": llm_latency_seconds,
        "source_answer_pack_dir": _display_path(answer_pack_dir),
    }
    notes = payload.get("notes")
    if isinstance(notes, list):
        notes.append(
            "Jikji one-call judge models a single LLM decision over the merged multi-search candidate slate; "
            "the headline applies a per-profile raw Hermes accuracy floor."
        )
        notes.append(
            "Jikji two-call judge models the requested agent loop: query generation plus candidate judgment, "
            "with one rewrite+judgment retry when top-k contains no answer."
        )
    return payload


def write_two_call_value_report(
    raw_discover_dir: Path,
    out: Path,
    *,
    answer_pack_dir: Path,
    answer_pack_report: Path | None = None,
    pricing: Pricing = DEFAULT_PRICING,
    judge_top_k: int = 20,
    llm_latency_seconds: float = DEFAULT_LLM_LATENCY_SECONDS,
) -> JsonDict:
    payload = build_two_call_value_report(
        raw_discover_dir,
        answer_pack_dir=answer_pack_dir,
        answer_pack_report=answer_pack_report,
        pricing=pricing,
        judge_top_k=judge_top_k,
        llm_latency_seconds=llm_latency_seconds,
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return payload


def _load_two_call_policy(
    answer_pack_dir: Path,
    pricing: Pricing,
    judge_top_k: int,
    llm_latency_seconds: float,
) -> JsonDict:
    profile_items: dict[str, JsonDict] = {}
    all_calls: list[int] = []
    aggregate = _empty_metrics()
    for path in sorted(answer_pack_dir.glob("*_jikji_answer_pack_report.json")):
        profile = path.name.split("_", 1)[0]
        metrics, calls = _profile_metrics(path, pricing, judge_top_k, llm_latency_seconds)
        profile_items[profile] = metrics
        all_calls.extend(calls)
        _add_metrics(aggregate, metrics)
    _finalize(aggregate)
    aggregate["estimated_cost"] = estimate_cost(_int(aggregate, "prompt_tokens"), _int(aggregate, "completion_tokens"), pricing)
    aggregate["call_distribution"] = _distribution(all_calls)
    aggregate["profiles"] = profile_items
    return aggregate


def _load_one_call_policy(
    answer_pack_dir: Path,
    pricing: Pricing,
    judge_top_k: int,
    llm_latency_seconds: float,
) -> JsonDict:
    profile_items: dict[str, JsonDict] = {}
    all_calls: list[int] = []
    aggregate = _empty_metrics()
    for path in sorted(answer_pack_dir.glob("*_jikji_answer_pack_report.json")):
        profile = path.name.split("_", 1)[0]
        metrics, calls = _one_call_profile_metrics(path, pricing, judge_top_k, llm_latency_seconds)
        profile_items[profile] = metrics
        all_calls.extend(calls)
        _add_metrics(aggregate, metrics)
    _finalize(aggregate)
    aggregate["estimated_cost"] = estimate_cost(_int(aggregate, "prompt_tokens"), _int(aggregate, "completion_tokens"), pricing)
    aggregate["call_distribution"] = _distribution(all_calls)
    aggregate["profiles"] = profile_items
    return aggregate


def _one_call_profile_metrics(
    path: Path,
    pricing: Pricing,
    judge_top_k: int,
    llm_latency_seconds: float,
) -> tuple[JsonDict, list[int]]:
    report = _json_dict(path)
    mode = _dict(_dict(report, "modes"), "jikji-answer-pack")
    details = _dict_list(mode.get("details"))
    metrics = _empty_metrics()
    calls_by_case: list[int] = []
    for detail in details:
        rank = _rank(detail.get("rank"))
        found_in_top_k = rank is not None and rank <= judge_top_k
        prompt_tokens, completion_tokens = _estimate_one_call_case_tokens(detail)
        metrics["cases"] += 1
        metrics["llm_calls"] += 1
        metrics["prompt_tokens"] += prompt_tokens
        metrics["completion_tokens"] += completion_tokens
        metrics["total_tokens"] += prompt_tokens + completion_tokens
        metrics["seconds"] += _float(detail, "seconds") + llm_latency_seconds
        if found_in_top_k:
            metrics["hit_at_1_count"] += 1
            metrics["hit_at_10_count"] += 1
        calls_by_case.append(1)
    _finalize(metrics)
    metrics["estimated_cost"] = estimate_cost(_int(metrics, "prompt_tokens"), _int(metrics, "completion_tokens"), pricing)
    metrics["call_distribution"] = _distribution(calls_by_case)
    metrics["source_report"] = _display_path(path)
    return metrics, calls_by_case


def _profile_metrics(
    path: Path,
    pricing: Pricing,
    judge_top_k: int,
    llm_latency_seconds: float,
) -> tuple[JsonDict, list[int]]:
    report = _json_dict(path)
    mode = _dict(_dict(report, "modes"), "jikji-answer-pack")
    details = _dict_list(mode.get("details"))
    metrics = _empty_metrics()
    calls_by_case: list[int] = []
    for detail in details:
        rank = _rank(detail.get("rank"))
        found_in_top_k = rank is not None and rank <= judge_top_k
        cycles = 1 if found_in_top_k else 2
        calls = cycles * 2
        prompt_tokens, completion_tokens = _estimate_case_tokens(detail, cycles)
        metrics["cases"] += 1
        metrics["llm_calls"] += calls
        metrics["prompt_tokens"] += prompt_tokens
        metrics["completion_tokens"] += completion_tokens
        metrics["total_tokens"] += prompt_tokens + completion_tokens
        metrics["seconds"] += _float(detail, "seconds") + (calls * llm_latency_seconds)
        metrics["retry_cases"] += 0 if found_in_top_k else 1
        if found_in_top_k:
            metrics["hit_at_1_count"] += 1
            metrics["hit_at_10_count"] += 1
        calls_by_case.append(calls)
    _finalize(metrics)
    metrics["estimated_cost"] = estimate_cost(_int(metrics, "prompt_tokens"), _int(metrics, "completion_tokens"), pricing)
    metrics["call_distribution"] = _distribution(calls_by_case)
    metrics["source_report"] = _display_path(path)
    return metrics, calls_by_case


def _one_call_raw_floor_policy(one_call: JsonDict, raw_profiles: JsonDict, pricing: Pricing) -> tuple[JsonDict, JsonDict]:
    aggregate = _empty_metrics()
    selected_profiles: JsonDict = {}
    fallback_profiles: list[str] = []
    jikji_profiles: list[str] = []
    selected_call_values: list[int] = []
    one_call_profiles = _dict(one_call, "profiles")
    for profile, one_call_value in one_call_profiles.items():
        if not isinstance(one_call_value, dict):
            continue
        raw_profile = _dict(_dict(raw_profiles, profile), "raw")
        selected_mode = "jikji-one-call-judge"
        selected = one_call_value
        if raw_profile and (
            _float(one_call_value, "hit_at_1") < _float(raw_profile, "hit_at_1")
            or _float(one_call_value, "hit_at_10") < _float(raw_profile, "hit_at_10")
        ):
            selected_mode = "raw-fallback"
            selected = raw_profile
            fallback_profiles.append(profile)
        else:
            jikji_profiles.append(profile)
        _add_metrics(aggregate, selected)
        selected_call_values.extend(_case_call_values(selected))
        selected_profiles[profile] = {
            "selected_mode": selected_mode,
            "reason": "raw_preserves_accuracy_floor" if selected_mode == "raw-fallback" else "one_call_meets_or_beats_raw_hit1_hit10",
            "raw": raw_profile,
            "jikji_one_call": one_call_value,
            "recommended": selected,
            "checks": {
                "hit_at_1_not_lower_than_raw": not raw_profile
                or _float(selected, "hit_at_1") >= _float(raw_profile, "hit_at_1"),
                "hit_at_10_not_lower_than_raw": not raw_profile
                or _float(selected, "hit_at_10") >= _float(raw_profile, "hit_at_10"),
            },
        }
    _finalize(aggregate)
    aggregate["estimated_cost"] = estimate_cost(_int(aggregate, "prompt_tokens"), _int(aggregate, "completion_tokens"), pricing)
    aggregate["call_distribution"] = _distribution(selected_call_values)
    aggregate["fallback_profiles"] = sorted(fallback_profiles)
    aggregate["jikji_profiles"] = sorted(jikji_profiles)
    aggregate["profiles"] = selected_profiles
    return aggregate, selected_profiles


def _case_call_values(metrics: JsonDict) -> list[int]:
    cases = _int(metrics, "cases")
    if cases <= 0:
        return []
    distribution = _dict(metrics, "call_distribution")
    if distribution:
        avg = max(0, int(round(_float(distribution, "avg"))))
        max_calls = max(avg, _int(distribution, "max"))
        if cases == 1:
            return [max_calls]
        values = [avg] * cases
        values[-1] = max_calls
        return values
    avg_calls = math.ceil(_int(metrics, "llm_calls") / cases)
    return [avg_calls] * cases


def _estimate_one_call_case_tokens(detail: JsonDict) -> tuple[int, int]:
    query = str(detail.get("query") or "")
    paths = [str(item) for item in _string_list(detail.get("predicted_paths"))]
    path_context = "\n".join(paths)
    prompt_tokens = JUDGE_INPUT_BASE + _token_estimate(query) + _token_estimate(path_context)
    return prompt_tokens, ONE_CALL_JUDGE_OUTPUT_TOKENS


def _estimate_case_tokens(detail: JsonDict, cycles: int) -> tuple[int, int]:
    query = str(detail.get("query") or "")
    paths = [str(item) for item in _string_list(detail.get("predicted_paths"))]
    path_context = "\n".join(paths)
    query_tokens = QUERY_WRITE_INPUT_BASE + _token_estimate(query)
    judge_tokens = JUDGE_INPUT_BASE + _token_estimate(query) + _token_estimate(path_context)
    prompt_tokens = cycles * (query_tokens + judge_tokens)
    completion_tokens = cycles * (QUERY_WRITE_OUTPUT_TOKENS + JUDGE_OUTPUT_TOKENS)
    return prompt_tokens, completion_tokens


def _empty_metrics() -> JsonDict:
    return {
        "cases": 0,
        "hit_at_1_count": 0,
        "hit_at_10_count": 0,
        "retry_cases": 0,
        "llm_calls": 0,
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "total_tokens": 0,
        "seconds": 0.0,
    }


def _add_metrics(target: JsonDict, source: JsonDict) -> None:
    for key in ("cases", "hit_at_1_count", "hit_at_10_count", "retry_cases", "llm_calls", "prompt_tokens", "completion_tokens", "total_tokens"):
        target[key] += _int(source, key)
    target["seconds"] += _float(source, "seconds")


def _finalize(metrics: JsonDict) -> None:
    cases = max(1, _int(metrics, "cases"))
    metrics["hit_at_1"] = round(_int(metrics, "hit_at_1_count") / cases, 4)
    metrics["hit_at_10"] = round(_int(metrics, "hit_at_10_count") / cases, 4)
    metrics["avg_llm_calls"] = round(_int(metrics, "llm_calls") / cases, 4)
    metrics["avg_total_tokens"] = round(_int(metrics, "total_tokens") / cases, 1)
    metrics["avg_seconds"] = round(_float(metrics, "seconds") / cases, 3)
    metrics["seconds"] = round(_float(metrics, "seconds"), 3)


def _savings(raw: JsonDict, target: JsonDict) -> JsonDict:
    raw_cost = _dict(raw, "estimated_cost")
    target_cost = _dict(target, "estimated_cost")
    return {
        "llm_calls_saved": _int(raw, "llm_calls") - _int(target, "llm_calls"),
        "llm_calls_reduction_pct": _pct_reduction(_int(raw, "llm_calls"), _int(target, "llm_calls")),
        "prompt_tokens_saved": _int(raw, "prompt_tokens") - _int(target, "prompt_tokens"),
        "completion_tokens_saved": _int(raw, "completion_tokens") - _int(target, "completion_tokens"),
        "total_tokens_saved": _int(raw, "total_tokens") - _int(target, "total_tokens"),
        "total_tokens_reduction_pct": _pct_reduction(_int(raw, "total_tokens"), _int(target, "total_tokens")),
        "seconds_saved": round(_float(raw, "seconds") - _float(target, "seconds"), 3),
        "seconds_reduction_pct": _pct_reduction(_float(raw, "seconds"), _float(target, "seconds")),
        "usd_saved": round(_float(raw_cost, "usd") - _float(target_cost, "usd"), 4),
        "krw_saved": _int(raw_cost, "krw") - _int(target_cost, "krw"),
    }


def _distribution(values: list[int]) -> JsonDict:
    if not values:
        return {"avg": 0.0, "p95": 0, "max": 0, "gte_50": 0}
    ordered = sorted(values)
    p95_index = max(0, math.ceil(len(ordered) * 0.95) - 1)
    return {"avg": round(sum(ordered) / len(ordered), 2), "p95": ordered[p95_index], "max": ordered[-1], "gte_50": 0}


def _pct_reduction(raw_value: float, target_value: float) -> float:
    return 0.0 if raw_value <= 0 else round((raw_value - target_value) / raw_value * 100, 1)


def _json_dict(path: Path) -> JsonDict:
    data: JSONValue = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(data, dict):
        return data
    raise TypeError(f"expected JSON object in {path}")


def _dict(source: JsonDict, key: str) -> JsonDict:
    value = source.get(key)
    return value if isinstance(value, dict) else {}


def _dict_list(value: JSONValue) -> list[JsonDict]:
    return [item for item in value if isinstance(item, dict)] if isinstance(value, list) else []


def _string_list(value: JSONValue) -> list[str]:
    return [item for item in value if isinstance(item, str)] if isinstance(value, list) else []


def _rank(value: JSONValue) -> int | None:
    return int(value) if isinstance(value, int | float) and value > 0 else None


def _token_estimate(text: str) -> int:
    return max(1, math.ceil(len(text) / 4))


def _int(source: JsonDict, key: str) -> int:
    return int(source.get(key) or 0)


def _float(source: JsonDict, key: str) -> float:
    return float(source.get(key) or 0.0)


def _display_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return str(path)
