"""EDiTh / Véracier Industries public enterprise-document benchmark adapter.

EDiTh ships a realistic enterprise PDF corpus plus an answer key.  Jikji's
evaluation target is file discovery, so this adapter converts each question's
ground-truth document list into a path-level eval set and can stream-extract a
bounded subset of PDFs from the public Hugging Face archive.
"""
from __future__ import annotations

import csv
import json
import posixpath
import re
import tarfile
import time
import urllib.parse
import urllib.request
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .agent_index import build_agent_index
from .config import Config
from .hippocamp import BenchResult, run_benchmark

EDITH_REPO = "lightonai/veracier-industries"
EDITH_RESOLVE = f"https://huggingface.co/datasets/{EDITH_REPO}/resolve/main/"
EDITH_METADATA_FILES = ("README.md", "MASTER_INDEX.csv", "ANSWER_KEY.json")
EDITH_ARCHIVE = "by_entity.tar.gz"
DEFAULT_EDITH_MAX_DOWNLOAD_BYTES = 2_000_000_000


class EdithDownloadLimitExceeded(RuntimeError):
    """Raised when the bounded EDiTh archive stream exceeds its byte budget."""


class _CountingReader:
    """File-like wrapper that enforces a compressed-byte transfer budget."""

    def __init__(self, raw: Any, *, limit: int) -> None:
        if limit <= 0:
            raise ValueError("max_download_bytes must be positive when downloading EDiTh documents")
        self.raw = raw
        self.limit = limit
        self.bytes_read = 0

    def read(self, size: int = -1) -> bytes:
        data = self.raw.read(size)
        self.bytes_read += len(data)
        if self.bytes_read > self.limit:
            raise EdithDownloadLimitExceeded(
                "EDiTh archive transfer exceeded "
                f"{self.limit} bytes before all selected documents were found "
                f"(read={self.bytes_read}). Re-run with --max-download-bytes or --no-docs."
            )
        return data

    def readable(self) -> bool:
        return True

    def __getattr__(self, name: str) -> Any:
        return getattr(self.raw, name)


@dataclass
class EdithExtractResult:
    found: dict[str, str]
    bytes_read: int
    byte_limit: int
    truncated: bool


@dataclass
class EdithMaterializeResult:
    metadata_dir: Path
    corpus_root: Path
    eval_set_path: Path
    selected_questions: int
    selected_docs: int
    extracted_docs: int
    skipped_questions: int
    archive_bytes_read: int
    archive_byte_limit: int
    archive_truncated: bool


@dataclass
class EdithSuiteResult:
    report_path: Path
    materialized: EdithMaterializeResult
    metrics: dict[str, Any]
    prepare_seconds: float


def _url(path: str) -> str:
    return EDITH_RESOLVE + urllib.parse.quote(path)


def _download(url: str, dest: Path) -> int:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=180) as resp:  # noqa: S310 - public benchmark URL.
        data = resp.read()
    dest.write_bytes(data)
    return len(data)


def fetch_edith_metadata(dest: Path) -> Path:
    metadata_dir = Path(dest).expanduser().resolve() / "metadata"
    metadata_dir.mkdir(parents=True, exist_ok=True)
    for name in EDITH_METADATA_FILES:
        path = metadata_dir / name
        if not path.exists():
            _download(_url(name), path)
    return metadata_dir


