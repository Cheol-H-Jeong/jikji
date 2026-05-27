"""Persistent instant-search index for Jikji.

The main JSONL map remains the durable interchange format.  This SQLite file is
an Everything-style accelerator generated during ``jikji prepare`` so repeated
``jikji search`` calls do not have to rebuild an inverted index from JSONL.
"""
from __future__ import annotations

import json
import math
import re
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

INSTANT_SEARCH_INDEX = "search_index.sqlite"
INSTANT_SEARCH_SCHEMA_VERSION = 1

_TOKEN_RE = re.compile(r"[0-9A-Za-z가-힣][0-9A-Za-z가-힣_.+-]*")
_COPY_SUFFIX_RE = re.compile(r"(?:\s*\(\d+\)|\s*-\s*copy|\s+copy|_copy)$", re.IGNORECASE)
_KOREAN_PARTICLE_SUFFIXES = (
    "이라고",
    "라고",
    "으로",
    "에서",
    "에게",
    "까지",
    "부터",
    "처럼",
    "보다",
    "이나",
    "나",
    "과",
    "와",
    "은",
    "는",
    "이",
    "가",
    "을",
    "를",
    "에",
    "의",
    "도",
    "만",
    "로",
)
_STOP_TERMS = {
    "file",
    "folder",
    "document",
    "문서",
    "파일",
    "폴더",
    "관련",
    "내용",
    "있는",
    "찾기",
    "찾아줘",
}


def instant_index_path(root: Path) -> Path:
    return Path(root).expanduser().resolve() / ".jikji" / INSTANT_SEARCH_INDEX


def _tokens(text: str, *, limit: int = 512) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for match in _TOKEN_RE.finditer(text or ""):
        token = match.group(0).casefold().strip("._+-")
        if len(token) < 2 or token in _STOP_TERMS or token in seen:
            continue
        seen.add(token)
        out.append(token)
        if len(out) >= limit:
            break
    return out


def _compact(text: str) -> str:
    return re.sub(r"[^0-9a-z가-힣]+", "", (text or "").casefold())


def _duplicate_stem(path: str) -> str:
    stem = Path(Path(path).name).stem.casefold().strip()
    while True:
        cleaned = _COPY_SUFFIX_RE.sub("", stem).strip()
        if cleaned == stem:
            return cleaned
        stem = cleaned


def _filename_lookup_keys(path_or_name: str) -> list[str]:
    raw = (path_or_name or "").strip()
    name = Path(raw).name or raw
    stem = Path(name).stem or name
    keys = {_compact(raw), _compact(name), _compact(stem), _compact(_duplicate_stem(name))}
    return sorted(key for key in keys if key)


def _term_variants(term: str) -> set[str]:
    term = term.casefold().strip()
    variants = {term} if term else set()
    if not term:
        return variants
    for suffix in _KOREAN_PARTICLE_SUFFIXES:
        if term.endswith(suffix) and len(term) > len(suffix) + 1:
            variants.add(term[: -len(suffix)])
            break
    compact = re.sub(r"[\s_·ㆍ.,;:()\[\]{}<>/\\\\|+-]+", "", term)
    if compact and compact != term and len(compact) >= 2:
        variants.add(compact)
    return variants


def row_from_card(card: dict[str, Any], chunks: list[dict[str, Any]]) -> dict[str, Any]:
    path = str(card.get("path") or "")
    chunk_text = "\n".join(
        " ".join(
            [
                str(chunk.get("preview") or ""),
                " ".join(str(x) for x in chunk.get("content_terms") or []),
                " ".join(str(x) for x in chunk.get("rare_terms") or []),
                " ".join(str(x) for x in chunk.get("phrase_signatures") or []),
                " ".join(str(x) for x in chunk.get("intent_tags") or []),
            ]
        )
        for chunk in chunks[:48]
    )
    content_terms = [str(x) for x in (card.get("content_terms") or [])]
    rare_terms = [str(x) for x in (card.get("rare_terms") or [])]
    phrase_signatures = [str(x) for x in (card.get("phrase_signatures") or [])]
    intent_tags = [str(x) for x in (card.get("intent_tags") or [])]
    format_hints = [str(x) for x in (card.get("format_hints") or [])]
    evidence_previews = [str(x) for x in (card.get("evidence_previews") or [])]
    return {
        "path": path,
        "name": card.get("name", ""),
        "ext": card.get("ext", ""),
        "sha256": card.get("sha256", ""),
        "duplicate_group_id": card.get("duplicate_group_id", ""),
        "filename_lookup_keys": list(card.get("filename_lookup_keys") or _filename_lookup_keys(path)),
        "keywords": content_terms + rare_terms + phrase_signatures,
        "semantic_hints": (
            intent_tags
            + list(card.get("folder_roles") or [])
            + format_hints
            + list(card.get("path_terms") or [])
            + list(card.get("name_terms") or [])
            + list(card.get("folder_terms") or [])
        ),
        "summary": card.get("summary", ""),
        "_source_text": "\n".join(str(x) for x in evidence_previews),
        "_map_card": card,
        "_map_chunks": chunks,
        "_map_text": chunk_text,
        "_map_content_text": " ".join(content_terms).casefold(),
        "_map_rare_text": " ".join(rare_terms).casefold(),
        "_map_phrase_text": " ".join(phrase_signatures).casefold(),
        "_map_intent_text": " ".join(intent_tags).casefold(),
        "_map_format_text": " ".join(format_hints).casefold(),
        "_map_evidence_text": " ".join(evidence_previews + [str(card.get("summary") or "")]).casefold(),
        "_map_all_text": " ".join(
            [
                " ".join(content_terms),
                " ".join(rare_terms),
                " ".join(phrase_signatures),
                " ".join(intent_tags),
                " ".join(format_hints),
                " ".join(evidence_previews),
                str(card.get("summary") or ""),
                chunk_text,
            ]
        ).casefold(),
    }


