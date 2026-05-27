"""HippoCamp adapter and raw-vs-Jikji benchmark helpers."""
from __future__ import annotations

import json
import os
import time
import urllib.parse
import urllib.request
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .agent_index import AGENT_DIR_NAME, build_agent_index
from .config import Config
from .eval import (
    _path_fingerprints,
    _rank_for_expected,
    _read_jsonl,
    _read_text_file,
    _score,
    _write_json,
    _write_jsonl,
    build_search_index,
    search,
    search_with_index,
)
from .search_index import instant_index_path

HF_REPO = "MMMem-org/HippoCamp"
HF_API_TREE = f"https://huggingface.co/api/datasets/{HF_REPO}/tree/main?recursive=true"
HF_RESOLVE = f"https://huggingface.co/datasets/{HF_REPO}/resolve/main/"
DEFAULT_TEXT_EXTENSIONS = {
    ".txt",
    ".md",
    ".markdown",
    ".csv",
    ".tsv",
    ".json",
    ".jsonl",
    ".xml",
    ".yaml",
    ".yml",
    ".ini",
    ".cfg",
    ".toml",
    ".html",
    ".htm",
    ".eml",
    ".ics",
}
BENCHMARK_LEAK_PATTERNS = (
    "*_Subset.json",
    "*.annotation.json",
    "hippocamp_eval_set*.jsonl",
    "eval_set*.jsonl",
    "*_gold.json",
    "*.qa.json",
)


@dataclass
class HippoCampFetchResult:
    root: Path
    annotation_path: Path
    files_downloaded: int
    bytes_downloaded: int
    skipped: int


@dataclass
class HippoCampImportResult:
    eval_set_path: Path
    cases: int
    scenarios: dict[str, int]
    skipped_cases: int = 0


@dataclass
class BenchResult:
    report_path: Path
    metrics: dict[str, Any]


@dataclass
class HippoCampSuiteResult:
    report_path: Path
    profiles: dict[str, Any]
    aggregate: dict[str, Any]


def _url(path: str) -> str:
    return HF_RESOLVE + urllib.parse.quote(path)


def _load_hf_tree() -> list[dict]:
    with urllib.request.urlopen(HF_API_TREE, timeout=30) as resp:
        data = json.load(resp)
    return data if isinstance(data, list) else []


def _download(url: str, dest: Path, *, max_bytes: int | None = None) -> int:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=120) as resp:
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
        data = b"".join(chunks)
    dest.write_bytes(data)
    return len(data)


def _safe_child(root: Path, rel: str) -> Path:
    if Path(rel).is_absolute() or ".." in Path(rel).parts:
        raise ValueError(f"unsafe remote path segment: {rel!r}")
    target = root / rel
    try:
        target.resolve().relative_to(root.resolve())
    except ValueError as exc:
        raise ValueError(f"download target escapes root: {rel!r}") from exc
    return target


def _looks_like_leak(path: Path) -> bool:
    import fnmatch

    name = path.name
    return any(fnmatch.fnmatch(name, pattern) for pattern in BENCHMARK_LEAK_PATTERNS)


def assert_benchmark_no_leak(root: Path, eval_set: Path | None = None, *, allow_leak: bool = False) -> None:
    """Reject deterministic benchmark inputs that expose answer files as corpus files."""
    if allow_leak:
        return
    root = Path(root).expanduser().resolve()
    problems: list[str] = []
    if eval_set is not None:
        eval_set = Path(eval_set).expanduser().resolve()
        try:
            eval_set.relative_to(root)
            problems.append(f"eval set is inside benchmark root: {eval_set}")
        except ValueError:
            pass
    for candidate in root.rglob("*"):
        if candidate.is_file() and _looks_like_leak(candidate):
            problems.append(f"possible benchmark answer/annotation leak inside root: {candidate}")
            if len(problems) >= 20:
                break
    if problems:
        raise RuntimeError("Benchmark no-leak check failed:\n- " + "\n- ".join(problems))


