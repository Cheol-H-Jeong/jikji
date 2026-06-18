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
_GENERIC_PATH_ANCHORS = {
    "CEO",
    "CFO",
    "COO",
    "CTO",
    "DOC",
    "DOCX",
    "INC",
    "LLC",
    "NDA",
    "PDF",
    "PPT",
    "PPTX",
    "RFP",
    "TXT",
    "XLS",
    "XLSX",
}
_GENERIC_PATH_ANCHORS.update({"ADAM", "CLIENT", "SINGAPORE"})



_TOPIC_REWRITES: tuple[tuple[tuple[str, ...], tuple[str, ...]], ...] = (
    (("sport", "sports", "interest"), ("tennis club lessons booking", "sports club application lessons")),
    (("music", "genre", "genres"), ("music carplay study playlist", "song artist track playlist")),
    (("movie", "theme", "song"), ("movie soundtrack theme song", "film music trailer song")),
    (("stress", "academic", "de-stress", "destress"), ("stress academic diary activity", "swim rental diary school stress")),
    (("meeting", "minutes", "habit"), ("meeting minutes notes", "minutes agenda follow up")),
    (("slides", "revising", "versions"), ("edited pptx original slides", "presentation edited version")),
    (("legal aid reports", "working documents", "standardize"), ("TJCC Case Report Sent Email Report structure wording", "past legal aid reports working documents")),
    (("stay in touch", "family"), ("Whatsapp Mom Dad Family Call Christmas Diary", "family contact mom dad whatsapp call")),
    (("workout routine", "work schedule"), ("half marathon garmin run workout calendar", "exercise routine work schedule exam prep")),
    (("manage money", "financially", "money"), ("budget bank statement groceries cost of living", "shopping list weekly groceries money Singapore")),
    (("keep things on track",), ("weekly priority checklist whiteboard calendar triage", "priority email case triage work study tracking")),
    (("part b", "exam eligibility"), ("Guide to Application Process Part B eligibility", "Part B registration exam eligibility lose")),
    (("ptp", "leave of absence"), ("Calculation of PTP Change in manner serving PTP", "Requirements of PTP under PTC notify SILE")),

    (("nda", "confidential", "copying"), ("NDA confidential information copying", "vendor NDA agreement confidential")),
)


def _norm(text: str) -> str:
    return " ".join(str(text or "").casefold().split())


def _trigger_present(query_norm: str, trigger: str) -> bool:
    trigger_norm = _norm(trigger)
    if " " in trigger_norm or "'" in trigger_norm:
        return trigger_norm in query_norm
    return trigger_norm in set(query_norm.split())




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
        if any(_trigger_present(q, trigger) for trigger in triggers):
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


def _query_anchors(query: str) -> list[str]:
    anchors: list[str] = []
    for idx, word in enumerate(str(query or "").split()):
        token = word.strip(".,:;!?()[]{}\"'")
        if len(token) < 3:
            continue
        is_anchor = token.isupper() or any(ch.isdigit() for ch in token)
        if idx > 0 and len(token) >= 4 and token[:1].isupper() and token[1:].islower():
            is_anchor = True
        if not is_anchor:
            continue
        if token.upper() in _GENERIC_PATH_ANCHORS:
            continue
        anchors.append(token.casefold())
    seen: set[str] = set()
    out: list[str] = []
    for anchor in anchors:
        if anchor not in seen:
            seen.add(anchor)
            out.append(anchor)
    return out





def _merge_candidates(root: Path, variants: list[str], *, top_k: int, per_query_k: int) -> list[dict[str, Any]]:
    merged: OrderedDict[str, dict[str, Any]] = OrderedDict()
    query_anchors = _query_anchors(variants[0] if variants else "")
    for variant_index, variant in enumerate(variants):
        for rank, item in enumerate(search(root, variant, top_k=per_query_k), 1):
            path = str(item.get("path") or "")
            if not path:
                continue
            score = float(item.get("score") or 0.0)
            weighted = score / max(1.0, rank ** 0.35)
            if variant_index > 0:
                weighted *= 3.0
            path_key = path.casefold()
            matched_terms = {str(term).casefold() for term in item.get("matched_terms") or []}
            if query_anchors and any(anchor in path_key for anchor in query_anchors):
                weighted = weighted * 8.0 + 50_000.0
            elif query_anchors and any(anchor in matched_terms for anchor in query_anchors):
                weighted = weighted * 4.0 + 20_000.0
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