def _read_master_index(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        rows = [dict(row) for row in csv.DictReader(handle)]
    return rows


def _norm_rel(value: str) -> str:
    value = (value or "").replace("\\", "/").strip().strip("/")
    value = re.sub(r"/+", "/", value)
    return value


def _safe_extract_path(root: Path, rel: str) -> Path:
    if Path(rel).is_absolute() or ".." in Path(rel).parts:
        raise ValueError(f"unsafe archive path: {rel!r}")
    target = root / rel
    try:
        target.resolve().relative_to(root.resolve())
    except ValueError as exc:
        raise ValueError(f"archive path escapes corpus root: {rel!r}") from exc
    return target


def _flatten_ground_truth(value: Any) -> list[str]:
    out: list[str] = []

    def visit(item: Any) -> None:
        if isinstance(item, str):
            if item.lower().endswith(".pdf"):
                out.append(_norm_rel(item))
        elif isinstance(item, (list, tuple, set)):
            for child in item:
                visit(child)
        elif isinstance(item, dict):
            for child in item.values():
                visit(child)

    visit(value)
    return sorted(dict.fromkeys(out))


def _master_lookup(rows: list[dict[str, str]]) -> dict[str, dict[str, str]]:
    lookup: dict[str, dict[str, str]] = {}
    for row in rows:
        filename = _norm_rel(str(row.get("filename") or ""))
        if not filename:
            continue
        lookup[filename] = row
        lookup[posixpath.basename(filename)] = row
    return lookup


def _question_doc_paths(answer: dict[str, Any], master: list[dict[str, str]]) -> list[str]:
    lookup = _master_lookup(master)
    paths: list[str] = []
    for raw in _flatten_ground_truth(answer.get("ground_truth") or {}):
        if raw in lookup:
            paths.append(_norm_rel(str(lookup[raw].get("filename") or raw)))
        elif posixpath.basename(raw) in lookup:
            paths.append(_norm_rel(str(lookup[posixpath.basename(raw)].get("filename") or raw)))
        else:
            paths.append(raw)
    return sorted(dict.fromkeys(paths))


def _select_eval_cases(
    answers: dict[str, Any],
    master: list[dict[str, str]],
    *,
    max_cases: int,
    max_docs: int,
) -> tuple[list[dict[str, Any]], set[str], int]:
    cases: list[dict[str, Any]] = []
    selected_docs: set[str] = set()
    skipped = 0
    for qid, answer in answers.items():
        if len(cases) >= max_cases:
            break
        if not isinstance(answer, dict):
            skipped += 1
            continue
        docs = _question_doc_paths(answer, master)
        if not docs:
            skipped += 1
            continue
        remaining = max(0, max_docs - len(selected_docs))
        if remaining <= 0:
            break
        bounded_docs = docs[:remaining]
        selected_docs.update(bounded_docs)
        cases.append({
            "id": f"edith-{qid}",
            "scenario": "edith_enterprise_pdf",
            "query": str(answer.get("question") or qid),
            "expected_source_paths": bounded_docs,
            "all_expected_source_paths": docs,
            "dropped_expected_source_paths": docs[remaining:],
            "role": answer.get("role", ""),
            "entity": answer.get("entity", ""),
            "difficulty_factors": answer.get("difficulty_factors", []),
            "source": "EDiTh / Véracier Industries",
            "dataset": EDITH_REPO,
            "question_id": qid,
            "public_benchmark": True,
        })
    return cases, selected_docs, skipped


def _suffix_candidates(member_name: str) -> list[str]:
    rel = _norm_rel(member_name)
    if rel.startswith("by_entity/"):
        rel = rel[len("by_entity/"):]
    parts = rel.split("/")
    return ["/".join(parts[idx:]) for idx in range(len(parts))]


def _stream_extract_selected_docs(
    corpus_root: Path,
    wanted: set[str],
    *,
    max_download_bytes: int = DEFAULT_EDITH_MAX_DOWNLOAD_BYTES,
) -> EdithExtractResult:
    """Stream the public tar.gz and extract only selected PDFs.

    The archive is large, so this avoids storing ``by_entity.tar.gz`` locally.
    It still has to read sequentially until all selected files are encountered.
    """
    corpus_root.mkdir(parents=True, exist_ok=True)
    wanted_norm = {_norm_rel(path) for path in wanted}
    wanted_basenames = {posixpath.basename(path): path for path in wanted_norm}
    found: dict[str, str] = {}
    counting: _CountingReader | None = None
    req = urllib.request.Request(_url(EDITH_ARCHIVE), headers={"User-Agent": "jikji-edith-benchmark"})
    with urllib.request.urlopen(req, timeout=240) as resp:  # noqa: S310 - public benchmark URL.
        counting = _CountingReader(resp, limit=max_download_bytes)
        with tarfile.open(fileobj=counting, mode="r|gz") as archive:
            for member in archive:
                if len(found) >= len(wanted_norm):
                    break
                if not member.isfile():
                    continue
                member_base = posixpath.basename(member.name)
                if member_base.startswith("._") or not member_base.lower().endswith(".pdf"):
                    continue
                matched = ""
                for suffix in _suffix_candidates(member.name):
                    if suffix in wanted_norm:
                        matched = suffix
                        break
                if not matched and member_base in wanted_basenames:
                    matched = wanted_basenames[member_base]
                if not matched:
                    continue
                rel = _norm_rel(member.name)
                if rel.startswith("by_entity/"):
                    rel = rel[len("by_entity/"):]
                target = _safe_extract_path(corpus_root, rel)
                source = archive.extractfile(member)
                if source is None:
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with source, target.open("wb") as out:
                    while True:
                        chunk = source.read(1024 * 1024)
                        if not chunk:
                            break
                        out.write(chunk)
                found[matched] = rel
    return EdithExtractResult(
        found=found,
        bytes_read=counting.bytes_read if counting is not None else 0,
        byte_limit=max_download_bytes,
        truncated=len(found) < len(wanted_norm),
    )


def _existing_selected_docs(corpus_root: Path, wanted: set[str]) -> dict[str, str]:
    wanted_norm = {_norm_rel(path) for path in wanted}
    wanted_basenames = {posixpath.basename(path): path for path in wanted_norm}
    found: dict[str, str] = {}
    if not corpus_root.exists():
        return found
    for path in corpus_root.rglob("*.pdf"):
        if not path.is_file():
            continue
        rel = path.relative_to(corpus_root).as_posix()
        suffixes = _suffix_candidates(rel)
        matched = next((suffix for suffix in suffixes if suffix in wanted_norm), "")
        if not matched and path.name in wanted_basenames:
            matched = wanted_basenames[path.name]
        if matched:
            found[matched] = rel
    return found


def _write_json(path: Path, obj: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows),
        encoding="utf-8",
    )


