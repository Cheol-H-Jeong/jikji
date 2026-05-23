from __future__ import annotations

import json

from jikji.agent_index import build_agent_index
from jikji.config import Config


def _jsonl(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def test_prepare_is_non_destructive_and_writes_jikji_artifacts(tmp_path):
    src = tmp_path / "기존" / "회의"
    src.mkdir(parents=True)
    doc = src / "회의록.txt"
    doc.write_text("Jikji smoke", encoding="utf-8")

    result = build_agent_index(tmp_path, Config())

    assert doc.exists()
    assert result.files == 1
    assert (tmp_path / ".jikji" / "agent_map.md").exists()
    assert (tmp_path / "000_JIKJI_AGENT_MAP.md").exists()
    rows = _jsonl(tmp_path / ".jikji" / "file_index.jsonl")
    assert rows[0]["path"] == "기존/회의/회의록.txt"


def test_prepare_prunes_deleted_document_cache(tmp_path):
    doc = tmp_path / "보고서.rtf"
    doc.write_text(r"{\rtf1\ansi Jikji stale body}", encoding="utf-8")
    build_agent_index(tmp_path, Config())
    rows = _jsonl(tmp_path / ".jikji" / "document_index.jsonl")
    assert rows
    cache = tmp_path / rows[0]["text_cache_path"]
    assert cache.exists()

    doc.unlink()
    build_agent_index(tmp_path, Config())

    assert not cache.exists()
    file_rows = _jsonl(tmp_path / ".jikji" / "file_index.jsonl")
    assert any(r.get("status") == "deleted" and r.get("path") == "보고서.rtf" for r in file_rows)
