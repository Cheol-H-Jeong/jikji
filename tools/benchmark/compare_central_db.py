#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def run(command: list[str], env: dict[str, str]) -> None:
    completed = subprocess.run(command, env=env, capture_output=True)
    if completed.returncode:
        raise SystemExit(completed.stderr.decode(errors="replace"))


def make_corpus(root: Path, count: int) -> None:
    shutil.rmtree(root, ignore_errors=True)
    (root / "docs").mkdir(parents=True)
    (root / "src").mkdir()
    for index in range(1, count + 1):
        (root / "docs" / f"doc-{index}.txt").write_text(
            f"performance marker document {index:04} contract search text\n",
            encoding="utf-8",
        )
        (root / "src" / f"file-{index}.rs").write_text(
            f"fn perf_{index:04}() {{}}\n",
            encoding="utf-8",
        )


def prepare_samples(binary: Path, corpus: Path, runs: int, label: str) -> list[float]:
    samples = []
    for index in range(runs):
        data = Path(tempfile.gettempdir()) / f"jikji-bench-{label}-{index}"
        shutil.rmtree(data, ignore_errors=True)
        env = os.environ | {"JIKJI_DATA_DIR": str(data)}
        started = time.perf_counter()
        run([str(binary), "prepare", str(corpus), "--no-agent-rules", "--json"], env)
        samples.append(time.perf_counter() - started)
    return samples


def find_samples(binary: Path, corpus: Path, runs: int, label: str, extra: list[str]) -> list[float]:
    data = Path(tempfile.gettempdir()) / f"jikji-find-{label}"
    shutil.rmtree(data, ignore_errors=True)
    env = os.environ | {"JIKJI_DATA_DIR": str(data)}
    run([str(binary), "prepare", str(corpus), "--no-agent-rules", "--json"], env)
    samples = []
    for _ in range(runs):
        started = time.perf_counter()
        run([str(binary), "find", str(corpus), "performance marker 0110", "--json", *extra], env)
        samples.append(time.perf_counter() - started)
    return samples


def search_samples(binary: Path, corpus: Path, runs: int, label: str, extra: list[str]) -> list[float]:
    data = Path(tempfile.gettempdir()) / f"jikji-search-{label}"
    shutil.rmtree(data, ignore_errors=True)
    env = os.environ | {"JIKJI_DATA_DIR": str(data)}
    run([str(binary), "prepare", str(corpus), "--no-agent-rules", "--json"], env)
    samples = []
    for _ in range(runs):
        started = time.perf_counter()
        run([str(binary), "search", str(corpus), "performance marker 0110", "--json", *extra], env)
        samples.append(time.perf_counter() - started)
    return samples


def command_samples(binary: Path, corpus: Path, command: str, runs: int, extra: list[str]) -> list[float]:
    samples = []
    data = Path(tempfile.gettempdir()) / f"jikji-{command}-benchmark"
    shutil.rmtree(data, ignore_errors=True)
    env = os.environ | {"JIKJI_DATA_DIR": str(data)}
    if command == "search":
        run([str(binary), "prepare", str(corpus), "--no-agent-rules", "--json"], env)
    for _ in range(runs):
        if command == "deep-index":
            shutil.rmtree(data, ignore_errors=True)
        started = time.perf_counter()
        arguments = [str(binary), command, str(corpus)]
        if command == "search":
            arguments.append("performance marker 0110")
        arguments.extend(extra)
        run(arguments, env)
        samples.append(time.perf_counter() - started)
    return samples


def metric(baseline: list[float], candidate: list[float]) -> dict[str, object]:
    base = statistics.median(baseline)
    current = statistics.median(candidate)
    return {
        "baseline_samples_s": baseline,
        "candidate_samples_s": candidate,
        "baseline_median_s": base,
        "candidate_median_s": current,
        "delta_pct": (current / base - 1.0) * 100.0,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    work = Path(tempfile.gettempdir())
    small = work / "jikji-benchmark-small"
    large = work / "jikji-benchmark-large"
    make_corpus(small, 120)  # 240 files
    make_corpus(large, 1000)  # 2,000 files
    prepare = metric(
        prepare_samples(args.baseline, large, 5, "baseline"),
        prepare_samples(args.candidate, large, 5, "candidate"),
    )
    find = metric(
        find_samples(args.baseline, small, 15, "baseline", []),
        find_samples(args.candidate, small, 15, "candidate", ["--no-background-refresh"]),
    )
    search = metric(
        search_samples(args.baseline, small, 15, "baseline", []),
        search_samples(args.candidate, small, 15, "candidate", ["--no-background-refresh"]),
    )
    deep = command_samples(args.candidate, small, "deep-index", 5, ["--no-agent-rules", "--json"])
    ready = command_samples(args.candidate, small, "search", 15, ["--no-background-refresh", "--json"])
    stale = command_samples(args.candidate, small, "search", 15, ["--stale-after-seconds", "0", "--json"])
    payload = {
        "corpora": {"small_files": 240, "large_files": 2000},
        "prepare_large": prepare,
        "find_small": find,
        "search_small": search,
        "deep_index_small": {"samples_s": deep, "median_s": statistics.median(deep)},
        "refresh_small": {
            "ready_samples_s": ready,
            "stale_samples_s": stale,
            "ready_median_s": statistics.median(ready),
            "stale_median_s": statistics.median(stale),
            "added_ms": (statistics.median(stale) - statistics.median(ready)) * 1000.0,
        },
    }
    args.out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
