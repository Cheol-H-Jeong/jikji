#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Final

from golden_fixtures import (
    CliStep,
    GoldenScenario,
    apply_mutation,
)
from parity_artifacts import (
    Json,
    _artifact_diff_summary,
    _contract_failures,
)
from parity_capture import _capture_artifacts
from parity_golden import _assert_python_matches_checked_in
from parity_json import _normalize_json, _normalize_text
from parity_scenarios import _build_scenario, _copy_tree, _generated_temp_scenario, _scenarios

TIMEOUT_S: Final = 30

@dataclass(frozen=True, slots=True)
class ParityArgs:
    python_repo: Path
    rust_bin: Path
    fixtures: Path
    out: Path


@dataclass(frozen=True, slots=True)
class Runtime:
    label: str
    executable: tuple[str, ...]
    cwd: Path
    root: Path
    env: dict[str, str]


@dataclass(frozen=True, slots=True)
class CommandRun:
    name: str
    args: tuple[str, ...]
    exit_code: int
    stdout: str
    stderr: str
    stdout_json: Json
    seconds: float
    retry_proof: str


@dataclass(frozen=True, slots=True)
class CommandPair:
    scenario: str
    python: CommandRun
    rust: CommandRun


@dataclass(frozen=True, slots=True)
class ScenarioResult:
    name: str
    commands: tuple[CommandPair, ...]
    python_artifacts: tuple[dict[str, Json], ...]
    rust_artifacts: tuple[dict[str, Json], ...]
    artifact_summary: dict[str, Json]


def main(argv: list[str] | None = None) -> int:
    from parity_report import _benchmark_prepare_search_find, _render_report, _run_bench_smoke

    args = _parse_args(sys.argv[1:] if argv is None else argv)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="jikji-final-parity-") as temp:
        temp_root = Path(temp)
        checked = _run_checked_in_golden(args, temp_root / "checked-in")
        generated = _run_generated_corpus(args, temp_root / "generated")
        bench = _benchmark_prepare_search_find(args, temp_root / "bench")
        smoke = _run_bench_smoke(args.rust_bin, temp_root / "bench-smoke")
    failures = _contract_failures(checked) + _contract_failures(generated)
    report = _render_report(args, checked, generated, bench, smoke, failures)
    args.out.write_text(report, encoding="utf-8")
    print(f"wrote parity benchmark evidence: {args.out}")
    return 1 if failures or not smoke["ok"] else 0


def _parse_args(argv: list[str]) -> ParityArgs:
    values = dict(zip(argv[0::2], argv[1::2], strict=False))
    expected = {"--python-repo", "--rust-bin", "--fixtures", "--out"}
    if set(values) != expected or len(argv) != 8:
        raise SystemExit(
            "usage: run_rust_vs_python.py --python-repo PATH --rust-bin PATH --fixtures PATH --out PATH"
        )
    python_repo = Path(values["--python-repo"]).expanduser().resolve()
    rust_bin = Path(values["--rust-bin"]).expanduser().resolve()
    fixtures = Path(values["--fixtures"]).expanduser().resolve()
    if not (_python_source_root(python_repo) / "jikji" / "__main__.py").exists():
        raise SystemExit(f"not a Jikji Python repo: {python_repo}")
    if not rust_bin.exists():
        raise SystemExit(f"missing Rust binary: {rust_bin}")
    if not (fixtures / "python" / "manifest.json").exists():
        raise SystemExit(f"missing checked-in Python golden fixtures: {fixtures}")
    return ParityArgs(
        python_repo=python_repo,
        rust_bin=rust_bin,
        fixtures=fixtures,
        out=Path(values["--out"]).resolve(),
    )


def _run_checked_in_golden(args: ParityArgs, root: Path) -> tuple[ScenarioResult, ...]:
    results = _run_scenarios(args, root, _scenarios())
    _assert_python_matches_checked_in(args.fixtures / "python", results)
    return results