def fetch_subset(
    dest: Path,
    *,
    profile: str = "Adam",
    split: str = "Subset",
    max_files: int = 120,
    max_file_bytes: int = 10 * 1024 * 1024,
    max_total_bytes: int = 250 * 1024 * 1024,
) -> HippoCampFetchResult:
    """Download a bounded HippoCamp subset from Hugging Face.

    Large video/audio artifacts are skipped by default through max_file_bytes.
    The resulting root is the inner profile folder, e.g. ``dest/Adam_Subset``.
    """
    dest = Path(dest).expanduser().resolve()
    inner = f"{profile}_{split}" if split.lower() == "subset" else profile
    prefix = f"{profile}/{split}/{inner}/"
    annotation_remote = f"{profile}/{split}/{inner}.json"
    root = dest / inner
    root.mkdir(parents=True, exist_ok=True)

    total = 0
    count = 0
    skipped = 0
    annotation_path = dest / f"{inner}.annotation.json"
    total += _download(_url(annotation_remote), annotation_path, max_bytes=max_file_bytes if max_file_bytes > 0 else None)

    tree = _load_hf_tree()
    for item in tree:
        if item.get("type") != "file":
            continue
        remote = str(item.get("path") or "")
        if not remote.startswith(prefix):
            continue
        size = int(item.get("size") or 0)
        rel = remote[len(prefix):]
        if not rel or rel.endswith(".json"):
            continue
        if count >= max_files or (max_file_bytes > 0 and size > max_file_bytes):
            skipped += 1
            continue
        if max_total_bytes > 0 and total + size > max_total_bytes:
            skipped += 1
            continue
        target = _safe_child(root, rel)
        if target.exists() and target.stat().st_size == size:
            count += 1
            total += size
            continue
        got = _download(_url(remote), target, max_bytes=max_file_bytes if max_file_bytes > 0 else None)
        total += got
        count += 1
    return HippoCampFetchResult(
        root=root,
        annotation_path=annotation_path,
        files_downloaded=count,
        bytes_downloaded=total,
        skipped=skipped,
    )


def import_eval_set(
    root: Path,
    *,
    annotation: Path | None = None,
    max_cases: int = 200,
    out: Path | None = None,
) -> HippoCampImportResult:
    """Convert HippoCamp QA annotations into Jikji eval_set JSONL."""
    root = Path(root).expanduser().resolve()
    if annotation is None:
        matches = sorted(root.glob("*_Subset.json")) + sorted(root.glob("*.json"))
        if not matches:
            raise FileNotFoundError("No HippoCamp annotation JSON found; pass --annotation")
        annotation = matches[0]
    data = json.loads(Path(annotation).read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("HippoCamp annotation JSON must be a list")

    cases: list[dict] = []
    counts: Counter[str] = Counter()
    skipped_cases = 0
    for row in data:
        if len(cases) >= max_cases or not isinstance(row, dict):
            break
        query = str(row.get("question") or "").strip()
        paths = [str(p) for p in (row.get("file_path") or []) if isinstance(p, str)]
        existing = [p for p in paths if (root / p).exists()]
        if not query or not existing:
            skipped_cases += 1
            continue
        scenario = "hippocamp_" + str(row.get("QA_type") or row.get("profiling_type") or "qa").strip().lower().replace(" ", "_")
        evidence = ""
        ev = row.get("evidence") or []
        if isinstance(ev, list) and ev:
            first = ev[0]
            if isinstance(first, dict):
                evidence = str(first.get("evidence_text") or "")
        if not evidence:
            evidence = str(row.get("evidence_text_joined") or row.get("gold_text") or row.get("answer") or "")
        counts[scenario] += 1
        cases.append({
            "id": f"{scenario}-{counts[scenario]:04d}",
            "scenario": scenario,
            "query": query,
            "expected_paths": existing,
            "evidence": evidence[:1000],
            "answer": str(row.get("answer") or "")[:2000],
            "source": "HippoCamp",
        })

    out = Path(out).expanduser().resolve() if out else root.parent / f"{root.name}_hippocamp_eval_set.jsonl"
    _write_jsonl(out, cases)
    _write_json(out.with_name("hippocamp_import_report.json"), {
        "root": str(root),
        "annotation": str(annotation),
        "cases": len(cases),
        "skipped_cases": skipped_cases,
        "scenarios": dict(counts),
    })
    return HippoCampImportResult(out, len(cases), dict(counts), skipped_cases)


def _raw_candidates(root: Path) -> list[dict]:
    rows: list[dict] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d != AGENT_DIR_NAME and not (Path(dirpath) / d).is_symlink()]
        for name in filenames:
            path = Path(dirpath) / name
            if path.is_symlink():
                continue
            if _looks_like_leak(path):
                continue
            try:
                rel = path.relative_to(root).as_posix()
                st = path.stat()
            except OSError:
                continue
            ext = path.suffix.lower()
            text = _read_text_file(path) if ext in DEFAULT_TEXT_EXTENSIONS else ""
            rows.append({
                "path": rel,
                "name": name,
                "ext": ext,
                "size": st.st_size,
                "keywords": [],
                "summary": "",
                "_source_text": text,
            })
    return rows