def materialize_edith_dataset(
    dest: Path,
    *,
    max_cases: int = 8,
    max_docs: int = 60,
    download_docs: bool = True,
    max_download_bytes: int = DEFAULT_EDITH_MAX_DOWNLOAD_BYTES,
) -> EdithMaterializeResult:
    dest = Path(dest).expanduser().resolve()
    metadata_dir = fetch_edith_metadata(dest)
    corpus_root = dest / "corpus"
    eval_set_path = dest / "eval" / "edith_eval.jsonl"
    master = _read_master_index(metadata_dir / "MASTER_INDEX.csv")
    answers = json.loads((metadata_dir / "ANSWER_KEY.json").read_text(encoding="utf-8"))
    if not isinstance(answers, dict):
        raise ValueError("EDiTh ANSWER_KEY.json must be an object")
    cases, selected_docs, skipped = _select_eval_cases(
        answers,
        master,
        max_cases=max_cases,
        max_docs=max_docs,
    )
    extracted: dict[str, str] = {}
    archive_bytes_read = 0
    archive_byte_limit = max_download_bytes
    archive_truncated = False
    if download_docs and selected_docs:
        extracted = _existing_selected_docs(corpus_root, selected_docs)
        missing = selected_docs - set(extracted)
        if missing:
            streamed = _stream_extract_selected_docs(
                corpus_root,
                missing,
                max_download_bytes=max_download_bytes,
            )
            extracted.update(streamed.found)
            archive_bytes_read = streamed.bytes_read
            archive_byte_limit = streamed.byte_limit
            archive_truncated = streamed.truncated

    final_cases: list[dict[str, Any]] = []
    for case in cases:
        expected = [extracted[path] for path in case["expected_source_paths"] if path in extracted]
        if not expected and download_docs:
            skipped += 1
            continue
        row = dict(case)
        row["expected_paths"] = sorted(dict.fromkeys(expected or list(case["expected_source_paths"])))
        row["expected_count"] = len(row["expected_paths"])
        dropped = set(row.get("dropped_expected_source_paths") or [])
        if download_docs:
            dropped.update(path for path in case["expected_source_paths"] if path not in extracted)
        row["dropped_expected_source_paths"] = sorted(dropped)
        final_cases.append(row)
    _write_jsonl(eval_set_path, final_cases)
    _write_json(dest / "edith_manifest.json", {
        "public_benchmark": True,
        "source": EDITH_REPO,
        "metadata_dir": str(metadata_dir),
        "corpus_root": str(corpus_root),
        "eval_set": str(eval_set_path),
        "selected_questions": len(final_cases),
        "candidate_questions": len(cases),
        "selected_docs": len(selected_docs),
        "extracted_docs": len(extracted),
        "skipped_questions": skipped,
        "download_docs": download_docs,
        "archive_bytes_read": archive_bytes_read,
        "archive_byte_limit": archive_byte_limit,
        "archive_truncated": archive_truncated,
        "archive": _url(EDITH_ARCHIVE),
    })
    return EdithMaterializeResult(
        metadata_dir=metadata_dir,
        corpus_root=corpus_root,
        eval_set_path=eval_set_path,
        selected_questions=len(final_cases),
        selected_docs=len(selected_docs),
        extracted_docs=len(extracted),
        skipped_questions=skipped,
        archive_bytes_read=archive_bytes_read,
        archive_byte_limit=archive_byte_limit,
        archive_truncated=archive_truncated,
    )


