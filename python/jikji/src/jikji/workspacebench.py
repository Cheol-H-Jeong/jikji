"""Workspace-Bench-Lite adapter for Jikji file-discovery evaluation.

Workspace-Bench evaluates complete workspace task solving.  Jikji does not solve
those tasks directly; it helps local agents discover the task-supporting files.
This adapter therefore converts each Workspace-Bench-Lite task into a no-leak
file-discovery case: given the task instruction, find the source files listed in
metadata/data_manifest and file_dep_graph.
"""
from __future__ import annotations

import json
import re
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .agent_index import build_agent_index
from .config import Config
from .eval import _write_json, _write_jsonl
from .hippocamp import BenchResult, run_benchmark

HF_REPO = "Workspace-Bench/Workspace-Bench-Lite"
HF_API = f"https://huggingface.co/api/datasets/{HF_REPO}"
HF_RESOLVE = f"https://huggingface.co/datasets/{HF_REPO}/resolve/main/"
TASK_PREFIX = "task_lite_clean_en"
DEFAULT_MAX_FILE_BYTES = 25 * 1024 * 1024
DEFAULT_MAX_TOTAL_BYTES = 500 * 1024 * 1024


@dataclass
class WorkspaceBenchBuildResult:
    dest: Path
    corpus_root: Path
    eval_set_path: Path
    manifest_path: Path
    tasks: int
    files_downloaded: int
    bytes_downloaded: int
    eval_cases: int


@dataclass
class WorkspaceBenchSuiteResult:
    build: WorkspaceBenchBuildResult
    deterministic_report: Path
    deterministic_metrics: dict[str, Any]
    prepare_seconds: float
    report_path: Path


def _hf_url(path: str) -> str:
    return HF_RESOLVE + urllib.parse.quote(path)


def _load_dataset_info() -> dict[str, Any]:
    with urllib.request.urlopen(HF_API, timeout=30) as resp:  # noqa: S310 - public benchmark endpoint.
        data = json.load(resp)
    return data if isinstance(data, dict) else {}


def _task_ids_from_info(info: dict[str, Any]) -> list[int]:
    ids: set[int] = set()
    for sibling in info.get("siblings") or []:
        name = str(sibling.get("rfilename") or "")
        match = re.fullmatch(rf"{TASK_PREFIX}/(\d+)/metadata\.json", name)
        if match:
            ids.add(int(match.group(1)))
    return sorted(ids)


def _download(url: str, dest: Path, *, max_bytes: int | None = None) -> int:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=120) as resp:  # noqa: S310 - public benchmark endpoint.
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = resp.read(1024 * 1024)
            if not chunk:
                break
            total += len(chunk)
            if max_bytes is not None and total > max_bytes:
                raise ValueError(f"download exceeds max_bytes={max_bytes}: {url}")
            chunks.append(chunk)
    dest.write_bytes(b"".join(chunks))
    return total


def _read_json_url(path: str) -> dict[str, Any]:
    with urllib.request.urlopen(_hf_url(path), timeout=60) as resp:  # noqa: S310 - public benchmark endpoint.
        data = json.load(resp)
    return data if isinstance(data, dict) else {}


def _safe_relpath(rel: str) -> Path:
    path = Path(rel)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"unsafe Workspace-Bench path: {rel!r}")
    return path


def _required_manifest_rows(metadata: dict[str, Any]) -> list[dict[str, str]]:
    """Return rows for source files needed by the task.

    Prefer file_dep_graph ``from`` nodes when they are available.  Fall back to
    all data_manifest rows because Lite tasks commonly store only task-supporting
    files in the workspace data directory.
    """
    rows = [row for row in metadata.get("data_manifest") or [] if isinstance(row, dict)]
    by_name = {str(row.get("filename") or ""): row for row in rows}
    required_names = {
        str(edge.get("from") or "")
        for edge in (metadata.get("file_dep_graph") or [])
        if isinstance(edge, dict) and edge.get("from")
    }
    if required_names:
        selected = [by_name[name] for name in sorted(required_names) if name in by_name]
        if selected:
            return selected
    return rows


def build_eval_case(task_dir: str, metadata: dict[str, Any]) -> dict[str, Any]:
    rows = _required_manifest_rows(metadata)
    expected = [f"{task_dir}/{_safe_relpath(str(row['stored_relpath'])).as_posix()}" for row in rows if row.get("stored_relpath")]
    task_id = str(metadata.get("absolute_id") or task_dir.rsplit("_", 1)[-1])
    persona = str(metadata.get("persona") or "workspace agent")
    task = str(metadata.get("task") or "")
    return {
        "id": f"workspacebench-{task_id}",
        "scenario": "workspace_task_supporting_files",
        "query": (
            f"You are a {persona}. For this Workspace-Bench task, find the source/input files under this "
            f"workspace that are needed before producing the requested output. Task: {task}"
        ),
        "expected_paths": expected,
        "dataset": "Workspace-Bench-Lite",
        "workspace_task_id": int(metadata.get("absolute_id") or task_id),
        "persona": persona,
        "task_diff": metadata.get("task_diff"),
        "tested_capabilities": metadata.get("tested_capabilities") or [],
        "output_files": metadata.get("output_files") or [],
        "expected_count": len(expected),
    }