def _raw_search(root: Path, query: str, *, top_k: int) -> list[dict]:
    return _raw_search_rows(_raw_candidates(root), query, top_k=top_k)


def _raw_search_rows(rows: list[dict], query: str, *, top_k: int) -> list[dict]:
    ranked = []
    for row in rows:
        score, reasons = _score(query, row)
        if score <= 0:
            continue
        ranked.append({
            "path": row.get("path"),
            "name": row.get("name"),
            "score": round(score, 3),
            "reasons": reasons,
        })
    ranked.sort(key=lambda item: (-float(item["score"]), str(item.get("path") or "")))
    return ranked[:top_k]


def _metrics(cases: list[dict], details: list[dict], *, top_k: int) -> dict[str, Any]:
    hits_at = Counter()
    hash_hits_at = Counter()
    duplicate_hits_at = Counter()
    rr = 0.0
    recall_sums = Counter()
    precision_sums = Counter()
    by_scenario: dict[str, list[dict]] = defaultdict(list)
    for detail in details:
        rank = detail["rank"]
        if rank:
            rr += 1.0 / rank
            for k in (1, 3, 5, 10):
                if rank <= k:
                    hits_at[k] += 1
        hash_rank = detail.get("hash_rank")
        duplicate_rank = detail.get("duplicate_rank")
        for k in (1, 3, 5, 10):
            if hash_rank is not None and hash_rank <= k:
                hash_hits_at[k] += 1
            if duplicate_rank is not None and duplicate_rank <= k:
                duplicate_hits_at[k] += 1
        expected = set(str(p) for p in (detail.get("expected_paths") or []))
        ranked_paths = [str(item.get("path") or "") for item in (detail.get("top_results") or [])]
        if expected:
            for k in (1, 3, 5, 10):
                top = ranked_paths[:k]
                overlap = len(expected.intersection(top))
                recall_sums[k] += overlap / min(k, len(expected))
                precision_sums[k] += overlap / k
        by_scenario[str(detail.get("scenario") or "unknown")].append(detail)
    total = len(cases)

    def ratio(n: int) -> float:
        return round(n / total, 4) if total else 0.0

    scenario_metrics = {}
    for scenario, items in sorted(by_scenario.items()):
        n = len(items)

        def scenario_recall(k: int, *, scenario_items=items, scenario_n=n) -> float:
            total_recall = 0.0
            for item in scenario_items:
                expected = set(str(p) for p in (item.get("expected_paths") or []))
                if not expected:
                    continue
                ranked_paths = [str(r.get("path") or "") for r in (item.get("top_results") or [])[:k]]
                total_recall += len(expected.intersection(ranked_paths)) / min(k, len(expected))
            return round(total_recall / scenario_n, 4)

        scenario_metrics[scenario] = {
            "cases": n,
            "hit_at_1": round(sum(1 for item in items if item["rank"] == 1) / n, 4),
            "hit_at_5": round(sum(1 for item in items if item["rank"] and item["rank"] <= 5) / n, 4),
            "hit_at_10": round(sum(1 for item in items if item["rank"] and item["rank"] <= 10) / n, 4),
            "set_recall_at_5": scenario_recall(5),
            "set_recall_at_10": scenario_recall(10),
            "hash_or_exact_hit_at_10": round(sum(1 for item in items if item.get("hash_rank") is not None and item["hash_rank"] <= 10) / n, 4),
            "duplicate_or_exact_hit_at_10": round(sum(1 for item in items if item.get("duplicate_rank") is not None and item["duplicate_rank"] <= 10) / n, 4),
            "mrr": round(sum((1.0 / item["rank"]) for item in items if item["rank"]) / n, 4),
        }
    return {
        "cases": total,
        "top_k": top_k,
        "hit_at_1": ratio(hits_at[1]),
        "hit_at_3": ratio(hits_at[3]),
        "hit_at_5": ratio(hits_at[5]),
        "hit_at_10": ratio(hits_at[10]),
        "set_recall_at_5": round(recall_sums[5] / total, 4) if total else 0.0,
        "set_recall_at_10": round(recall_sums[10] / total, 4) if total else 0.0,
        "set_precision_at_5": round(precision_sums[5] / total, 4) if total else 0.0,
        "set_precision_at_10": round(precision_sums[10] / total, 4) if total else 0.0,
        "hash_or_exact_hit_at_5": ratio(hash_hits_at[5]),
        "hash_or_exact_hit_at_10": ratio(hash_hits_at[10]),
        "duplicate_or_exact_hit_at_5": ratio(duplicate_hits_at[5]),
        "duplicate_or_exact_hit_at_10": ratio(duplicate_hits_at[10]),
        "mrr": round(rr / total, 4) if total else 0.0,
        "by_scenario": scenario_metrics,
    }


