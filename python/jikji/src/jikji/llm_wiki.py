"""Deterministic local LLM-wiki and knowledge-graph artifacts for Jikji.

The wiki layer follows the common LLM Wiki pattern (raw sources -> markdown wiki
-> graph/context packs) without requiring an LLM, network call, embeddings, or a
separate vector DB.  It is intentionally compact and source-grounded: every page
and graph edge points back to an original local path or an existing Jikji map
artifact.
"""
from __future__ import annotations

import hashlib
import json
import re
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

WIKI_DIR_NAME = "wiki"
WIKI_INDEX = "wiki/index.md"
KNOWLEDGE_GRAPH = "knowledge_graph.json"
GRAPH_ROUTES = "graph_routes.jsonl"
LLM_WIKI_SCHEMA = "llm_wiki_schema.md"

_WORD_RE = re.compile(r"[0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿][0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿_.+-]*")
_SPACE_RE = re.compile(r"\s+")


def _safe_id(prefix: str, value: str, *, length: int = 16) -> str:
    digest = hashlib.sha1(value.encode("utf-8", "ignore")).hexdigest()[:length]
    return f"{prefix}:{digest}"


def _source_slug(path: str) -> str:
    digest = hashlib.sha1(path.encode("utf-8", "ignore")).hexdigest()[:12]
    stem = re.sub(r"[^0-9A-Za-z가-힣_-]+", "-", Path(path).stem).strip("-")[:48] or "source"
    return f"{stem}-{digest}.md"


def _compact(text: Any, *, limit: int = 240) -> str:
    value = _SPACE_RE.sub(" ", str(text or "")).strip()
    if len(value) > limit:
        return value[: max(0, limit - 1)].rstrip() + "…"
    return value


def _terms(*parts: Any, limit: int = 16) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for part in parts:
        for match in _WORD_RE.finditer(str(part or "")):
            term = match.group(0).strip("._+-").casefold()
            if len(term) < 2 or term in seen:
                continue
            seen.add(term)
            out.append(term)
            if len(out) >= limit:
                return out
    return out


def _iter_preview(card: dict[str, Any], chunks: list[dict[str, Any]]) -> str:
    previews = [str(x) for x in card.get("evidence_previews") or [] if str(x).strip()]
    for chunk in chunks[:3]:
        preview = str(chunk.get("preview") or "").strip()
        if preview:
            previews.append(preview)
    return _compact(" ".join(previews) or card.get("summary") or card.get("name") or card.get("path"), limit=420)


def _source_page(card: dict[str, Any], chunks: list[dict[str, Any]], *, wiki_rel: str) -> str:
    path = str(card.get("path") or "")
    terms = [str(x) for x in (card.get("rare_terms") or card.get("content_terms") or [])[:18]]
    intents = [str(x) for x in (card.get("intent_tags") or [])[:8]]
    folders = [str(x) for x in (card.get("folder_terms") or [])[:8]]
    preview = _iter_preview(card, chunks)
    lines = [
        "---",
        "schema: jikji.llm_wiki.source.v1",
        f"source_path: {json.dumps(path, ensure_ascii=False)}",
        f"source_id: {json.dumps(_safe_id('source', path), ensure_ascii=False)}",
        f"wiki_path: {json.dumps(wiki_rel, ensure_ascii=False)}",
        f"ext: {json.dumps(str(card.get('ext') or ''), ensure_ascii=False)}",
        f"parse_status: {json.dumps(str(card.get('parse_status') or ''), ensure_ascii=False)}",
        "---",
        "",
        f"# {path}",
        "",
        "## Agent-use summary",
        f"- Original path: `{path}`",
        f"- File type: `{card.get('ext') or '[none]'}` · parse: `{card.get('parse_status') or 'unknown'}`",
        f"- Text cache: `{card.get('text_cache_path') or ''}`",
        f"- Duplicate group: `{card.get('duplicate_group_id') or ''}`",
        "",
        "## Retrieval terms",
        ", ".join(terms) if terms else "—",
        "",
        "## Intents / folders",
        f"- intents: {', '.join(intents) if intents else '—'}",
        f"- folders: {', '.join(folders) if folders else '—'}",
        "",
        "## Grounded preview",
        preview or "—",
        "",
        "## Graph links",
    ]
    for term in terms[:10]:
        lines.append(f"- term: `term:{term}`")
    for intent in intents[:6]:
        lines.append(f"- intent: `intent:{intent}`")
    lines.append("")
    return "\n".join(lines)


