#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
# How to run:
# python3 tools/parity/run_rust_vs_python.py --python-repo /Users/jeffrey/Projects-dev/jikji --rust-bin target/release/jikji --fixtures tests/golden --out .omo/evidence/rust-port-workplan/task-08-parity-benchmark.txt

from __future__ import annotations

from parity_artifacts import Json, _artifact_diff_summary, _command_failures
from parity_commands import CommandPair, CommandRun, main

__all__ = [
    "CommandPair",
    "CommandRun",
    "Json",
    "_artifact_diff_summary",
    "_command_failures",
    "main",
]

if __name__ == "__main__":
    raise SystemExit(main())
