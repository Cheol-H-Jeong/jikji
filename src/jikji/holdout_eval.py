"""Scorer-blind holdout evaluation set generation for Jikji.

This module intentionally does not import or call ``jikji.eval`` scoring,
query-token, filename-anchor, duplicate-query, or format-alias helpers.  The
resulting JSONL is meant as a locked benchmark artifact: generate it, record its
checksum/profile, and do not inspect individual cases while tuning retrieval.
"""
from __future__ import annotations

import hashlib
import json
import re
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .agent_index import _atomic_write_text

GENERATOR_NAME = "jikji-holdout-scorer-blind"
GENERATOR_VERSION = 1

_NOISE = {
    "file",
    "folder",
    "document",
    "문서",
    "파일",
    "자료",
    "내용",
    "관련",
    "있는",
    "찾아줘",
    "확인",
    "원본",
    "그림",
    "이름",
    "서식",
    "페이지",
    "www",
    "http",
    "https",
}
_FORMAT_LABELS = {
    ".hwp": "한글 문서",
    ".hwpx": "한글 문서",
    ".pdf": "PDF",
    ".ppt": "PPT",
    ".pptx": "PPT",
    ".xls": "엑셀",
    ".xlsx": "엑셀",
    ".doc": "워드",
    ".docx": "워드",
    ".txt": "텍스트",
    ".md": "마크다운",
    ".zip": "압축파일",
    ".html": "HTML",
    ".htm": "HTML",
}
_COPY_SUFFIX_RE = re.compile(r"(?:\s*\(\d+\)|\s*-\s*copy|\s+copy|_copy)$", re.IGNORECASE)


@dataclass
class HoldoutEvalResult:
    eval_set_path: Path
    profile_path: Path
    checksum: str
    cases: int
    scenarios: dict[str, int]


def _read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    rows: list[dict] = []
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(row, dict):
            rows.append(row)
    return rows


def _write_json(path: Path, obj: Any) -> None:
    _atomic_write_text(path, json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True) + "\n")