def build_llm_wiki_artifacts(
    index_dir: Path,
    manifest: dict[str, Any],
    file_cards: list[dict[str, Any]],
    chunk_rows: list[dict[str, Any]],
    folder_profiles: list[dict[str, Any]],
) -> dict[str, Any]:
    """Write deterministic wiki/graph artifacts and return manifest additions."""
    index_dir = Path(index_dir)
    wiki_dir = index_dir / WIKI_DIR_NAME
    sources_dir = wiki_dir / "sources"
    sources_dir.mkdir(parents=True, exist_ok=True)
    for old_page in sources_dir.glob("*.md"):
        try:
            old_page.unlink()
        except OSError:
            pass

    chunks_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for chunk in chunk_rows:
        path = str(chunk.get("path") or "")
        if path:
            chunks_by_path[path].append(chunk)

    nodes: dict[str, dict[str, Any]] = {}
    edges: list[dict[str, Any]] = []
    graph_routes: list[dict[str, Any]] = []
    term_counts: Counter[str] = Counter()
    intent_counts: Counter[str] = Counter()
    folder_counts: Counter[str] = Counter()

    def add_node(node_id: str, kind: str, label: str, **attrs: Any) -> None:
        if node_id not in nodes:
            nodes[node_id] = {"id": node_id, "kind": kind, "label": label, **attrs}

    def add_edge(src: str, dst: str, kind: str, weight: float = 1.0, **attrs: Any) -> None:
        edges.append({"src": src, "dst": dst, "kind": kind, "weight": round(float(weight), 4), **attrs})

    add_node("root", "corpus", str(manifest.get("root") or "root"), files=manifest.get("files"), folders=manifest.get("folders"))

    for folder in folder_profiles:
        folder_path = str(folder.get("path") or ".")
        node_id = _safe_id("folder", folder_path)
        add_node(node_id, "folder", folder_path, file_count=folder.get("file_count_direct", 0), roles=folder.get("roles", []))
        add_edge("root", node_id, "contains_folder", 1.0)
        for role in folder.get("roles") or []:
            role = str(role)
            intent_counts[role] += 1
            intent_id = f"intent:{role}"
            add_node(intent_id, "intent", role)
            add_edge(node_id, intent_id, "folder_has_intent", 0.7)

    for card in file_cards:
        path = str(card.get("path") or "")
        if not path:
            continue
        source_id = _safe_id("source", path)
        folder_path = Path(path).parent.as_posix() or "."
        if folder_path == "":
            folder_path = "."
        folder_id = _safe_id("folder", folder_path)
        wiki_rel = f"{WIKI_DIR_NAME}/sources/{_source_slug(path)}"
        source_page = sources_dir / Path(wiki_rel).name
        source_page.write_text(_source_page(card, chunks_by_path.get(path, []), wiki_rel=wiki_rel), encoding="utf-8")

        source_terms = [str(x).casefold() for x in (card.get("rare_terms") or [])[:12]]
        source_terms += [str(x).casefold() for x in (card.get("content_terms") or [])[:12]]
        source_terms = list(dict.fromkeys(t for t in source_terms if t))[:18]
        source_intents = [str(x) for x in (card.get("intent_tags") or [])[:8]]
        source_preview = _iter_preview(card, chunks_by_path.get(path, []))
        add_node(
            source_id,
            "source",
            path,
            path=path,
            wiki_path=f".jikji/{wiki_rel}",
            ext=card.get("ext", ""),
            parse_status=card.get("parse_status", ""),
            text_cache_path=card.get("text_cache_path", ""),
            preview=source_preview,
        )
        add_edge("root", source_id, "contains_source", 1.0)
        add_edge(folder_id, source_id, "folder_contains_source", 1.0)
        folder_counts[folder_path] += 1

        for rank, term in enumerate(source_terms, 1):
            term_counts[term] += 1
            term_id = f"term:{term}"
            add_node(term_id, "term", term)
            add_edge(source_id, term_id, "mentions", max(0.2, 1.0 - rank / 32.0))
        for intent in source_intents:
            intent_counts[intent] += 1
            intent_id = f"intent:{intent}"
            add_node(intent_id, "intent", intent)
            add_edge(source_id, intent_id, "has_intent", 0.9)
        dup = str(card.get("duplicate_group_id") or "")
        if dup:
            dup_id = f"duplicate:{dup}"
            add_node(dup_id, "duplicate_group", dup)
            add_edge(source_id, dup_id, "member_of_duplicate_group", 0.8)

        graph_routes.append({
            "schema_version": 1,
            "path": path,
            "source_id": source_id,
            "wiki_path": f".jikji/{wiki_rel}",
            "folder": folder_path,
            "terms": source_terms[:12],
            "intents": source_intents[:6],
            "ext": card.get("ext", ""),
            "parse_status": card.get("parse_status", ""),
            "text_cache_path": card.get("text_cache_path", ""),
            "preview": source_preview,
        })

    top_terms = [term for term, _ in term_counts.most_common(80)]
    top_intents = [intent for intent, _ in intent_counts.most_common(40)]
    index_lines = [
        "---",
        "schema: jikji.llm_wiki.index.v1",
        f"root: {json.dumps(str(manifest.get('root') or ''), ensure_ascii=False)}",
        f"sources: {len(file_cards)}",
        f"graph_nodes: {len(nodes)}",
        f"graph_edges: {len(edges)}",
        "---",
        "",
        "# Jikji LLM Wiki",
        "",
        "This is a deterministic local LLM Wiki compiled from the original folder without moving or modifying source files.",
        "It follows the raw-source -> markdown wiki -> graph/context-pack pattern, but every fact is extracted locally from Jikji indexes and parser caches.",
        "",
        "## Agent route",
        "1. Run `jikji find ROOT \"query\" --json` first.",
        "2. Use returned candidates and `graph_routes.jsonl` rows before opening original files.",
        "3. Read a source wiki page only when candidate evidence is ambiguous.",
        "4. Read original files only for final verification.",
        "",
        "## Top terms",
        ", ".join(top_terms) if top_terms else "—",
        "",
        "## Top intents",
        ", ".join(top_intents) if top_intents else "—",
        "",
        "## Artifacts",
        "- `.jikji/wiki/sources/*.md` — one compact grounded markdown page per source",
        "- `.jikji/knowledge_graph.json` — source/folder/term/intent/duplicate graph",
        "- `.jikji/graph_routes.jsonl` — low-token graph route rows for agents",
        "- `.jikji/llm_wiki_schema.md` — schema and safety contract",
        "",
    ]
    (wiki_dir / "index.md").write_text("\n".join(index_lines), encoding="utf-8")

    schema = """# Jikji LLM Wiki Schema

Status: deterministic local schema v1

## Layers

1. Raw source files stay in their original local paths and are never moved.
2. `.jikji/doc_text/` stores parser text caches for supported documents/media.
3. `.jikji/wiki/` stores compact markdown source pages for agent reading.
4. `.jikji/knowledge_graph.json` stores a typed graph of corpus/source/folder/term/intent/duplicate nodes.
5. `.jikji/graph_routes.jsonl` stores one low-token route row per source for fast file discovery.

## Edge kinds

- `contains_folder`
- `contains_source`
- `folder_contains_source`
- `mentions`
- `has_intent`
- `folder_has_intent`
- `member_of_duplicate_group`

## Safety

All artifacts are generated.  They may contain source-derived sensitive text snippets.  Jikji must never edit, move, rename, or delete original files while compiling this wiki.
"""
    (index_dir / LLM_WIKI_SCHEMA).write_text(schema, encoding="utf-8")

    graph = {
        "schema_version": 1,
        "root": manifest.get("root", ""),
        "source": "jikji deterministic llm-wiki compiler",
        "nodes": sorted(nodes.values(), key=lambda n: (str(n.get("kind")), str(n.get("id")))),
        "edges": sorted(edges, key=lambda e: (str(e.get("src")), str(e.get("kind")), str(e.get("dst")))),
        "stats": {
            "nodes": len(nodes),
            "edges": len(edges),
            "sources": len(file_cards),
            "folders_linked": len(folder_counts),
            "terms": len(term_counts),
            "intents": len(intent_counts),
        },
    }
    (index_dir / KNOWLEDGE_GRAPH).write_text(json.dumps(graph, ensure_ascii=False, indent=2, sort_keys=True), encoding="utf-8")
    with (index_dir / GRAPH_ROUTES).open("w", encoding="utf-8") as handle:
        for row in sorted(graph_routes, key=lambda r: str(r.get("path") or "")):
            handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")

    return {
        "llm_wiki": f".jikji/{WIKI_INDEX}",
        "knowledge_graph": f".jikji/{KNOWLEDGE_GRAPH}",
        "graph_routes": f".jikji/{GRAPH_ROUTES}",
        "llm_wiki_schema": f".jikji/{LLM_WIKI_SCHEMA}",
        "llm_wiki_sources": len(file_cards),
        "knowledge_graph_nodes": len(nodes),
        "knowledge_graph_edges": len(edges),
    }