def run_benchmark(
    root: Path,
    *,
    eval_set: Path | None = None,
    modes: tuple[str, ...] = ("raw", "jikji"),
    top_k: int = 5,
    prepare: bool = False,
    allow_leak: bool = False,
) -> BenchResult:
    """Compare raw filesystem lookup with Jikji-assisted lookup."""
    root = Path(root).expanduser().resolve()
    eval_path = eval_set or (root / AGENT_DIR_NAME / "eval" / "hippocamp_eval_set.jsonl")
    assert_benchmark_no_leak(root, eval_path, allow_leak=allow_leak)
    if prepare or not (root / AGENT_DIR_NAME / "file_index.jsonl").exists():
        cfg = Config(include_hidden=True)
        cfg.ignore_patterns.extend(BENCHMARK_LEAK_PATTERNS)
        build_agent_index(root, cfg)
    cases = _read_jsonl(eval_path)
    if not cases:
        raise FileNotFoundError(f"No benchmark eval set found: {eval_path}")
    fingerprints = _path_fingerprints(root)

    report: dict[str, Any] = {
        "root": str(root),
        "eval_set": str(eval_path),
        "modes": {},
    }
    for mode in modes:
        details = []
        started = time.perf_counter()
        use_instant_jikji = mode == "jikji" and instant_index_path(root).exists()
        jikji_index = None if use_instant_jikji else (build_search_index(root) if mode == "jikji" else None)
        raw_rows = _raw_candidates(root) if mode == "raw" else None
        for case in cases:
            if mode == "raw":
                ranked = _raw_search_rows(raw_rows or [], str(case.get("query") or ""), top_k=top_k)
            elif mode == "jikji":
                if use_instant_jikji:
                    ranked = search(root, str(case.get("query") or ""), top_k=top_k)
                else:
                    ranked = search_with_index(jikji_index, str(case.get("query") or ""), top_k=top_k)  # type: ignore[arg-type]
            else:
                raise ValueError(f"unsupported benchmark mode: {mode}")
            expected = set(str(p) for p in (case.get("expected_paths") or []))
            rank = _rank_for_expected(ranked, expected, fingerprints, mode="exact")
            hash_rank = _rank_for_expected(ranked, expected, fingerprints, mode="hash")
            duplicate_rank = _rank_for_expected(ranked, expected, fingerprints, mode="duplicate")
            details.append({
                "id": case.get("id"),
                "scenario": case.get("scenario"),
                "query": case.get("query"),
                "expected_paths": sorted(expected),
                "rank": rank,
                "hash_rank": hash_rank,
                "duplicate_rank": duplicate_rank,
                "top_results": ranked,
            })
        metrics = _metrics(cases, details, top_k=top_k)
        metrics["seconds"] = round(time.perf_counter() - started, 3)
        report["modes"][mode] = {"metrics": metrics, "details": details}

    out = eval_path.parent / f"{root.name}_hippocamp_benchmark_report.json"
    _write_json(out, report)
    return BenchResult(out, {mode: data["metrics"] for mode, data in report["modes"].items()})


