"""Adaptive Jikji discovery cascade for local-agent file retrieval."""
from __future__ import annotations

from collections import OrderedDict
from pathlib import Path
from typing import Any

from .eval import search

_SINGLE_HINTS = {
    "which", "what file", "find the", "locate", "contract", "agreement", "nda",
    "invoice", "report", "form", "pdf", "document", "file", "where is",
}
_BROAD_HINTS = {
    "habit", "habits", "usual", "usually", "summarize", "summary", "profile",
    "primary", "preferred", "preference", "interest", "interests", "genres",
    "records", "past versions", "how i've", "how i", "what are my",
}
_EVIDENCE_HINTS = {
    "supporting", "evidence", "records", "versions", "minutes", "items",
    "responsibilities", "tasks", "plans", "before", "after", "history",
}

_TOPIC_REWRITES: tuple[tuple[tuple[str, ...], tuple[str, ...]], ...] = (
    (("sport", "sports", "interest"), ("tennis club lessons booking", "sports club application lessons")),
    (("music", "genre", "genres"), ("music carplay study playlist", "song artist track playlist")),
    (("movie", "theme", "song"), ("movie soundtrack theme song", "film music trailer song")),
    (("stress", "academic", "de-stress", "destress"), ("stress academic diary activity", "swim rental diary school stress")),
    (("meeting", "minutes", "habit"), ("meeting minutes notes", "minutes agenda follow up")),
    (("slides", "revising", "versions"), ("edited pptx original slides", "presentation edited version")),
    (("nda", "confidential", "copying"), ("NDA confidential information copying", "vendor NDA agreement confidential")),
)


def _norm(text: str) -> str:
    return " ".join(str(text or "").casefold().split())


def classify_query(query: str) -> str:
    q = _norm(query)
    if any(hint in q for hint in _BROAD_HINTS):
        return "evidence_set"
    if any(hint in q for hint in _EVIDENCE_HINTS):
        return "evidence_set"
    if any(hint in q for hint in _SINGLE_HINTS):
        return "single_file"
    return "adaptive"


def query_variants(query: str) -> list[str]:
    q = _norm(query)
    variants: list[str] = [query]
    for triggers, rewrites in _TOPIC_REWRITES:
        if any(trigger in q for trigger in triggers):
            variants.extend(rewrites)
    # Keep quoted/capitalized-looking anchors available without relying on the agent.
    words = [w.strip(".,:;!?()[]{}\"'") for w in str(query).split()]
    anchors = [w for w in words if len(w) >= 3 and (w.isupper() or any(ch.isdigit() for ch in w))]
    if anchors:
        variants.append(" ".join(anchors))
    out: list[str] = []
    seen: set[str] = set()
    for variant in variants:
        key = _norm(variant)
        if key and key not in seen:
            seen.add(key)
            out.append(variant)
    return out[:6]


def _merge_candidates(root: Path, variants: list[str], *, top_k: int, per_query_k: int) -> list[dict[str, Any]]:
    merged: OrderedDict[str, dict[str, Any]] = OrderedDict()
    for variant in variants:
        for rank, item in enumerate(search(root, variant, top_k=per_query_k), 1):
            path = str(item.get("path") or "")
            if not path:
                continue
            score = float(item.get("score") or 0.0)
            weighted = score / max(1.0, rank ** 0.35)
            existing = merged.get(path)
            if existing is None:
                clone = dict(item)
                clone["discover_score"] = weighted
                clone["queries"] = [variant]
                clone["best_query_rank"] = rank
                merged[path] = clone
            else:
                existing["discover_score"] = float(existing.get("discover_score") or 0.0) + weighted * 0.35
                existing.setdefault("queries", [])
                if variant not in existing["queries"]:
                    existing["queries"].append(variant)
                existing["best_query_rank"] = min(int(existing.get("best_query_rank") or rank), rank)
    ranked = sorted(
        merged.values(),
        key=lambda item: (-float(item.get("discover_score") or 0.0), int(item.get("best_query_rank") or 999), str(item.get("path") or "")),
    )
    return ranked[:top_k]


def _confidence(query_type: str, candidates: list[dict[str, Any]]) -> str:
    if not candidates:
        return "low"
    top = float(candidates[0].get("discover_score") or candidates[0].get("score") or 0.0)
    second = float(candidates[1].get("discover_score") or candidates[1].get("score") or 0.0) if len(candidates) > 1 else 0.0
    reasons = set(str(r) for r in (candidates[0].get("reasons") or []))
    if query_type == "single_file" and (top > second * 1.5 or {"duplicate-anchor", "filename-anchor", "fielded-bm25"} & reasons):
        return "high"
    if query_type == "evidence_set" and len(candidates) >= 2:
        return "medium_high"
    if len(candidates) >= 3:
        return "medium"
    return "low"


def _recommended_action(query_type: str, confidence: str) -> str:
    if query_type == "single_file" and confidence == "high":
        return "return_top1_after_light_verification"
    if query_type == "evidence_set" and confidence in {"medium", "medium_high", "high"}:
        return "return_top5_to_top10_evidence_set"
    if confidence == "low":
        return "rewrite_query_and_fallback_search"
    return "verify_top_candidates"


def discover(root: Path, query: str, *, top_k: int = 20, per_query_k: int | None = None) -> dict[str, Any]:
    root = Path(root).expanduser().resolve()
    query_type = classify_query(query)
    variants = query_variants(query)
    per_query_k = per_query_k or max(top_k, 20)
    candidates = _merge_candidates(root, variants, top_k=top_k, per_query_k=per_query_k)
    confidence = _confidence(query_type, candidates)
    paths = [str(item.get("path") or "") for item in candidates if item.get("path")]
    compact_candidates = [
        {
            "p": item.get("path"),
            "s": round(float(item.get("discover_score") or item.get("score") or 0.0), 3),
            "rank": item.get("best_query_rank"),
            "why": (item.get("reasons") or [])[:5],
            "terms": (item.get("matched_terms") or [])[:8],
            "queries": (item.get("queries") or [])[:3],
            "ev": " | ".join(str(x) for x in (item.get("evidence") or [])[:2])[:240],
        }
        for item in candidates
    ]
    return {
        "mode": "discover",
        "root": str(root),
        "query": query,
        "query_type": query_type,
        "confidence": confidence,
        "recommended_action": _recommended_action(query_type, confidence),
        "paths": paths,
        "query_variants": variants,
        "candidates": compact_candidates,
    }