def build_workspacebench_benchmark(
    dest: Path,
    *,
    max_tasks: int = 12,
    start: int = 0,
    max_file_bytes: int = DEFAULT_MAX_FILE_BYTES,
    max_total_bytes: int = DEFAULT_MAX_TOTAL_BYTES,
) -> WorkspaceBenchBuildResult:
    dest = Path(dest).expanduser().resolve()
    corpus_root = dest / "corpus"
    metadata_dir = dest / "metadata"
    eval_dir = dest / "eval"
    corpus_root.mkdir(parents=True, exist_ok=True)
    metadata_dir.mkdir(parents=True, exist_ok=True)
    eval_dir.mkdir(parents=True, exist_ok=True)

    info = _load_dataset_info()
    task_ids = _task_ids_from_info(info)
    selected_ids = task_ids[start:start + max_tasks]
    if not selected_ids:
        raise RuntimeError("No Workspace-Bench-Lite task metadata found from Hugging Face")

    cases: list[dict[str, Any]] = []
    manifest_tasks: list[dict[str, Any]] = []
    files_downloaded = 0
    bytes_downloaded = 0
    skipped_files: list[dict[str, str]] = []

    for task_id in selected_ids:
        remote_base = f"{TASK_PREFIX}/{task_id}"
        metadata = _read_json_url(f"{remote_base}/metadata.json")
        task_dir = f"task_{task_id}"
        (corpus_root / task_dir).mkdir(parents=True, exist_ok=True)
        _write_json(metadata_dir / f"task_{task_id}_metadata.json", metadata)

        for row in metadata.get("data_manifest") or []:
            if not isinstance(row, dict) or not row.get("stored_relpath"):
                continue
            rel = _safe_relpath(str(row["stored_relpath"]))
            target = corpus_root / task_dir / rel
            remote = f"{remote_base}/{rel.as_posix()}"
            if target.exists() and target.stat().st_size > 0:
                size = target.stat().st_size
            else:
                try:
                    size = _download(_hf_url(remote), target, max_bytes=max_file_bytes)
                except Exception as exc:  # keep bounded public benchmark crawl progressing per file.
                    skipped_files.append({"task_id": str(task_id), "path": rel.as_posix(), "reason": type(exc).__name__})
                    continue
            files_downloaded += 1
            bytes_downloaded += size
            if bytes_downloaded > max_total_bytes:
                raise ValueError(f"Workspace-Bench download exceeds max_total_bytes={max_total_bytes}")

        case = build_eval_case(task_dir, metadata)
        if case["expected_paths"]:
            cases.append(case)
        manifest_tasks.append({
            "task_id": task_id,
            "persona": metadata.get("persona"),
            "task_diff": metadata.get("task_diff"),
            "files": len(metadata.get("data_manifest") or []),
            "expected_files": len(case.get("expected_paths") or []),
        })

    eval_set = eval_dir / "workspacebench_lite_eval.jsonl"
    _write_jsonl(eval_set, cases)
    manifest = dest / "manifest.json"
    _write_json(manifest, {
        "source": "Workspace-Bench-Lite",
        "source_url": "https://huggingface.co/datasets/Workspace-Bench/Workspace-Bench-Lite",
        "task_prefix": TASK_PREFIX,
        "selected_task_ids": selected_ids,
        "tasks": len(selected_ids),
        "files_downloaded": files_downloaded,
        "bytes_downloaded": bytes_downloaded,
        "eval_set": str(eval_set),
        "eval_cases": len(cases),
        "tasks_summary": manifest_tasks,
        "skipped_files": skipped_files[:100],
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "honesty_note": (
            "This is a Jikji file-discovery adaptation of Workspace-Bench-Lite. "
            "It measures whether agents can identify task-supporting source files, "
            "not whether they complete Workspace-Bench output-generation rubrics."
        ),
    })
    return WorkspaceBenchBuildResult(
        dest=dest,
        corpus_root=corpus_root,
        eval_set_path=eval_set,
        manifest_path=manifest,
        tasks=len(selected_ids),
        files_downloaded=files_downloaded,
        bytes_downloaded=bytes_downloaded,
        eval_cases=len(cases),
    )


def run_workspacebench_suite(
    dest: Path,
    *,
    max_tasks: int = 12,
    start: int = 0,
    top_k: int = 10,
    max_file_bytes: int = DEFAULT_MAX_FILE_BYTES,
    max_total_bytes: int = DEFAULT_MAX_TOTAL_BYTES,
) -> WorkspaceBenchSuiteResult:
    build = build_workspacebench_benchmark(
        dest,
        max_tasks=max_tasks,
        start=start,
        max_file_bytes=max_file_bytes,
        max_total_bytes=max_total_bytes,
    )
    cfg = Config(include_hidden=False)
    cfg.max_files = 1_000_000
    t0 = time.perf_counter()
    build_agent_index(build.corpus_root, cfg)
    prepare_seconds = time.perf_counter() - t0
    bench: BenchResult = run_benchmark(
        build.corpus_root,
        eval_set=build.eval_set_path,
        modes=("raw", "jikji"),
        top_k=top_k,
        prepare=False,
        allow_leak=False,
    )
    report_path = build.dest / "reports" / "workspacebench_lite_suite_report.json"
    _write_json(report_path, {
        "build": {
            "dest": str(build.dest),
            "corpus_root": str(build.corpus_root),
            "eval_set": str(build.eval_set_path),
            "manifest": str(build.manifest_path),
            "tasks": build.tasks,
            "files_downloaded": build.files_downloaded,
            "bytes_downloaded": build.bytes_downloaded,
            "eval_cases": build.eval_cases,
        },
        "prepare_seconds": round(prepare_seconds, 3),
        "deterministic_report": str(bench.report_path),
        "deterministic_metrics": bench.metrics,
    })
    return WorkspaceBenchSuiteResult(
        build=build,
        deterministic_report=bench.report_path,
        deterministic_metrics=bench.metrics,
        prepare_seconds=round(prepare_seconds, 3),
        report_path=report_path,
    )