def _write_jsonl(path: Path, rows: list[dict]) -> None:
    _atomic_write_text(path, "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows))


def _stable_key(seed: str, *parts: str) -> str:
    return hashlib.sha256((seed + "\0" + "\0".join(parts)).encode("utf-8", "ignore")).hexdigest()


def _case_hash(case: dict) -> str:
    payload = json.dumps(case, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(payload.encode("utf-8", "ignore")).hexdigest()


def _file_sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _words(text: str, *, limit: int = 24) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for token in re.findall(r"[0-9A-Za-z가-힣][0-9A-Za-z가-힣_.-]{1,80}", text or ""):
        norm = token.casefold().strip("._-")
        if not norm or norm in seen or norm in _NOISE or len(norm) < 2:
            continue
        if norm.isdigit() and len(norm) < 4:
            continue
        seen.add(norm)
        out.append(token.strip("._-"))
        if len(out) >= limit:
            break
    return out


def _compact(text: str) -> str:
    return re.sub(r"[^0-9a-z가-힣]+", "", (text or "").casefold())


def _format_label(ext: str) -> str:
    return _FORMAT_LABELS.get(ext.lower(), ext.lower().lstrip(".") or "파일")


def _copyless_stem(path: str) -> str:
    stem = Path(path).stem.casefold().strip()
    while True:
        cleaned = _COPY_SUFFIX_RE.sub("", stem).strip()
        if cleaned == stem:
            return stem
        stem = cleaned


def _folder_label(path: str) -> str:
    parts = [p for p in Path(path).parts[:-1] if p not in {"", "."}]
    if not parts:
        return "."
    # Prefer a concrete local folder, not always the root backup folder.
    return parts[-1] if len(parts[-1]) >= 3 else parts[-2] if len(parts) >= 2 else parts[-1]


def _card_text(card: dict) -> str:
    return " ".join([
        str(card.get("name") or ""),
        " ".join(str(x) for x in (card.get("content_terms") or [])),
        " ".join(str(x) for x in (card.get("rare_terms") or [])),
        " ".join(str(x) for x in (card.get("phrase_signatures") or [])),
        " ".join(str(x) for x in (card.get("evidence_previews") or [])),
        str(card.get("summary") or ""),
    ])


def _terms(card: dict, *, limit: int = 16) -> list[str]:
    words: list[str] = []
    seen: set[str] = set()
    for source in (
        card.get("rare_terms") or [],
        card.get("content_terms") or [],
        _words(str(card.get("name") or ""), limit=limit),
    ):
        for raw in source:
            term = str(raw).strip()
            norm = term.casefold()
            if norm and norm not in seen and norm not in _NOISE and len(_compact(norm)) >= 3:
                seen.add(norm)
                words.append(term)
                if len(words) >= limit:
                    return words
    return words


def _phrases(card: dict) -> list[str]:
    phrases: list[str] = []
    for source in (card.get("phrase_signatures") or [], card.get("evidence_previews") or []):
        text = re.sub(r"\s+", " ", str(source)).strip()
        if 10 <= len(text) <= 90 and not any(noise in text.casefold() for noise in ("http", "www", "copyright")):
            phrases.append(text)
    return phrases[:4]


def generate_holdout_eval_set(
    root: Path,
    *,
    max_cases: int = 180,
    out: Path | None = None,
    seed: str = "jikji-holdout-v1",
) -> HoldoutEvalResult:
    """Generate a locked scorer-blind holdout eval set from Jikji map cards.

    The generator is deterministic but intentionally separate from the search
    scorer.  It writes a JSON profile with checksum and anti-overfit usage notes.
    Callers should not print or inspect individual cases while tuning retrieval.
    """
    root = Path(root).expanduser().resolve()
    cards = [row for row in _read_jsonl(root / ".jikji" / "file_cards.jsonl") if row.get("path")]
    if not cards:
        raise FileNotFoundError("No .jikji/file_cards.jsonl found. Run `jikji prepare ROOT` first.")

    cards_by_path = {str(card.get("path")): card for card in cards if card.get("path")}
    term_to_paths: dict[str, set[str]] = defaultdict(set)
    phrase_to_paths: dict[str, set[str]] = defaultdict(set)
    name_token_to_paths: dict[str, set[str]] = defaultdict(set)
    folder_to_paths: dict[str, set[str]] = defaultdict(set)
    copyless_to_paths: dict[str, set[str]] = defaultdict(set)

    for card in cards:
        path = str(card.get("path") or "")
        text = _card_text(card)
        for term in _terms(card, limit=48):
            term_to_paths[_compact(term)].add(path)
        for phrase in _phrases(card):
            phrase_to_paths[_compact(phrase)].add(path)
        for word in _words(str(card.get("name") or ""), limit=20):
            name_token_to_paths[_compact(word)].add(path)
        folder_to_paths[_compact(_folder_label(path))].add(path)
        copyless_to_paths[_compact(_copyless_stem(path))].add(path)
        for word in _words(text, limit=32):
            term_to_paths[_compact(word)].add(path)

    ordered = sorted(cards, key=lambda c: _stable_key(seed, str(c.get("path") or "")))
    cases: list[dict] = []
    counts: Counter[str] = Counter()
    used_queries: set[str] = set()
    used_primary: Counter[str] = Counter()
    per_scenario_cap = max(3, max_cases // 6)

    def add(scenario: str, query: str, expected: set[str], evidence: str, primary: str, **extra: Any) -> None:
        if len(cases) >= max_cases or counts[scenario] >= per_scenario_cap:
            return
        paths = sorted(p for p in expected if p in cards_by_path)
        if not paths or len(paths) > 12 or used_primary[primary] >= 2:
            return
        query = re.sub(r"\s+", " ", query).strip()
        qkey = hashlib.sha1(query.casefold().encode("utf-8", "ignore")).hexdigest()
        if qkey in used_queries:
            return
        used_queries.add(qkey)
        used_primary[primary] += 1
        counts[scenario] += 1
        case = {
            "id": f"holdout-{scenario}-{counts[scenario]:04d}",
            "scenario": scenario,
            "query": query,
            "expected_paths": paths,
            "expected_count": len(paths),
            "evidence": evidence[:360],
            "holdout": True,
            "generator": GENERATOR_NAME,
        }
        case.update(extra)
        case["case_sha256"] = _case_hash({k: v for k, v in case.items() if k != "case_sha256"})
        cases.append(case)

    for card in ordered:
        path = str(card.get("path") or "")
        ext = str(card.get("ext") or "").lower()
        label = _format_label(ext)
        terms = _terms(card, limit=12)
        compact_terms = [_compact(t) for t in terms if _compact(t) in term_to_paths]
        folder = _folder_label(path)
        folder_key = _compact(folder)

        if len(compact_terms) >= 3:
            expected = set(term_to_paths[compact_terms[0]]) & set(term_to_paths[compact_terms[1]]) & set(term_to_paths[compact_terms[2]])
            if ext:
                expected = {p for p in expected if str(cards_by_path[p].get("ext") or "").lower() == ext}
            add(
                "multi_clue_unseen",
                f"{terms[0]}, {terms[1]}, {terms[2]} 단서가 함께 보이는 {label}를 찾아줘.",
                expected,
                "independent multi-term clue",
                path,
                ext=ext,
            )

        if compact_terms and folder_key:
            expected = set(term_to_paths[compact_terms[0]]) & set(folder_to_paths.get(folder_key, set()))
            if ext:
                expected = {p for p in expected if str(cards_by_path[p].get("ext") or "").lower() == ext}
            add(
                "folder_constrained_unseen",
                f"{folder} 근처에서 {terms[0]} 단서가 나오는 {label} 후보를 찾아줘.",
                expected,
                "independent folder+term clue",
                path,
                ext=ext,
                folder_hint=folder,
            )

        for phrase in _phrases(card):
            expected = set(phrase_to_paths.get(_compact(phrase), set()))
            add(
                "phrase_memory_unseen",
                f"'{phrase}' 표현을 본 기억이 있는데 그 파일을 찾아줘.",
                expected,
                phrase,
                path,
            )
            break

        name_words = [w for w in _words(str(card.get("name") or ""), limit=12) if len(_compact(w)) >= 3]
        if name_words:
            token = name_words[-1]
            expected = set(name_token_to_paths.get(_compact(token), set()))
            if ext:
                expected = {p for p in expected if str(cards_by_path[p].get("ext") or "").lower() == ext}
            add(
                "name_token_unseen",
                f"제목에 {token} 단어가 들어간 {label} 파일을 찾아줘.",
                expected,
                str(card.get("name") or path),
                path,
                ext=ext,
            )

        stem_key = _compact(_copyless_stem(path))
        stem_group = set(copyless_to_paths.get(stem_key, set()))
        if 2 <= len(stem_group) <= 12:
            add(
                "copy_family_unseen",
                f"{Path(path).stem} 이름으로 저장된 복사본이나 버전들을 모아줘.",
                stem_group,
                "independent copyless-stem family",
                path,
            )

        if len(cases) >= max_cases:
            break

    # Add stable duplicate_map cases without using search-side duplicate logic.
    for group in sorted(_read_jsonl(root / ".jikji" / "duplicate_map.jsonl"), key=lambda g: str(g.get("group_id") or "")):
        if len(cases) >= max_cases or counts["hash_duplicate_unseen"] >= per_scenario_cap:
            break
        members = {str(p) for p in (group.get("members") or []) if str(p) in cards_by_path}
        if not (2 <= len(members) <= 12):
            continue
        rep = str(group.get("representative") or sorted(members)[0])
        add(
            "hash_duplicate_unseen",
            f"{Path(rep).name}와 실제로 같은 파일로 보이는 중복 항목들을 찾아줘.",
            members,
            "duplicate_map members only",
            rep,
            duplicate_group_id=str(group.get("group_id") or ""),
        )

    if not cases:
        raise RuntimeError("Could not generate holdout cases from this corpus")

    out_path = Path(out).expanduser().resolve() if out else root / ".jikji" / "eval" / "holdout_eval_set.jsonl"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    _write_jsonl(out_path, cases)
    checksum = _file_sha(out_path)
    profile = {
        "generator": GENERATOR_NAME,
        "generator_version": GENERATOR_VERSION,
        "root": str(root),
        "eval_set": str(out_path),
        "sha256": checksum,
        "cases": len(cases),
        "scenarios": dict(counts),
        "seed_sha256": hashlib.sha256(seed.encode("utf-8", "ignore")).hexdigest(),
        "locked_holdout": True,
        "anti_overfit_protocol": {
            "do_not_inspect_cases_while_tuning": True,
            "do_not_change_retrieval_based_on_this_set": True,
            "allowed_use": "final/regression evaluation only after changes are frozen",
            "generator_firewall": "does not import search scorer/query helpers",
        },
        "case_sha256_manifest": [case["case_sha256"] for case in cases],
    }
    profile_path = out_path.with_suffix(".profile.json")
    _write_json(profile_path, profile)
    return HoldoutEvalResult(out_path, profile_path, checksum, len(cases), dict(counts))
