from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Final

Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]
TIME_KEYS: Final = frozenset({"generated_at", "indexed_at", "mtime", "mtime_ns", "created", "modified"})

def _normalize_json(value) -> Json:
    if value is None or isinstance(value, bool | int | float | str):
        return value
    if isinstance(value, list):
        return [_normalize_json(item) for item in value]
    if isinstance(value, dict):
        return {
            str(key): "<TIMESTAMP>" if str(key) in TIME_KEYS else _normalize_json(item)
            for key, item in value.items()
        }
    raise TypeError(f"unsupported JSON value: {type(value).__name__}")


def _normalize_text(text: str, root: Path, retry_proof: str = "") -> str:
    normalized = text
    for root_form in sorted({str(root), str(root.resolve())}, key=len, reverse=True):
        normalized = normalized.replace(root_form, "<SCENARIO_ROOT>")
    normalized = re.sub(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})",
        "<TIMESTAMP>",
        normalized,
    )
    if retry_proof:
        normalized = normalized.replace(retry_proof, "<RETRY_PROOF>")
    return normalized
def _read_json(path: Path) -> Json:
    return json.loads(path.read_text(encoding="utf-8"))
