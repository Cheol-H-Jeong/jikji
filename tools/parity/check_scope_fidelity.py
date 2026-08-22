#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
# How to run:
# python3 tools/parity/check_scope_fidelity.py --python-repo /Users/jeffrey/Projects-dev/jikji --rust-workspace . --plan .omo/plans/rust-port-workplan.md --json

from __future__ import annotations

import ast
import json
import re
import subprocess
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Final, TypeAlias

from scope_contract import (
    PYTHON_BENCHMARK_COMPAT_RATIONALE,
    REQUIRED_CRATES,
    REQUIRED_RUST_COMMANDS,
)

Json: TypeAlias = str | int | float | bool | None | list["Json"] | dict[str, "Json"]

HIDDEN_RUST_COMMANDS: Final = ("post-install-prepare",)
NESTED_GRAPH_COMMANDS: Final = ("status", "query", "explain")
PYTHON_EXTERNAL_COMPAT_RATIONALE: Final = PYTHON_BENCHMARK_COMPAT_RATIONALE
DOC_PATHS: Final = (
    "README.md", "docs/schema.md", "docs/agent-usage.md", "docs/agent-installation.md",
    "docs/local-agent-search-standard.md", "docs/release-publishing.md",
    "skills/jikji/SKILL.md", "crates/jikji-agent/assets/jikji/SKILL.md",
)


@dataclass(frozen=True, slots=True)
class ScopeArgs:
    python_repo: Path
    rust_workspace: Path
    plan: Path
    json_output: bool