def run_edith_suite(
    dest: Path,
    *,
    max_cases: int = 8,
    max_docs: int = 60,
    top_k: int = 10,
    download_docs: bool = True,
    prepare: bool = True,
    max_download_bytes: int = DEFAULT_EDITH_MAX_DOWNLOAD_BYTES,
) -> EdithSuiteResult:
    dest = Path(dest).expanduser().resolve()
    started = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    materialized = materialize_edith_dataset(
        dest,
        max_cases=max_cases,
        max_docs=max_docs,
        download_docs=download_docs,
        max_download_bytes=max_download_bytes,
    )
    prepare_seconds = 0.0
    if prepare and download_docs and materialized.extracted_docs:
        t0 = time.perf_counter()
        cfg = Config(include_hidden=True)
        cfg.max_files = 1_000_000
        cfg.ignore_patterns.extend(("*.json", "*.jsonl", "*.csv", "*.xlsx"))
        build_agent_index(materialized.corpus_root, cfg)
        prepare_seconds = time.perf_counter() - t0
    bench: BenchResult | None = None
    if download_docs and materialized.extracted_docs:
        bench = run_benchmark(
            materialized.corpus_root,
            eval_set=materialized.eval_set_path,
            modes=("raw", "jikji"),
            top_k=top_k,
            prepare=False,
            allow_leak=False,
        )
    report_path = dest / "reports" / f"edith_suite_{started}.json"
    skipped_benchmark_key = "metadata_only" if not download_docs else "no_document_cases"
    metrics = bench.metrics if bench is not None else {
        skipped_benchmark_key: {
            "cases": materialized.selected_questions,
            "selected_docs": materialized.selected_docs,
            "download_docs": download_docs,
            "note": (
                "Document download disabled; no raw-vs-Jikji benchmark was run."
                if not download_docs
                else "No EDiTh documents were extracted; no raw-vs-Jikji benchmark was run."
            ),
        },
    }
    _write_json(report_path, {
        "public_benchmark": True,
        "source": EDITH_REPO,
        "materialized": {
            "metadata_dir": str(materialized.metadata_dir),
            "corpus_root": str(materialized.corpus_root),
            "eval_set_path": str(materialized.eval_set_path),
            "selected_questions": materialized.selected_questions,
            "selected_docs": materialized.selected_docs,
            "extracted_docs": materialized.extracted_docs,
            "skipped_questions": materialized.skipped_questions,
            "archive_bytes_read": materialized.archive_bytes_read,
            "archive_byte_limit": materialized.archive_byte_limit,
            "archive_truncated": materialized.archive_truncated,
        },
        "prepare_seconds": round(prepare_seconds, 3),
        "benchmark_report": str(bench.report_path) if bench is not None else None,
        "metrics": metrics,
    })
    return EdithSuiteResult(
        report_path=report_path,
        materialized=materialized,
        metrics=metrics,
        prepare_seconds=round(prepare_seconds, 3),
    )


def edith_answer_summary(dest: Path) -> dict[str, Any]:
    metadata_dir = fetch_edith_metadata(dest)
    master = _read_master_index(metadata_dir / "MASTER_INDEX.csv")
    answers = json.loads((metadata_dir / "ANSWER_KEY.json").read_text(encoding="utf-8"))
    cases, selected_docs, skipped = _select_eval_cases(answers, master, max_cases=10_000, max_docs=10_000)
    formats = Counter(row.get("format", "") for row in master)
    languages = Counter(row.get("language", "") for row in master)
    return {
        "source": EDITH_REPO,
        "metadata_dir": str(metadata_dir),
        "master_rows": len(master),
        "answer_questions": len(answers),
        "file_retrieval_questions": len(cases),
        "referenced_docs": len(selected_docs),
        "skipped_questions": skipped,
        "formats": dict(formats.most_common()),
        "languages": dict(languages.most_common()),
    }
