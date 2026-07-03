from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from golden_fixtures import CliStep
from parity_artifacts import Json, _ranked_paths
from parity_commands import Runtime, _python_source_root, _run_cli
from parity_scenarios import _build_scenario, _copy_tree, _generated_temp_scenario

if TYPE_CHECKING:
    from parity_commands import CommandPair, ParityArgs, ScenarioResult

def _benchmark_prepare_search_find(args: ParityArgs, root: Path) -> dict[str, Json]:
    scenario = _generated_temp_scenario()
    python_root = root / "python" / scenario.name
    rust_root = root / "rust" / scenario.name
    _build_scenario(scenario.name, python_root)
    _copy_tree(python_root, rust_root)
    python_runtime = Runtime(
        label="python",
        executable=(sys.executable, "-m", "jikji.__main__"),
        cwd=args.python_repo,
        root=python_root,
        env={**os.environ, "PYTHONPATH": str(_python_source_root(args.python_repo))},
    )
    rust_runtime = Runtime(
        label="rust",
        executable=(str(args.rust_bin),),
        cwd=args.rust_bin.parent,
        root=rust_root,
        env=os.environ.copy(),
    )
    steps = (
        CliStep("prepare", ("prepare", "{root}", "--json")),
        CliStep("search", ("search", "{root}", "alpha needle", "--json")),
        CliStep("find", ("find", "{root}", "renewal clause", "--json")),
    )
    timings: dict[str, Json] = {}
    for step in steps:
        python_run = _run_cli(python_runtime, step, "")
        rust_run = _run_cli(rust_runtime, step, "")
        timings[step.name] = {
            "python_seconds": python_run.seconds,
            "rust_seconds": rust_run.seconds,
            "python_exit": python_run.exit_code,
            "rust_exit": rust_run.exit_code,
        }
    return timings


def _run_bench_smoke(rust_bin: Path, root: Path) -> dict[str, Json]:
    return {
        "ok": True,
        "rust_bin": str(rust_bin),
        "scratch_root": str(root),
        "status": "rust_bench_removed",
        "replacement": "tools/parity/compare_victoria_python_eval.py",
    }


def _render_report(
    args: ParityArgs,
    checked: tuple[ScenarioResult, ...],
    generated: tuple[ScenarioResult, ...],
    bench: dict[str, Json],
    smoke: dict[str, Json],
    failures: list[str],
) -> str:
    scenarios = (*checked, *generated)
    intentional_non_parity = _intentional_non_parity_lines(scenarios)
    lines = [
        "# Task 8 Final Parity Benchmark Evidence",
        "",
        f"python_repo: {args.python_repo}",
        f"rust_bin: {args.rust_bin}",
        f"fixtures: {args.fixtures}",
        f"result: {'FAIL' if failures or not smoke['ok'] else 'PASS'}",
        "",
        "## Feature / JSON / Ranking Parity",
    ]
    for scenario in scenarios:
        lines.append(f"- scenario={scenario.name}")
        for pair in scenario.commands:
            json_keys = _json_key_summary(pair)
            ranking = _ranking_summary(pair)
            lines.append(
                "  "
                f"{pair.python.name}: exit python={pair.python.exit_code} rust={pair.rust.exit_code}; "
                f"seconds python={pair.python.seconds:.6f} rust={pair.rust.seconds:.6f}; "
                f"json={json_keys}; ranking={ranking}"
            )
        lines.append(f"  artifacts={json.dumps(scenario.artifact_summary, ensure_ascii=False)}")
    lines.extend(
        [
            "",
            "## Prepare/Search/Find Timings",
            json.dumps(bench, ensure_ascii=False, indent=2),
            "",
            "## Shared Python Evaluator Benchmark Path",
            json.dumps(smoke, ensure_ascii=False, indent=2),
            "",
            "## Contract Failures",
        ]
    )
    if failures:
        lines.extend(f"- {failure}" for failure in failures)
    else:
        lines.append("- none")
    lines.extend(
        [
            "",
            "## Intentional Non-Parity",
            "- Generated Markdown prose and validated generated JSON/JSONL prose may differ by implementation after required artifact presence and schema fields pass.",
            "- Exact doc_text cache bytes may differ by parser implementation after required presence and non-empty cache checks pass.",
            "- Wiki source page hash suffixes are implementation-specific; parity compares semantic source stems and counts.",
            "- Timings are measured wall-clock seconds from this run only; no faster-performance claim is inferred.",
        ]
    )
    if intentional_non_parity:
        lines.extend(f"- {item}" for item in intentional_non_parity)
    else:
        lines.append("- no scenario-level intentional artifact non-parity observed")
    lines.append("")
    return "\n".join(lines)


def _intentional_non_parity_lines(scenarios: tuple[ScenarioResult, ...]) -> list[str]:
    lines: list[str] = []
    for scenario in scenarios:
        values = scenario.artifact_summary.get("intentional_non_parity")
        if not isinstance(values, list) or not values:
            continue
        rendered = [str(value) for value in values[:8]]
        if len(values) > 8:
            rendered.append(f"... {len(values) - 8} more")
        lines.append(f"{scenario.name}: {rendered}")
    return lines


def _json_key_summary(pair: CommandPair) -> str:
    if not isinstance(pair.python.stdout_json, dict) or not isinstance(pair.rust.stdout_json, dict):
        return "non-json"
    missing = sorted(set(pair.python.stdout_json) - set(pair.rust.stdout_json))
    extra = sorted(set(pair.rust.stdout_json) - set(pair.python.stdout_json))
    return f"missing={missing}, extra={extra}"


def _ranking_summary(pair: CommandPair) -> str:
    if not isinstance(pair.python.stdout_json, dict) or not isinstance(pair.rust.stdout_json, dict):
        return "non-json"
    return f"python={_ranked_paths(pair.python.stdout_json)}, rust={_ranked_paths(pair.rust.stdout_json)}"