def run_suite(
    dest: Path,
    *,
    profiles: tuple[str, ...] = ("Adam", "Bei", "Victoria"),
    split: str = "Subset",
    max_files: int = 120,
    max_file_bytes: int = 10 * 1024 * 1024,
    max_total_bytes: int = 250 * 1024 * 1024,
    cases: int = 200,
    top_k: int = 5,
    fetch: bool = True,
) -> HippoCampSuiteResult:
    """Fetch/import/prepare/benchmark multiple HippoCamp profiles and aggregate metrics."""
    dest = Path(dest).expanduser().resolve()
    dest.mkdir(parents=True, exist_ok=True)
    profile_reports: dict[str, Any] = {}
    aggregate_rows: dict[str, list[dict[str, Any]]] = defaultdict(list)

    for profile in profiles:
        if fetch:
            fetched = fetch_subset(
                dest,
                profile=profile,
                split=split,
                max_files=max_files,
                max_file_bytes=max_file_bytes,
                max_total_bytes=max_total_bytes,
            )
            root = fetched.root
            annotation = fetched.annotation_path
            fetch_payload: dict[str, Any] = {
                "root": str(root),
                "annotation": str(annotation),
                "files_downloaded": fetched.files_downloaded,
                "bytes_downloaded": fetched.bytes_downloaded,
                "skipped": fetched.skipped,
            }
        else:
            inner = f"{profile}_{split}" if split.lower() == "subset" else profile
            root = dest / inner
            annotation = root / f"{inner}.json"
            fetch_payload = {"root": str(root), "annotation": str(annotation), "fetch": "skipped"}

        build_agent_index(root, Config(include_hidden=True))
        imported = import_eval_set(root, annotation=annotation, max_cases=cases)
        bench = run_benchmark(root, eval_set=imported.eval_set_path, modes=("raw", "jikji"), top_k=top_k)
        profile_reports[profile] = {
            "fetch": fetch_payload,
            "eval_set": str(imported.eval_set_path),
            "cases": imported.cases,
            "scenarios": imported.scenarios,
            "benchmark_report": str(bench.report_path),
            "metrics": bench.metrics,
        }
        for mode, metrics in bench.metrics.items():
            aggregate_rows[mode].append(metrics)

    aggregate: dict[str, Any] = {}
    for mode, rows in aggregate_rows.items():
        total_cases = sum(int(row.get("cases") or 0) for row in rows)

        def weighted(key: str, metric_rows=rows, case_count=total_cases) -> float:
            if not case_count:
                return 0.0
            return round(
                sum(float(row.get(key) or 0.0) * int(row.get("cases") or 0) for row in metric_rows) / case_count,
                4,
            )

        aggregate[mode] = {
            "profiles": len(rows),
            "cases": total_cases,
            "hit_at_1": weighted("hit_at_1"),
            "hit_at_3": weighted("hit_at_3"),
            "hit_at_5": weighted("hit_at_5"),
            "mrr": weighted("mrr"),
            "seconds": round(sum(float(row.get("seconds") or 0.0) for row in rows), 3),
        }

    out = dest / "hippocamp_suite_report.json"
    _write_json(out, {"dest": str(dest), "profiles": profile_reports, "aggregate": aggregate})
    return HippoCampSuiteResult(out, profile_reports, aggregate)