def terms_for_row(row: dict[str, Any]) -> set[str]:
    card = row.get("_map_card") or {}
    fields = " ".join(
        [
            str(row.get("path") or ""),
            str(row.get("name") or ""),
            " ".join(str(x) for x in (row.get("keywords") or [])),
            " ".join(str(x) for x in (row.get("semantic_hints") or [])),
            str(row.get("summary") or "")[:1000],
            str(row.get("_source_text") or "")[:2000],
            str(row.get("_map_text") or "")[:3000],
            " ".join(str(x) for x in (card.get("rare_terms") or [])),
            " ".join(str(x) for x in (card.get("content_terms") or [])),
            " ".join(str(x) for x in (card.get("phrase_signatures") or [])),
            " ".join(str(x) for x in (card.get("intent_tags") or [])),
        ]
    )
    out: set[str] = set()
    for token in _tokens(fields, limit=512):
        out.update(_term_variants(token))
    for key in row.get("filename_lookup_keys") or []:
        out.add(str(key).casefold())
    return {term for term in out if term}


def build_instant_search_index(
    index_dir: Path,
    file_cards: list[dict[str, Any]],
    chunk_rows: list[dict[str, Any]],
) -> Path:
    index_dir = Path(index_dir)
    path = index_dir / INSTANT_SEARCH_INDEX
    tmp = path.with_suffix(".sqlite.tmp")
    for candidate in (tmp, path):
        try:
            candidate.unlink()
        except FileNotFoundError:
            pass

    chunks_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for chunk in chunk_rows:
        rel = str(chunk.get("path") or "")
        if rel:
            chunks_by_path[rel].append(chunk)

    rows: list[dict[str, Any]] = []
    term_rows: list[tuple[str, int]] = []
    filename_rows: list[tuple[str, int]] = []
    df: Counter[str] = Counter()
    for doc_id, card in enumerate(file_cards, 1):
        row = row_from_card(card, chunks_by_path.get(str(card.get("path") or ""), []))
        rows.append(row)
        terms = terms_for_row(row)
        df.update(terms)
        term_rows.extend((term, doc_id) for term in terms)
        filename_rows.extend((str(key).casefold(), doc_id) for key in (row.get("filename_lookup_keys") or []))

    total = max(1, len(rows))
    con = sqlite3.connect(str(tmp))
    try:
        con.execute("PRAGMA journal_mode=OFF")
        con.execute("PRAGMA synchronous=OFF")
        con.execute("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        con.execute(
            "CREATE TABLE docs("
            "id INTEGER PRIMARY KEY, path TEXT NOT NULL, name TEXT NOT NULL, ext TEXT NOT NULL, "
            "duplicate_group_id TEXT NOT NULL, row_json TEXT NOT NULL)"
        )
        con.execute("CREATE TABLE terms(term TEXT NOT NULL, doc_id INTEGER NOT NULL)")
        con.execute("CREATE TABLE filename_keys(key TEXT NOT NULL, doc_id INTEGER NOT NULL)")
        con.execute("CREATE TABLE idf(term TEXT PRIMARY KEY, value REAL NOT NULL)")
        con.executemany(
            "INSERT INTO docs(id,path,name,ext,duplicate_group_id,row_json) VALUES(?,?,?,?,?,?)",
            [
                (
                    idx,
                    str(row.get("path") or ""),
                    str(row.get("name") or ""),
                    str(row.get("ext") or ""),
                    str(row.get("duplicate_group_id") or ""),
                    json.dumps(row, ensure_ascii=False, sort_keys=True),
                )
                for idx, row in enumerate(rows, 1)
            ],
        )
        con.executemany("INSERT INTO terms(term,doc_id) VALUES(?,?)", term_rows)
        con.executemany("INSERT INTO filename_keys(key,doc_id) VALUES(?,?)", filename_rows)
        con.executemany(
            "INSERT INTO idf(term,value) VALUES(?,?)",
            [(term, math.log((1 + total) / (1 + freq)) + 1.0) for term, freq in df.items()],
        )
        con.executemany(
            "INSERT INTO meta(key,value) VALUES(?,?)",
            [
                ("schema_version", str(INSTANT_SEARCH_SCHEMA_VERSION)),
                ("rows", str(len(rows))),
                ("terms", str(len(term_rows))),
            ],
        )
        con.execute("CREATE INDEX idx_terms_term ON terms(term)")
        con.execute("CREATE INDEX idx_filename_keys_key ON filename_keys(key)")
        con.commit()
    finally:
        con.close()
    tmp.replace(path)
    return path
