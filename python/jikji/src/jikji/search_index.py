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
INSTANT_SEARCH_SCHEMA_VERSION = 3
_DOC_TEXT_INDEX_CHARS = 64_000

_TOKEN_RE = re.compile(r"[0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿][0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿_.+-]*")
_CJK_RE = re.compile(r"[가-힣ぁ-ゟ゠-ヿ一-鿿]")
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


def _cjk_ngrams(text: str, *, limit: int = 32) -> list[str]:
    compact = re.sub(r"[^0-9a-z가-힣ぁ-ゟ゠-ヿ一-鿿]+", "", (text or "").casefold())
    if len(compact) < 2 or not _CJK_RE.search(compact):
        return []
    out: list[str] = []
    seen: set[str] = set()
    for n in (4, 3, 2):
        if len(compact) < n:
            continue
        for idx in range(0, len(compact) - n + 1):
            gram = compact[idx:idx + n]
            if gram in seen:
                continue
            seen.add(gram)
            out.append(gram)
            if len(out) >= limit:
                return out
    return out


def _compact(text: str) -> str:
    return re.sub(r"[^0-9a-z가-힣ぁ-ゟ゠-ヿ一-鿿]+", "", (text or "").casefold())


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


def _read_cached_doc_text(root: Path, cache_path: str, *, limit: int = _DOC_TEXT_INDEX_CHARS) -> str:
    if not cache_path:
        return ""
    path = root / cache_path
    try:
        if path.is_file():
            return path.read_text(encoding="utf-8", errors="ignore")[:limit]
        if path.is_dir():
            parts: list[str] = []
            total = 0
            for chunk in sorted(path.glob("chunk_*.txt")):
                text = chunk.read_text(encoding="utf-8", errors="ignore")
                parts.append(text)
                total += len(text)
                if total >= limit:
                    break
            return "\n".join(parts)[:limit]
    except OSError:
        return ""
    return ""


# Native text/markdown corpora (e.g. materialized Markdown IR) never produce a
# parsed doc_text cache, so without this their full body would be invisible to
# ranking and Jikji would only see the first handful of extracted card terms.
# Reading the source body for these files lets the contextual-anchor BM25 layer
# in `eval._score_map` fuse folder/metadata priors with real full-text matching.
_NATIVE_BODY_CHARS = 24_000
_NATIVE_TEXT_EXTENSIONS = {
    ".md", ".markdown", ".txt", ".text", ".rst", ".log", ".csv", ".tsv",
    ".json", ".jsonl", ".yaml", ".yml", ".xml", ".html", ".htm", ".ini",
    ".cfg", ".conf", ".org", ".tex",
}


def _is_native_text_card(card: dict[str, Any]) -> bool:
    if str(card.get("text_cache_path") or ""):
        return False
    parse_status = str(card.get("parse_status") or "").lower()
    if parse_status in {"native_text", "native", "text"}:
        return True
    return str(card.get("ext") or "").lower() in _NATIVE_TEXT_EXTENSIONS


def _read_native_body(root: Path | None, card: dict[str, Any], *, limit: int = _NATIVE_BODY_CHARS) -> str:
    if root is None or not _is_native_text_card(card):
        return ""
    rel = str(card.get("path") or "")
    if not rel:
        return ""
    path = root / rel
    try:
        if not path.is_file():
            return ""
        raw = path.read_bytes()[: limit * 4]
    except OSError:
        return ""
    for enc in ("utf-8", "utf-8-sig", "cp949", "euc-kr", "latin-1"):
        try:
            return raw.decode(enc, errors="strict")[:limit]
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="ignore")[:limit]


def row_from_card(card: dict[str, Any], chunks: list[dict[str, Any]], *, root: Path | None = None) -> dict[str, Any]:
    path = str(card.get("path") or "")
    doc_text = _read_cached_doc_text(root, str(card.get("text_cache_path") or "")) if root is not None else ""
    native_body = _read_native_body(root, card)
    body_text = native_body
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
    source_text = "\n".join([*(str(x) for x in evidence_previews), doc_text]).strip()
    title_line = ""
    for line in (native_body or doc_text or "").splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        title_line = stripped.lstrip("#").strip()
        break
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
        "_source_text": source_text,
        "_body_text": body_text[:_NATIVE_BODY_CHARS].casefold(),
        "_native_title": title_line.casefold(),
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
                doc_text,
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
            str(row.get("_source_text") or "")[2000:24000],
            str(row.get("_map_text") or "")[:3000],
            str(row.get("_body_text") or "")[:24000],
            " ".join(str(x) for x in (card.get("rare_terms") or [])),
            " ".join(str(x) for x in (card.get("content_terms") or [])),
            " ".join(str(x) for x in (card.get("phrase_signatures") or [])),
            " ".join(str(x) for x in (card.get("intent_tags") or [])),
        ]
    )
    out: set[str] = set()
    for token in _tokens(fields, limit=512):
        out.update(_term_variants(token))
        out.update(_cjk_ngrams(token, limit=256))
    for key in row.get("filename_lookup_keys") or []:
        out.add(str(key).casefold())
        out.update(_cjk_ngrams(str(key), limit=64))
    return {term for term in out if term}