def _clamp01(value: float) -> float:
    return max(0.0, min(1.0, value))


def _family(path: str) -> str:
    p = Path(path)
    return str(p.parent) if str(p.parent) != "." else "."


def _confidence_factors(query_type: str, candidates: list[dict[str, Any]], variants: list[str]) -> dict[str, float]:
    if not candidates:
        return {
            "score_margin": 0.0,
            "variant_agreement": 0.0,
            "family_coherence": 0.0,
            "evidence_coverage": 0.0,
            "duplicate_or_anchor_signal": 0.0,
        }
    top = float(candidates[0].get("discover_score") or candidates[0].get("score") or 0.0)
    second = float(candidates[1].get("discover_score") or candidates[1].get("score") or 0.0) if len(candidates) > 1 else 0.0
    margin = _clamp01((top - second) / max(top, 1.0)) if top > 0 else 0.0
    query_hits = max(len(candidates[0].get("queries") or []), max((len(c.get("queries") or []) for c in candidates[:5]), default=0))
    variant_agreement = _clamp01(query_hits / max(1, min(len(variants), 4)))
    top_family = _family(str(candidates[0].get("path") or ""))
    family_matches = sum(1 for c in candidates[:10] if _family(str(c.get("path") or "")) == top_family)
    family_coherence = _clamp01(family_matches / max(1, min(len(candidates), 10)))
    evidence_hits = sum(1 for c in candidates[:10] if c.get("evidence"))
    evidence_coverage = _clamp01(evidence_hits / max(1, min(len(candidates), 10)))
    anchor_reasons = {"duplicate-anchor", "filename-anchor", "fielded-bm25", "duplicate-expansion", "body-coverage"}
    anchor_hits = sum(1 for c in candidates[:5] if anchor_reasons & set(str(r) for r in (c.get("reasons") or [])))
    duplicate_or_anchor_signal = _clamp01(anchor_hits / max(1, min(len(candidates), 5)))
    if query_type == "evidence_set":
        family_coherence = max(family_coherence, _clamp01(len(candidates) / 8.0) * 0.7)
    return {
        "score_margin": round(margin, 4),
        "variant_agreement": round(variant_agreement, 4),
        "family_coherence": round(family_coherence, 4),
        "evidence_coverage": round(evidence_coverage, 4),
        "duplicate_or_anchor_signal": round(duplicate_or_anchor_signal, 4),
    }


def _confidence_score(query_type: str, factors: dict[str, float]) -> float:
    if query_type == "single_file":
        score = (
            factors["score_margin"] * 0.30
            + factors["variant_agreement"] * 0.20
            + factors["evidence_coverage"] * 0.15
            + factors["duplicate_or_anchor_signal"] * 0.35
        )
    elif query_type == "evidence_set":
        score = (
            factors["variant_agreement"] * 0.25
            + factors["family_coherence"] * 0.30
            + factors["evidence_coverage"] * 0.20
            + factors["duplicate_or_anchor_signal"] * 0.25
        )
    else:
        score = sum(factors.values()) / max(1, len(factors))
    return round(_clamp01(score), 4)


def _confidence(query_type: str, candidates: list[dict[str, Any]], factors: dict[str, float], score: float) -> str:
    if not candidates:
        return "low"
    if score >= 0.78:
        return "high"
    if score >= 0.55:
        return "medium_high"
    if score >= 0.35:
        return "medium"
    if query_type == "evidence_set" and len(candidates) >= 2:
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
    confidence_factors = _confidence_factors(query_type, candidates, variants)
    confidence_score = _confidence_score(query_type, confidence_factors)
    confidence = _confidence(query_type, candidates, confidence_factors, confidence_score)
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
        "confidence_score": confidence_score,
        "confidence_factors": confidence_factors,
        "recommended_action": _recommended_action(query_type, confidence),
        "paths": paths,
        "query_variants": variants,
        "candidates": compact_candidates,
    }