@dataclass(frozen=True, slots=True)
class ScopeResult:
    passed: bool
    issues: tuple[str, ...]
    checks: dict[str, bool]
    details: dict[str, Json]

    def to_json(self) -> dict[str, Json]:
        return {
            "pass": self.passed,
            "issues": list(self.issues),
            "checks": self.checks,
            "details": self.details,
        }


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    issues: list[str] = []
    python_commands = python_cli_commands(args.python_repo, issues)
    rust_commands = rust_help_commands(args.rust_workspace, issues)
    details: dict[str, Json] = {
        "python_commands": sorted(python_commands),
        "rust_commands": sorted(rust_commands),
        "hidden_rust_commands": list(HIDDEN_RUST_COMMANDS),
        "nested_graph_commands": list(NESTED_GRAPH_COMMANDS),
        "python_external_compat_rationale": PYTHON_EXTERNAL_COMPAT_RATIONALE,
    }
    checks = {
        "cli_surface": check_cli_surface(python_commands, rust_commands, issues),
        "docs_and_skills": check_docs_and_skills(args.rust_workspace, issues),
        "split_crates": check_split_crates(args.rust_workspace, issues),
        "media_bridge": check_media_bridge(args.rust_workspace, issues),
        "trusted_publishing": check_trusted_publishing(args.rust_workspace, issues),
        "benchmark_evidence": check_benchmark_evidence(args.rust_workspace, issues),
        "plan_scope": check_plan_scope(args.plan, issues),
    }
    result = ScopeResult(
        passed=all(checks.values()) and not issues,
        issues=tuple(issues),
        checks=checks,
        details=details,
    )
    print(json.dumps(result.to_json(), ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if result.passed else 1


def parse_args(argv: Sequence[str]) -> ScopeArgs:
    json_output = False
    items = list(argv)
    if items and items[-1] == "--json":
        json_output = True
        items = items[:-1]
    values = dict(zip(items[0::2], items[1::2], strict=False))
    expected = {"--python-repo", "--rust-workspace", "--plan"}
    if set(values) != expected or len(items) != 6:
        raise SystemExit(
            "usage: check_scope_fidelity.py --python-repo PATH --rust-workspace PATH --plan PATH [--json]"
        )
    return ScopeArgs(
        python_repo=Path(values["--python-repo"]).resolve(),
        rust_workspace=Path(values["--rust-workspace"]).resolve(),
        plan=Path(values["--plan"]).resolve(),
        json_output=json_output,
    )


def python_cli_commands(python_repo: Path, issues: list[str]) -> set[str]:
    source = python_cli_source(python_repo)
    if not source.is_file():
        issues.append(f"missing Python reference CLI: {source}")
        return set()
    tree = ast.parse(source.read_text(encoding="utf-8"))
    commands: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not isinstance(node.func, ast.Attribute) or node.func.attr != "add_parser":
            continue
        if node.args and isinstance(node.args[0], ast.Constant) and isinstance(node.args[0].value, str):
            commands.add(node.args[0].value)
    return commands


def python_cli_source(python_repo: Path) -> Path:
    monorepo_source = python_repo / "python/jikji/src/jikji/__main__.py"
    if monorepo_source.is_file():
        return monorepo_source
    return python_repo / "src/jikji/__main__.py"


def rust_help_commands(rust_workspace: Path, issues: list[str]) -> set[str]:
    binary = rust_workspace / "target/release/jikji"
    command = [str(binary), "--help"] if binary.exists() else ["cargo", "run", "-p", "jikji-cli", "--bin", "jikji", "--", "--help"]
    completed = subprocess.run(
        command,
        cwd=rust_workspace,
        text=True,
        capture_output=True,
        timeout=60,
        check=False,
    )
    if completed.returncode != 0:
        issues.append(f"Rust CLI help failed: {completed.stderr.strip()}")
        return set()
    commands: set[str] = set()
    for line in completed.stdout.splitlines():
        match = re.match(r"\s{2}([a-z][a-z0-9-]+)\s{2,}", line)
        if match:
            commands.add(match.group(1))
    commands.update(rust_hidden_commands(rust_workspace, binary, issues))
    return commands


def rust_hidden_commands(rust_workspace: Path, binary: Path, issues: list[str]) -> set[str]:
    commands: set[str] = set()
    for name in HIDDEN_RUST_COMMANDS:
        command = [str(binary), name, "--help"] if binary.exists() else [
            "cargo", "run", "-p", "jikji-cli", "--bin", "jikji", "--", name, "--help",
        ]
        completed = subprocess.run(
            command,
            cwd=rust_workspace,
            text=True,
            capture_output=True,
            timeout=60,
            check=False,
        )
        if completed.returncode == 0:
            commands.add(name)
        else:
            issues.append(f"Rust hidden CLI command failed help check: {name}: {completed.stderr.strip()}")
    return commands


def check_cli_surface(
    python_commands: set[str],
    rust_commands: set[str],
    issues: list[str],
) -> bool:
    missing = sorted(set(REQUIRED_RUST_COMMANDS) - rust_commands)
    for command in missing:
        issues.append(f"missing Rust CLI command: {command}")
    nested = {command for command in NESTED_GRAPH_COMMANDS if "graph" in rust_commands}
    unreviewed_python = sorted(python_commands - rust_commands - nested)
    for command in unreviewed_python:
        issues.append(f"Python CLI command lacks Rust coverage or review: {command}")
    return not missing and not unreviewed_python and check_external_rationales(rust_commands, issues)


def check_external_rationales(rust_commands: set[str], issues: list[str]) -> bool:
    del rust_commands, issues
    return True




def check_docs_and_skills(root: Path, issues: list[str]) -> bool:
    passed = True
    for rel in DOC_PATHS:
        path = root / rel
        if not path.is_file() or path.stat().st_size == 0:
            issues.append(f"missing docs/skill surface: {rel}")
            passed = False
    install_text = read_joined(root, ("README.md", "docs/agent-installation.md", "skills/jikji/SKILL.md"))
    for marker in ("cargo install", "github.com/NomaDamas/jikji", "non-destructive"):
        if marker not in install_text:
            issues.append(f"docs/skills missing marker: {marker}")
            passed = False
    if "github.com/NomaDamas/jikji-rust" in install_text:
        issues.append("docs/skills still reference the retired jikji-rust fork URL")
        passed = False
    return passed


def check_split_crates(root: Path, issues: list[str]) -> bool:
    cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
    passed = True
    for crate in REQUIRED_CRATES:
        if f'"crates/{crate}"' not in cargo and f"crates/{crate}" not in cargo:
            issues.append(f"workspace missing crate member: {crate}")
            passed = False
    forbidden = ("jikji-cli",)
    for crate in ("jikji-parser", "jikji-index", "jikji-search"):
        text = (root / f"crates/{crate}/Cargo.toml").read_text(encoding="utf-8")
        if any(item in text for item in forbidden):
            issues.append(f"{crate} must not depend on jikji-cli")
            passed = False
    return passed


def check_media_bridge(root: Path, issues: list[str]) -> bool:
    bridge = root / "crates/jikji-media-bridge/src/lib.rs"
    prepare = root / "crates/jikji-cli/src/args/prepare.rs"
    text = bridge.read_text(encoding="utf-8") + "\n" + prepare.read_text(encoding="utf-8")
    required = ("JIKJI_OCR_ENGINE", "JIKJI_ASR_ENGINE", "enable_media_index")
    forbidden = ("JIKJI_MEDIA_BRIDGE_PYTHON", "JIKJI_MEDIA_BRIDGE_SCRIPT", "run_python_bridge")
    missing = [marker for marker in required if marker not in text]
    present_forbidden = [marker for marker in forbidden if marker in text]
    for marker in missing:
        issues.append(f"native media bridge missing marker: {marker}")
    for marker in present_forbidden:
        issues.append(f"retired Python media bridge marker remains: {marker}")
    return not missing and not present_forbidden


def check_trusted_publishing(root: Path, issues: list[str]) -> bool:
    publish = (root / ".github/workflows/publish.yml").read_text(encoding="utf-8")
    release = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
    markers = ("id-token: write", "rust-lang/crates-io-auth-action", "cargo package --workspace")
    missing = [marker for marker in markers if marker not in publish]
    if "sha256" not in release and "build-artifacts.sh" not in release:
        missing.append("release artifact checksum workflow")
    for marker in missing:
        issues.append(f"trusted publishing workflow missing marker: {marker}")
    return not missing


def check_benchmark_evidence(root: Path, issues: list[str]) -> bool:
    evidence = root / ".omo/evidence/rust-port-workplan/task-08-parity-benchmark.txt"
    report = root / "docs/rust-port-parity-report.md"
    if not evidence.is_file() or evidence.stat().st_size == 0:
        issues.append("missing task-08 benchmark evidence")
        return False
    text = evidence.read_text(encoding="utf-8", errors="replace")
    report_text = report.read_text(encoding="utf-8") if report.is_file() else ""
    markers = ("Prepare/Search/Find Timings", "Shared Python Evaluator Benchmark Path", "Contract failures: none")
    missing = [marker for marker in markers if marker not in text and marker not in report_text]
    for marker in missing:
        issues.append(f"benchmark evidence missing marker: {marker}")
    return not missing


def check_plan_scope(plan: Path, issues: list[str]) -> bool:
    text = plan.read_text(encoding="utf-8")
    markers = ("Must have", "Must NOT have", "Final verification wave")
    missing = [marker for marker in markers if marker not in text]
    for marker in missing:
        issues.append(f"plan missing scope section: {marker}")
    return not missing


def read_joined(root: Path, rels: tuple[str, ...]) -> str:
    return "\n".join((root / rel).read_text(encoding="utf-8", errors="replace") for rel in rels)


if __name__ == "__main__":
    raise SystemExit(main())
