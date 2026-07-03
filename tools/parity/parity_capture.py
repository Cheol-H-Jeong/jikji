from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import TYPE_CHECKING, Final

from parity_json import Json, _normalize_json, _normalize_text

if TYPE_CHECKING:
    from parity_commands import CommandRun

ARTIFACT_EXTENSIONS: Final = frozenset({".json", ".jsonl", ".md", ".txt"})


def _capture_artifacts(root: Path) -> list[dict[str, Json]]:
    rows: list[dict[str, Json]] = []
    for path in sorted((root / ".jikji").rglob("*")) + sorted(root.glob(".jikji_agent_map.md")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if path.suffix in ARTIFACT_EXTENSIONS:
            text = _normalized_artifact_text(path, root)
            digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
        else:
            digest = "<BINARY_ARTIFACT>"
        rows.append({"path": rel, "sha256": digest, "bytes": path.stat().st_size})
    return rows


def _normalized_artifact_text(path: Path, root: Path) -> str:
    text = _normalize_text(path.read_text(encoding="utf-8", errors="replace"), root)
    if path.suffix == ".json":
        return json.dumps(_normalize_json(json.loads(text)), ensure_ascii=False, indent=2, sort_keys=False) + "\n"
    if path.suffix == ".jsonl":
        lines = [
            json.dumps(_normalize_json(json.loads(line)), ensure_ascii=False, sort_keys=False)
            for line in text.splitlines()
            if line
        ]
        return "\n".join(lines) + ("\n" if lines else "")
    return text


def _command_record(run: CommandRun) -> dict[str, Json]:
    return {
        "name": run.name,
        "command": list(run.args),
        "exit_code": run.exit_code,
        "stdout": run.stdout,
        "stderr": run.stderr,
        "stdout_json": run.stdout_json,
    }