def _run_generated_corpus(args: ParityArgs, root: Path) -> tuple[ScenarioResult, ...]:
    return _run_scenarios(args, root, (_generated_temp_scenario(),))


def _run_scenarios(
    args: ParityArgs,
    root: Path,
    scenarios: tuple[GoldenScenario, ...],
) -> tuple[ScenarioResult, ...]:
    results: list[ScenarioResult] = []
    for template in scenarios:
        python_root = root / "python" / template.name
        rust_root = root / "rust" / template.name
        scenario = _build_scenario(template.name, python_root)
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
        command_pairs = _run_command_pairs(scenario, python_runtime, rust_runtime)
        python_artifacts = tuple(_capture_artifacts(python_root))
        rust_artifacts = tuple(_capture_artifacts(rust_root))
        artifact_summary = _artifact_diff_summary(
            python_artifacts,
            rust_artifacts,
            python_root,
            rust_root,
        )
        results.append(
            ScenarioResult(
                name=template.name,
                commands=tuple(command_pairs),
                python_artifacts=python_artifacts,
                rust_artifacts=rust_artifacts,
                artifact_summary=artifact_summary,
            )
        )
    return tuple(results)


def _python_source_root(python_repo: Path) -> Path:
    monorepo_source = python_repo / "python" / "jikji" / "src"
    if (monorepo_source / "jikji" / "__main__.py").exists():
        return monorepo_source
    return python_repo / "src"


def _run_command_pairs(
    scenario: GoldenScenario,
    python_runtime: Runtime,
    rust_runtime: Runtime,
) -> list[CommandPair]:
    command_pairs: list[CommandPair] = []
    python_retry_proof = ""
    rust_retry_proof = ""
    for step in scenario.steps:
        if step.args[0] == "__mutate__":
            apply_mutation(python_runtime.root, step.args[1])
            apply_mutation(rust_runtime.root, step.args[1])
            continue
        python_run = _run_cli(python_runtime, step, python_retry_proof)
        rust_run = _run_cli(rust_runtime, step, rust_retry_proof)
        python_retry_proof = python_run.retry_proof or python_retry_proof
        rust_retry_proof = rust_run.retry_proof or rust_retry_proof
        command_pairs.append(CommandPair(scenario.name, python_run, rust_run))
    return command_pairs


def _run_cli(runtime: Runtime, step: CliStep, retry_proof: str) -> CommandRun:
    command = tuple(
        str(runtime.root) if item == "{root}" else retry_proof if item == "{retry_proof}" else item
        for item in step.args
    )
    started = time.perf_counter()
    completed = subprocess.run(
        (*runtime.executable, *command),
        cwd=runtime.cwd,
        env=runtime.env,
        text=True,
        capture_output=True,
        timeout=TIMEOUT_S,
        check=False,
    )
    seconds = time.perf_counter() - started
    actual_retry_proof = _extract_retry_proof(completed.stdout)
    proof_for_output = actual_retry_proof or retry_proof
    stdout = _normalize_text(completed.stdout, runtime.root, proof_for_output)
    stderr = _normalize_text(completed.stderr, runtime.root, proof_for_output)
    normalized_args = tuple(_normalize_text(item, runtime.root, proof_for_output) for item in command)
    return CommandRun(
        name=step.name,
        args=normalized_args,
        exit_code=completed.returncode,
        stdout=stdout,
        stderr=stderr,
        stdout_json=_parse_stdout_json(stdout),
        seconds=round(seconds, 6),
        retry_proof=actual_retry_proof,
    )
def _extract_retry_proof(stdout: str) -> str:
    stripped = stdout.strip()
    if not stripped.startswith("{"):
        return ""
    parsed = json.loads(stripped)
    if not isinstance(parsed, dict):
        return ""
    proof = parsed.get("retry_proof")
    return proof if isinstance(proof, str) else ""


def _parse_stdout_json(stdout: str) -> Json:
    stripped = stdout.strip()
    if not stripped.startswith(("{", "[")):
        return None
    return _normalize_json(json.loads(stripped))