_FIELD_WEIGHTS = {
    "path": 5.0,
    "name": 6.0,
    "ext": 2.0,
    "body": 1.0,
    "meta": 2.2,
    "semantic": 3.0,
}


def _token_counts(text: str, *, limit: int = 4096) -> Counter[str]:
    counts: Counter[str] = Counter()
    for token in _tokens(text, limit=limit):
        for variant in _term_variants(token):
            counts[variant] += 1
        for gram in _cjk_ngrams(token, limit=128):
            counts[gram] += 1
    return counts


def fielded_terms_for_row(row: dict[str, Any]) -> dict[str, Counter[str]]:
    card = row.get("_map_card") or {}
    fields = {
        "path": " ".join([
            str(row.get("path") or ""),
            " ".join(str(x) for x in (row.get("filename_lookup_keys") or [])),
            " ".join(str(x) for x in (card.get("path_terms") or [])),
            " ".join(str(x) for x in (card.get("folder_terms") or [])),
        ]),
        "name": " ".join([
            str(row.get("name") or ""),
            Path(str(row.get("name") or "")).stem,
            " ".join(str(x) for x in (card.get("name_terms") or [])),
        ]),
        "ext": str(row.get("ext") or "").lstrip("."),
        "body": " ".join([
            str(row.get("_source_text") or "")[:24_000],
            str(row.get("_body_text") or "")[:24_000],
            str(row.get("_map_text") or "")[:12_000],
        ]),
        "meta": " ".join([
            str(row.get("summary") or ""),
            " ".join(str(x) for x in (row.get("semantic_hints") or [])),
            " ".join(str(x) for x in (row.get("keywords") or [])),
            " ".join(str(x) for x in (card.get("format_hints") or [])),
        ]),
        "semantic": " ".join([
            str(row.get("_map_content_text") or ""),
            str(row.get("_map_rare_text") or ""),
            str(row.get("_map_phrase_text") or ""),
            str(row.get("_map_intent_text") or ""),
            str(row.get("_map_evidence_text") or ""),
        ]),
    }
    return {field: _token_counts(text) for field, text in fields.items()}


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
    field_term_rows: list[tuple[str, str, int, int]] = []
    field_len_rows: list[tuple[int, str, int]] = []
    field_df: Counter[str] = Counter()
    field_len_totals: Counter[str] = Counter()
    df: Counter[str] = Counter()
    for doc_id, card in enumerate(file_cards, 1):
        row = row_from_card(card, chunks_by_path.get(str(card.get("path") or ""), []), root=index_dir.parent)
        rows.append(row)
        terms = terms_for_row(row)
        df.update(terms)
        term_rows.extend((term, doc_id) for term in terms)
        filename_rows.extend((str(key).casefold(), doc_id) for key in (row.get("filename_lookup_keys") or []))
        fielded = fielded_terms_for_row(row)
        field_seen: set[str] = set()
        for field, counts in fielded.items():
            field_len = sum(counts.values())
            field_len_rows.append((doc_id, field, field_len))
            field_len_totals[field] += field_len
            for term, tf in counts.items():
                field_term_rows.append((term, field, doc_id, int(tf)))
                field_seen.add(term)
        field_df.update(field_seen)

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
        con.execute("CREATE TABLE field_terms(term TEXT NOT NULL, field TEXT NOT NULL, doc_id INTEGER NOT NULL, tf INTEGER NOT NULL)")
        con.execute("CREATE TABLE field_lengths(doc_id INTEGER NOT NULL, field TEXT NOT NULL, length INTEGER NOT NULL)")
        con.execute("CREATE TABLE field_idf(term TEXT PRIMARY KEY, value REAL NOT NULL)")
        con.execute("CREATE TABLE field_avg(field TEXT PRIMARY KEY, value REAL NOT NULL)")
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
        con.executemany("INSERT INTO field_terms(term,field,doc_id,tf) VALUES(?,?,?,?)", field_term_rows)
        con.executemany("INSERT INTO field_lengths(doc_id,field,length) VALUES(?,?,?)", field_len_rows)
        con.executemany(
            "INSERT INTO idf(term,value) VALUES(?,?)",
            [(term, math.log((1 + total) / (1 + freq)) + 1.0) for term, freq in df.items()],
        )
        con.executemany(
            "INSERT INTO field_idf(term,value) VALUES(?,?)",
            [(term, math.log((total - freq + 0.5) / (freq + 0.5) + 1.0)) for term, freq in field_df.items()],
        )
        con.executemany(
            "INSERT INTO field_avg(field,value) VALUES(?,?)",
            [(field, total_len / total) for field, total_len in sorted(field_len_totals.items())],
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
        con.execute("CREATE INDEX idx_field_terms_term ON field_terms(term)")
        con.execute("CREATE INDEX idx_field_terms_doc ON field_terms(doc_id)")
        con.commit()
    finally:
        con.close()
    tmp.replace(path)
    return path
