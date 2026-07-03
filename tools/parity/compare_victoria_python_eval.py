#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
# How to run:
# python3 tools/parity/compare_victoria_python_eval.py \
#   --python-repo /Users/jeffrey/Projects-dev/jikji \
#   --rust-bin target/release/jikji \
#   --dataset /Users/jeffrey/Projects/FileOrgBench/data/hippocamp/Victoria/Subset/Victoria_Subset \
#   --annotation /Users/jeffrey/Projects/FileOrgBench/data/hippocamp/Victoria/Subset/Victoria_Subset.json \
#   --out docs/victoria-python-rust-eval-report.json
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Final, TypedDict

TIMEOUT_S: Final = 120
Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]


class CandidateRow(TypedDict, total=False):
    path: str
    name: str
    score: float
    reasons: list[str]


@dataclass(frozen=True, slots=True)
class CompareArgs:
    python_repo: Path
    rust_bin: Path
    dataset: Path
    annotation: Path
    out: Path
    top_k: int


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    sys.path.insert(0, str(_python_source_root(args.python_repo)))
    from jikji import hippocamp  # noqa: PLC0415

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="jikji-victoria-shared-eval-") as temp:
        temp_root = Path(temp)
        shared_eval_set = temp_root / "eval" / "victoria_eval.jsonl"
        python_eval_set = temp_root / "python-eval" / "victoria_eval.jsonl"
        rust_eval_set = temp_root / "rust-eval" / "victoria_eval.jsonl"
        python_root = temp_root / "python" / "Victoria_Subset"
        rust_root = temp_root / "rust" / "Victoria_Subset"
        _copy_dataset(args.dataset, python_root)
        _copy_dataset(args.dataset, rust_root)
        _run_python_prepare(args, python_root)
        _run_rust_prepare(args, rust_root)
        imported = hippocamp.import_eval_set(
            python_root,
            annotation=args.annotation,
            max_cases=200,
            out=shared_eval_set,
        )
        python_eval_set.parent.mkdir(parents=True, exist_ok=True)
        rust_eval_set.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(imported.eval_set_path, python_eval_set)
        shutil.copy2(imported.eval_set_path, rust_eval_set)
        python_result = hippocamp.run_benchmark(
            python_root,
            eval_set=python_eval_set,
            modes=("jikji",),
            top_k=args.top_k,
            prepare=False,
        )
        original_search = hippocamp.search
        hippocamp.search = _rust_search(args.rust_bin)
        try:
            rust_result = hippocamp.run_benchmark(
                rust_root,
                eval_set=rust_eval_set,
                modes=("jikji",),
                top_k=args.top_k,
                prepare=False,
            )
        finally:
            hippocamp.search = original_search
        python_report_path = args.out.with_name(f"{args.out.stem}.python-report.json")
        rust_report_path = args.out.with_name(f"{args.out.stem}.rust-report.json")
        shutil.copy2(python_result.report_path, python_report_path)
        shutil.copy2(rust_result.report_path, rust_report_path)
        payload = {
            "dataset": str(args.dataset),
            "annotation": str(args.annotation),
            "eval_code": "python_jikji.hippocamp.run_benchmark",
            "rust_under_test": str(args.rust_bin),
            "cases": imported.cases,
            "top_k": args.top_k,
            "metrics": {
                "python_jikji": python_result.metrics["jikji"],
                "rust_jikji": rust_result.metrics["jikji"],
            },
            "reports": {
                "python": str(python_report_path),
                "rust": str(rust_report_path),
            },
        }
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(payload["metrics"], ensure_ascii=False, indent=2))
    return 0


def _parse_args(argv: list[str]) -> CompareArgs:
    values = dict(zip(argv[0::2], argv[1::2], strict=False))
    required = {"--python-repo", "--rust-bin", "--dataset", "--annotation", "--out"}
    if not required.issubset(values) or len(argv) not in {10, 12}:
        raise SystemExit(
            "usage: compare_victoria_python_eval.py --python-repo PATH --rust-bin PATH "
            "--dataset PATH --annotation PATH --out PATH [--top-k N]"
        )
    python_repo = Path(values["--python-repo"]).expanduser().resolve()
    rust_bin = Path(values["--rust-bin"]).expanduser().resolve()
    dataset = Path(values["--dataset"]).expanduser().resolve()
    annotation = Path(values["--annotation"]).expanduser().resolve()
    if not (_python_source_root(python_repo) / "jikji" / "hippocamp.py").exists():
        raise SystemExit(f"not a Python Jikji repo: {python_repo}")
    if not rust_bin.exists():
        raise SystemExit(f"missing Rust binary: {rust_bin}")
    if not dataset.is_dir():
        raise SystemExit(f"missing Victoria dataset directory: {dataset}")
    if not annotation.is_file():
        raise SystemExit(f"missing Victoria annotation: {annotation}")
    return CompareArgs(
        python_repo=python_repo,
        rust_bin=rust_bin,
        dataset=dataset,
        annotation=annotation,
        out=Path(values["--out"]).expanduser().resolve(),
        top_k=int(values.get("--top-k", "10")),
    )


def _copy_dataset(source: Path, target: Path) -> None:
    shutil.copytree(source, target)


def _run_python_prepare(args: CompareArgs, root: Path) -> None:
    env = {**os.environ, "PYTHONPATH": str(_python_source_root(args.python_repo))}
    subprocess.run(
        (sys.executable, "-m", "jikji.__main__", "prepare", str(root), "--json"),
        cwd=args.python_repo,
        env=env,
        text=True,
        capture_output=True,
        timeout=TIMEOUT_S,
        check=True,
    )


def _python_source_root(python_repo: Path) -> Path:
    monorepo_source = python_repo / "python" / "jikji" / "src"
    if (monorepo_source / "jikji" / "hippocamp.py").exists():
        return monorepo_source
    return python_repo / "src"


def _run_rust_prepare(args: CompareArgs, root: Path) -> None:
    subprocess.run(
        (str(args.rust_bin), "prepare", str(root), "--json"),
        text=True,
        capture_output=True,
        timeout=TIMEOUT_S,
        check=True,
    )


def _rust_search(rust_bin: Path):
    def search(root: Path, query: str, *, top_k: int = 10) -> list[CandidateRow]:
        completed = subprocess.run(
            (str(rust_bin), "search", str(root), query, "--top-k", str(top_k), "--json"),
            text=True,
            capture_output=True,
            timeout=TIMEOUT_S,
            check=True,
        )
        payload: Json = json.loads(completed.stdout)
        if not isinstance(payload, dict):
            return []
        candidates = payload.get("candidates")
        if not isinstance(candidates, list):
            return []
        return [
            _candidate(row)
            for row in candidates
            if isinstance(row, dict) and isinstance(row.get("path"), str)
        ]

    return search


def _candidate(row: dict[str, Json]) -> CandidateRow:
    reasons = row.get("reasons")
    return {
        "path": str(row["path"]),
        "name": str(row.get("name") or ""),
        "score": float(row.get("score") or 0.0),
        "reasons": [str(item) for item in reasons] if isinstance(reasons, list) else [],
    }


if __name__ == "__main__":
    raise SystemExit(main())
