from __future__ import annotations

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .discover import discover


@dataclass
class AnswerPackAttempt:
    payload: dict[str, Any]
    candidates: list[dict[str, str]]
    predicted: list[str]
    stdout: str
    stderr: str
    returncode: int
    seconds: float


def _ranked_answer_pack_paths(payload: dict[str, Any]) -> list[str]:
    paths: list[str] = []
    for key in ("answer_paths", "supporting_paths"):
        for value in payload.get(key) or []:
            path = str(value)
            if path and path not in paths:
                paths.append(path)
    return paths


def run_answer_pack_attempt(root: Path, query: str, *, top_k: int) -> AnswerPackAttempt:
    started = time.perf_counter()
    try:
        payload = discover(root, query, top_k=top_k)
        if payload.get("handoff_action") == "direct_use":
            predicted = _ranked_answer_pack_paths(payload)
        else:
            predicted = []
        return AnswerPackAttempt(
            payload=payload,
            candidates=[{"path": path} for path in predicted],
            predicted=predicted,
            stdout=json.dumps(payload, ensure_ascii=False, indent=2),
            stderr="",
            returncode=0,
            seconds=round(time.perf_counter() - started, 3),
        )
    except Exception as exc:
        return AnswerPackAttempt(
            payload={},
            candidates=[],
            predicted=[],
            stdout="",
            stderr=str(exc),
            returncode=-1,
            seconds=round(time.perf_counter() - started, 3),
        )
