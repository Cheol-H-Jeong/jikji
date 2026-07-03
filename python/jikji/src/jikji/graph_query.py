"""Query helpers for Jikji's deterministic LLM Wiki knowledge graph."""
from __future__ import annotations

import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

from .agent_index import AGENT_DIR_NAME

_TOKEN_RE = re.compile(r"[0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿][0-9A-Za-z가-힣ぁ-ゟ゠-ヿ一-鿿_.+-]*")


def _read_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def _iter_jsonl(path: Path):
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as handle:
            for line in handle:
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(row, dict):
                    yield row
    except OSError:
        return


def _tokens(text: str) -> set[str]:
    return {m.group(0).casefold().strip("._+-") for m in _TOKEN_RE.finditer(text or "") if len(m.group(0).strip("._+-")) >= 2}


def graph_status(root: Path) -> dict[str, Any]:
    root = Path(root).expanduser().resolve()
    index_dir = root / AGENT_DIR_NAME
    graph = _read_json(index_dir / "knowledge_graph.json")
    manifest = _read_json(index_dir / "manifest.json")
    stats = graph.get("stats") if isinstance(graph.get("stats"), dict) else {}
    return {
        "root": str(root),
        "prepared": bool(graph),
        "manifest": {
            "files": manifest.get("files"),
            "folders": manifest.get("folders"),
            "documents": manifest.get("documents"),
            "llm_wiki_sources": manifest.get("llm_wiki_sources"),
            "knowledge_graph_nodes": manifest.get("knowledge_graph_nodes"),
            "knowledge_graph_edges": manifest.get("knowledge_graph_edges"),
        },
        "stats": stats,
        "artifacts": {
            "wiki_index": str(index_dir / "wiki" / "index.md"),
            "knowledge_graph": str(index_dir / "knowledge_graph.json"),
            "graph_routes": str(index_dir / "graph_routes.jsonl"),
        },
    }


def query_graph_routes(root: Path, query: str, *, top_k: int = 10) -> list[dict[str, Any]]:
    root = Path(root).expanduser().resolve()
    query_terms = _tokens(query)
    if not query_terms:
        return []
    ranked: list[dict[str, Any]] = []
    for row in _iter_jsonl(root / AGENT_DIR_NAME / "graph_routes.jsonl"):
        fields = [
            row.get("path", ""),
            row.get("folder", ""),
            row.get("preview", ""),
            " ".join(str(x) for x in row.get("terms") or []),
            " ".join(str(x) for x in row.get("intents") or []),
        ]
        route_terms = _tokens("\n".join(str(x) for x in fields))
        overlap = sorted(query_terms & route_terms)
        if not overlap:
            continue
        score = len(overlap) * 100 + sum(1 for term in overlap if term in _tokens(str(row.get("path") or ""))) * 20
        ranked.append({
            "path": row.get("path", ""),
            "score": score,
            "matched_terms": overlap[:16],
            "wiki_path": row.get("wiki_path", ""),
            "text_cache_path": row.get("text_cache_path", ""),
            "folder": row.get("folder", ""),
            "intents": row.get("intents", []),
            "preview": row.get("preview", ""),
        })
    ranked.sort(key=lambda item: (-float(item.get("score") or 0), str(item.get("path") or "")))
    return ranked[: max(1, top_k)]


def explain_source(root: Path, source_path: str) -> dict[str, Any]:
    root = Path(root).expanduser().resolve()
    wanted = str(source_path or "")
    route = {}
    for row in _iter_jsonl(root / AGENT_DIR_NAME / "graph_routes.jsonl"):
        if str(row.get("path") or "") == wanted:
            route = row
            break
    graph = _read_json(root / AGENT_DIR_NAME / "knowledge_graph.json")
    source_id = str(route.get("source_id") or "")
    neighbors: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for edge in graph.get("edges") or []:
        if not isinstance(edge, dict):
            continue
        if str(edge.get("src") or "") == source_id:
            neighbors[str(edge.get("kind") or "edge")].append(edge)
        elif str(edge.get("dst") or "") == source_id:
            neighbors[str(edge.get("kind") or "edge")].append(edge)
    return {
        "root": str(root),
        "path": wanted,
        "found": bool(route),
        "route": route,
        "neighbors": {k: v[:20] for k, v in neighbors.items()},
    }
