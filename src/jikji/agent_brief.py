"""Compact query-specific route briefs for local agents."""
from __future__ import annotations

import json
import shlex
from pathlib import Path
from typing import Any

from .agent_index import AGENT_DIR_NAME, VISIBLE_MAP_NAME


def _read_json_obj(path: Path) -> dict[str, Any]:
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


def _candidate_cards(root: Path, candidates: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    wanted = {str(item.get("path") or "") for item in candidates}
    if not wanted:
        return {}
    out: dict[str, dict[str, Any]] = {}
    for row in _iter_jsonl(root / AGENT_DIR_NAME / "file_cards.jsonl"):
        path = str(row.get("path") or "")
        if path in wanted:
            out[path] = row
        if len(out) >= len(wanted):
            break
    return out


def _candidate_folder_profiles(root: Path, candidates: list[dict[str, Any]], *, limit: int = 8) -> list[dict[str, Any]]:
    wanted = {
        (Path(str(item.get("path") or "")).parent.as_posix() or ".")
        for item in candidates
        if str(item.get("path") or "")
    }
    wanted = {"." if value == "" else value for value in wanted}
    if not wanted:
        return []
    rows = []
    for row in _iter_jsonl(root / AGENT_DIR_NAME / "folder_profile.jsonl"):
        if str(row.get("path") or ".") in wanted:
            rows.append({
                "path": row.get("path", "."),
                "roles": row.get("roles", []),
                "file_count_direct": row.get("file_count_direct", 0),
                "top_extensions_direct": row.get("top_extensions_direct", {}),
                "summary": row.get("summary", ""),
            })
        if len(rows) >= limit:
            break
    return rows


def _truncate_evidence(value: Any, limit: int) -> str:
    text = " ".join(str(value).split())
    if limit > 0 and len(text) > limit:
        text = text[: max(0, limit - 1)].rstrip() + "…"
    return text


def _shell_join(parts: list[str]) -> str:
    return " ".join(shlex.quote(str(part)) for part in parts)



def build_compact_agent_brief_payload(
    root: Path,
    query: str,
    *,
    top_k: int,
    index_status: str,
    foreground_prepared: bool,
    background_refresh_started: bool,
    candidates: list[dict[str, Any]],
    evidence_max_chars: int = 160,
) -> dict[str, Any]:
    """Return a token-minimal graph route sheet for agent file discovery."""
    root = Path(root).expanduser().resolve()
    wanted = {str(item.get("path") or "") for item in candidates}
    routes: dict[str, dict[str, Any]] = {}
    for row in _iter_jsonl(root / AGENT_DIR_NAME / "graph_routes.jsonl"):
        path = str(row.get("path") or "")
        if path in wanted:
            routes[path] = row
        if len(routes) >= len(wanted):
            break
    compact_candidates = []
    for rank, item in enumerate(candidates, 1):
        path = str(item.get("path") or "")
        route = routes.get(path, {})
        preview = route.get("preview") or (item.get("evidence") or [""])[0]
        compact_candidates.append({
            "r": rank,
            "p": path,
            "s": item.get("score"),
            "why": (item.get("reasons") or [])[:4],
            "terms": (route.get("terms") or item.get("matched_terms") or [])[:8],
            "intents": (route.get("intents") or item.get("matched_intents") or [])[:4],
            "wiki": route.get("wiki_path", ""),
            "cache": route.get("text_cache_path", ""),
            "ev": _truncate_evidence(preview, evidence_max_chars),
        })
    return {
        "schema_version": 1,
        "mode": "compact_graph_brief",
        "root": str(root),
        "q": query,
        "top_k": top_k,
        "index": index_status,
        "prepared": foreground_prepared,
        "refreshing": background_refresh_started,
        "policy": "Use candidates[].p first. If candidates are plausible, stop discovery and do not call search_files or broad grep/find/listing. Read candidates[].wiki/cache only if ambiguous. Open original only for final verification. Do not browse whole filesystem first.",
        "artifacts": {
            "graph_routes": str(root / AGENT_DIR_NAME / "graph_routes.jsonl"),
            "knowledge_graph": str(root / AGENT_DIR_NAME / "knowledge_graph.json"),
            "wiki_index": str(root / AGENT_DIR_NAME / "wiki" / "index.md"),
        },
        "candidates": compact_candidates,
        "search_loop_guard": {
            "success": bool(compact_candidates),
            "stop_search": bool(compact_candidates),
            "suppress_followup_search_files": bool(compact_candidates),
            "fallback_allowed": not bool(compact_candidates),
            "next_step": "use_candidates_p_directly" if compact_candidates else "run_sharper_jikji_query_before_raw_fallback",
        },
    }
def build_agent_brief_payload(
    root: Path,
    query: str,
    *,
    top_k: int,
    index_status: str,
    foreground_prepared: bool,
    background_refresh_started: bool,
    candidates: list[dict[str, Any]],
    evidence_max_items: int = 3,
    evidence_max_chars: int = 0,
) -> dict[str, Any]:
    root = Path(root).expanduser().resolve()
    manifest = _read_json_obj(root / AGENT_DIR_NAME / "manifest.json")
    corpus_profile = _read_json_obj(root / AGENT_DIR_NAME / "corpus_profile.json")
    cards = _candidate_cards(root, candidates)
    enriched = []
    for rank, item in enumerate(candidates, 1):
        path = str(item.get("path") or "")
        card = cards.get(path, {})
        cache = str(card.get("text_cache_path") or "")
        original = root / path
        raw_evidence = list(item.get("evidence") or [])[: max(0, evidence_max_items)]
        if evidence_max_chars > 0:
            evidence = [_truncate_evidence(x, evidence_max_chars) for x in raw_evidence]
        else:
            evidence = raw_evidence
        enriched.append({
            "rank": rank,
            "path": path,
            "score": item.get("score"),
            "reasons": item.get("reasons", []),
            "matched_terms": item.get("matched_terms", []),
            "matched_intents": item.get("matched_intents", []),
            "duplicate_group_id": item.get("duplicate_group_id", ""),
            "ext": card.get("ext", ""),
            "parse_status": card.get("parse_status", ""),
            "text_cache_path": cache,
            "evidence": evidence,
            "next_reads": [
                {"purpose": "open original file if final verification is needed", "path": str(original)},
                *(
                    [{"purpose": "search extracted document text cache", "path": str(root / cache)}]
                    if cache
                    else []
                ),
            ],
        })
    commands = {
        "repeat_ranked_search": _shell_join(["jikji", "search", str(root), query, "--top-k", str(top_k), "--json"]),
        "fallback_generated_map_rg": _shell_join([
            "rg",
            "-n",
            "--",
            query,
            str(root / AGENT_DIR_NAME / "file_cards.jsonl"),
            str(root / AGENT_DIR_NAME / "chunk_map.jsonl"),
            str(root / AGENT_DIR_NAME / "folder_profile.jsonl"),
        ]),
        "fallback_doc_text_rg": _shell_join(["rg", "-n", "--", query, str(root / AGENT_DIR_NAME / "doc_text")]),
        "last_resort_original_rg": _shell_join(["rg", "-n", "--glob", "!**/.jikji/**", "--", query, str(root)]),
    }
    return {
        "schema_version": 1,
        "root": str(root),
        "query": query,
        "top_k": top_k,
        "index_status": index_status,
        "foreground_prepared": foreground_prepared,
        "background_refresh_started": background_refresh_started,
        "agent_policy": [
            "Use candidate paths first; a plausible non-empty candidate list is a successful Jikji result.",
            "After a successful Jikji result, stop discovery and do not call search_files, grep/find, or broad directory listing for the same purpose.",
            "Return relative paths exactly as listed under candidates.path.",
            "Read original files only for final verification or when evidence is insufficient.",
            "Never move, rename, delete, or reorganize source files.",
        ],
        "search_loop_guard": {
            "success": bool(enriched),
            "stop_search": bool(enriched),
            "suppress_followup_search_files": bool(enriched),
            "fallback_allowed": not bool(enriched),
            "next_step": "use_candidate_paths_directly" if enriched else "run_sharper_jikji_query_before_raw_fallback",
        },
        "route_order": [
            "1. Trust this brief's candidates when evidence/reasons match the user request.",
            "2. If ambiguous, run repeat_ranked_search with a sharper query or larger top-k.",
            "3. If still insufficient, search file_cards/chunk_map/folder_profile.",
            "4. Search .jikji/doc_text for parser-extracted bodies.",
            "5. Search original text-like files excluding .jikji as a last resort.",
        ],
        "corpus_summary": {
            "files": manifest.get("files"),
            "folders": manifest.get("folders"),
            "documents": manifest.get("documents"),
            "chunks": manifest.get("chunks"),
            "search_index_bytes": manifest.get("search_index_bytes"),
            "top_extensions": corpus_profile.get("top_extensions", {}),
            "parse_status_counts": corpus_profile.get("parse_status_counts", {}),
        },
        "candidate_folders": _candidate_folder_profiles(root, candidates),
        "candidates": enriched,
        "commands": commands,
        "artifacts": {
            "visible_map": str(root / VISIBLE_MAP_NAME),
            "agent_routes": str(root / AGENT_DIR_NAME / "agent_routes.md"),
            "file_cards": str(root / AGENT_DIR_NAME / "file_cards.jsonl"),
            "chunk_map": str(root / AGENT_DIR_NAME / "chunk_map.jsonl"),
            "folder_profile": str(root / AGENT_DIR_NAME / "folder_profile.jsonl"),
            "search_index": str(root / AGENT_DIR_NAME / "search_index.sqlite"),
            "doc_text_dir": str(root / AGENT_DIR_NAME / "doc_text"),
        },
    }


def brief_markdown(payload: dict[str, Any]) -> str:
    lines = [
        "# Jikji Agent Brief",
        "",
        f"- Root: `{payload['root']}`",
        f"- Query: `{payload['query']}`",
        f"- Index: `{payload['index_status']}`",
        "",
        "## Agent policy",
    ]
    lines.extend(f"- {item}" for item in payload["agent_policy"])
    lines.extend(["", "## Candidate paths"])
    for item in payload["candidates"]:
        reasons = ",".join(str(x) for x in item.get("reasons") or [])
        lines.append(f"{item['rank']:02d}. `{item['path']}` — score={item.get('score')} reasons={reasons}")
        for preview in item.get("evidence") or []:
            lines.append(f"    - evidence: {preview[:220]}")
    lines.extend(["", "## Route order"])
    lines.extend(f"- {item}" for item in payload["route_order"])
    lines.extend(["", "## Useful commands"])
    for name, command in payload["commands"].items():
        lines.append(f"- {name}: `{command}`")
    lines.append("")
    return "\n".join(lines)
