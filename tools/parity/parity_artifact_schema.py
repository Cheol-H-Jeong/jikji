from __future__ import annotations

import json
from pathlib import Path
from typing import Final

from parity_json import Json, _normalize_json

REQUIRED_ARTIFACT_FILES: Final = frozenset({
    ".jikji/manifest.json", ".jikji/file_index.jsonl", ".jikji/folder_index.jsonl",
    ".jikji/document_index.jsonl", ".jikji/file_cards.jsonl", ".jikji/chunk_map.jsonl",
    ".jikji/search_index.sqlite", ".jikji/duplicate_map.jsonl", ".jikji/folder_profile.jsonl",
    ".jikji/corpus_profile.json", ".jikji/intent_taxonomy.json", ".jikji/autorag_manifest.json",
    ".jikji/knowledge_graph.json", ".jikji/graph_routes.jsonl", ".jikji/llm_wiki_schema.md",
    ".jikji/wiki/index.md", ".jikji/parse_errors.jsonl", ".jikji/agent_map.md",
    ".jikji/agent_routes.md", ".jikji/agent_skill_context.md", ".jikji/human_guide.md",
    ".jikji_agent_map.md",
})
REQUIRED_ARTIFACT_DIRS: Final = frozenset({".jikji/doc_text", ".jikji/doc_meta", ".jikji/wiki", ".jikji/wiki/sources"})
MANIFEST_REQUIRED_FIELDS: Final = frozenset({
    "schema_version", "generated_at", "root", "files", "folders", "documents", "docs_parsed",
    "docs_reused", "docs_failed", "parse_errors", "deleted_since_last_index", "mode",
    "non_destructive", "cache_key_policy", "owned_paths", "retired_cleanup_paths",
    "parser_required_extensions", "native_text_extensions", "source_tree_signature",
})
SOURCE_TREE_SIGNATURE_FIELDS: Final = frozenset({"algorithm", "digest", "files", "folders", "total_size", "max_mtime_ns"})
JSON_REQUIRED_FIELDS: Final = {
    ".jikji/manifest.json": MANIFEST_REQUIRED_FIELDS,
    ".jikji/knowledge_graph.json": frozenset({"schema_version", "nodes", "edges"}),
}
JSONL_REQUIRED_FIELDS: Final = {
    ".jikji/file_index.jsonl": frozenset({
        "status", "path", "name", "ext", "mime", "size", "mtime", "mtime_ns", "created",
        "modified", "sha256", "parser_required", "parse_status", "text_cache_path",
        "doc_meta_path", "keywords", "summary", "indexed_at",
    }),
    ".jikji/folder_index.jsonl": frozenset({
        "folder_id", "path", "name", "depth", "file_count_direct", "subfolder_count_direct",
        "total_size_direct", "top_extensions_direct", "child_folders", "keywords", "summary",
    }),
    ".jikji/document_index.jsonl": frozenset({"file_id", "path", "text_cache_path", "doc_meta_path", "parse_status"}),
    ".jikji/graph_routes.jsonl": frozenset({
        "path", "source_id", "wiki_path", "folder", "terms", "intents", "ext", "parse_status",
        "text_cache_path", "preview",
    }),
    ".jikji/parse_errors.jsonl": frozenset({"path", "code", "stage", "error"}),
}

def _artifact_schema_errors(root: Path) -> tuple[list[str], frozenset[str]]:
    errors: list[str] = []
    validated_paths: set[str] = set()
    for rel in sorted(REQUIRED_ARTIFACT_FILES):
        path = root / rel
        if not path.is_file():
            errors.append(rel)
    for rel in sorted(REQUIRED_ARTIFACT_DIRS):
        if not (root / rel).is_dir():
            errors.append(rel)
    for rel, fields in JSON_REQUIRED_FIELDS.items():
        path = root / rel
        if not path.is_file():
            continue
        loaded = _load_json_artifact(path, rel, errors)
        if loaded is None:
            continue
        validated_paths.add(rel)
        _append_missing_fields(errors, rel, loaded, fields)
        if rel == ".jikji/manifest.json":
            signature = loaded.get("source_tree_signature")
            if isinstance(signature, dict):
                _append_missing_fields(errors, f"{rel}:source_tree_signature", signature, SOURCE_TREE_SIGNATURE_FIELDS)
            else:
                errors.append(f"{rel}: source_tree_signature is not an object")
    for rel, fields in JSONL_REQUIRED_FIELDS.items():
        path = root / rel
        if not path.is_file():
            continue
        validated_paths.add(rel)
        _validate_jsonl_artifact(path, rel, fields, errors)
    for path in sorted((root / ".jikji" / "doc_meta").glob("sha256_*.json")):
        rel = path.relative_to(root).as_posix()
        loaded = _load_json_artifact(path, rel, errors)
        if loaded is None:
            continue
        validated_paths.add(rel)
        _append_missing_fields(
            errors,
            rel,
            loaded,
            frozenset({"schema_version", "file_id", "path", "source", "parser"}),
        )
    _validate_remaining_json_artifacts(root, errors, validated_paths)
    deduped_errors = _dedupe(errors)
    errored_paths = {_artifact_error_path(error) for error in deduped_errors}
    return deduped_errors, frozenset(path for path in validated_paths if path not in errored_paths)
def _load_json_artifact(path: Path, rel: str, errors: list[str]) -> dict[str, Json] | None:
    try:
        loaded = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        errors.append(f"{rel}: invalid JSON at line {error.lineno}")
        return None
    if not isinstance(loaded, dict):
        errors.append(f"{rel}: JSON artifact is not an object")
        return None
    return loaded
def _validate_jsonl_artifact(
    path: Path,
    rel: str,
    fields: frozenset[str],
    errors: list[str],
) -> None:
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line:
            continue
        try:
            loaded = json.loads(line)
        except json.JSONDecodeError as error:
            errors.append(f"{rel}: invalid JSONL at line {line_number}: column {error.colno}")
            continue
        if not isinstance(loaded, dict):
            errors.append(f"{rel}: row {line_number} is not an object")
            continue
        _append_missing_fields(errors, f"{rel}:row {line_number}", loaded, fields)
def _validate_remaining_json_artifacts(root: Path, errors: list[str], validated_paths: set[str]) -> None:
    index_dir = root / ".jikji"
    if not index_dir.is_dir():
        return
    for path in sorted(index_dir.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if rel in validated_paths:
            continue
        suffix = path.suffix
        if suffix == ".json":
            if _load_json_value(path, rel, errors) is not None:
                validated_paths.add(rel)
        elif suffix == ".jsonl":
            _validate_jsonl_artifact(path, rel, frozenset(), errors)
            validated_paths.add(rel)
def _load_json_value(path: Path, rel: str, errors: list[str]) -> Json:
    try:
        return _normalize_json(json.loads(path.read_text(encoding="utf-8")))
    except json.JSONDecodeError as error:
        errors.append(f"{rel}: invalid JSON at line {error.lineno}")
        return None


def _append_missing_fields(
    errors: list[str],
    rel: str,
    loaded: dict[str, Json],
    fields: frozenset[str],
) -> None:
    missing = sorted(fields - set(loaded))
    if missing:
        errors.append(f"{rel}: missing required fields {missing}")


def _dedupe(items: list[str]) -> list[str]:
    return list(dict.fromkeys(items))


def _artifact_error_path(error: str) -> str:
    return error.split(":", 1)[0]
