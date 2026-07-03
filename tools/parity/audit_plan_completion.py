#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
# How to run:
# python3 tools/parity/audit_plan_completion.py --plan .omo/plans/rust-port-workplan.md --evidence .omo/evidence/rust-port-workplan --json

from __future__ import annotations

import json
import re
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Final, TypeAlias

Json: TypeAlias = str | int | float | bool | None | list["Json"] | dict[str, "Json"]

REQUIRED_TASK_EVIDENCE: Final = (
    "task-01-golden-red.txt",
    "task-02-workspace-green.txt",
    "task-03-index-green.txt",
    "task-04-parser-green.txt",
    "task-05-search-green.txt",
    "task-06-agent-gui-bench-green.txt",
    "task-07-ci-release-green.txt",
    "task-08-parity-benchmark.txt",
)
REQUIRED_COMMAND_MARKERS: Final = (
    ("python3 tools/parity/capture_python_golden.py",),
    ("cargo metadata --format-version=1", '"workspace_members"'),
    ("cargo test --workspace --no-run", "Executable unittests", "Finished `test` profile"),
    ("prepare_clean_doctor_parity",),
    ("cargo test -p jikji-parser -p jikji-media-bridge --all-features",),
    ("search_find_parity",),
    ("curl -i",),
    ("cargo package --workspace --exclude jikji-parity --exclude jikji-bench",),
    ("bash scripts/release/build-artifacts.sh",),
    ("python3 tools/parity/run_rust_vs_python.py",),
    ("Prepare/Search/Find Timings",),
)
SCOPE_MARKERS: Final = (
    "non_destructive",
    "clean_preserves",
    "raw_fallback_allowed",
    "python_required_by_default",
    "trusted publishing",
    "contract_failures",
)
REQUIRED_REPO_PATHS: Final = (
    "AGENTS.md",
    "README.md",
    "docs/schema.md",
    "docs/agent-usage.md",
    "docs/local-agent-search-standard.md",
    "docs/release-publishing.md",
    "docs/rust-port-parity-report.md",
    "skills/jikji/SKILL.md",
    "Cargo.toml",
    "crates/jikji-core/Cargo.toml",
    "crates/jikji-parser/Cargo.toml",
    "crates/jikji-index/Cargo.toml",
    "crates/jikji-search/Cargo.toml",
    "crates/jikji-agent/Cargo.toml",
    "crates/jikji-media-bridge/Cargo.toml",
    "crates/jikji-cli/Cargo.toml",
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    ".github/workflows/publish.yml",
)


@dataclass(frozen=True, slots=True)
class AuditArgs:
    plan: Path
    evidence: Path
    json_output: bool


@dataclass(frozen=True, slots=True)
class AuditResult:
    passed: bool
    issues: tuple[str, ...]
    checks: dict[str, bool]

    def to_json(self) -> dict[str, Json]:
        return {
            "pass": self.passed,
            "issues": list(self.issues),
            "checks": self.checks,
        }


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    issues: list[str] = []
    checks = {
        "todos_checked": audit_todos(args.plan, issues),
        "evidence_files": audit_evidence_files(args.evidence, issues),
        "required_command_coverage": audit_command_coverage(args.evidence, issues),
        "scope_checks": audit_scope_checks(args.evidence, issues),
        "repo_artifacts": audit_repo_artifacts(args.plan.parent.parent.parent, issues),
    }
    result = AuditResult(
        passed=all(checks.values()) and not issues,
        issues=tuple(issues),
        checks=checks,
    )
    if args.json_output:
        print(json.dumps(result.to_json(), ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print("PASS" if result.passed else "FAIL")
        for issue in issues:
            print(f"- {issue}")
    return 0 if result.passed else 1


def parse_args(argv: Sequence[str]) -> AuditArgs:
    values = dict(zip(argv[0::2], argv[1::2], strict=False))
    if set(values) != {"--plan", "--evidence"} or len(argv) not in {4, 5}:
        raise SystemExit("usage: audit_plan_completion.py --plan PATH --evidence DIR [--json]")
    if len(argv) == 5 and argv[-1] != "--json":
        raise SystemExit("usage: audit_plan_completion.py --plan PATH --evidence DIR [--json]")
    return AuditArgs(
        plan=Path(values["--plan"]).resolve(),
        evidence=Path(values["--evidence"]).resolve(),
        json_output="--json" in argv,
    )


def audit_todos(plan: Path, issues: list[str]) -> bool:
    text = read_text(plan, issues)
    todo_ids = {int(match) for match in re.findall(r"^- \[x\] (\d+)\.", text, re.MULTILINE)}
    final_ids = set(re.findall(r"^- \[ \] (F\d+)\.", text, re.MULTILINE))
    missing = sorted(set(range(1, 9)) - todo_ids)
    if missing:
        issues.append(f"unchecked or missing todos: {missing}")
    if final_ids != {"F1", "F2", "F3", "F4"}:
        issues.append(f"final verification wave is incomplete: {sorted(final_ids)}")
    return not missing and final_ids == {"F1", "F2", "F3", "F4"}


def audit_evidence_files(evidence: Path, issues: list[str]) -> bool:
    passed = True
    for name in REQUIRED_TASK_EVIDENCE:
        path = evidence / name
        if not path.is_file() or path.stat().st_size == 0:
            issues.append(f"missing or empty required evidence: {path}")
            passed = False
    return passed


def audit_command_coverage(evidence: Path, issues: list[str]) -> bool:
    corpus = evidence_text(evidence)
    missing = [markers for markers in REQUIRED_COMMAND_MARKERS if not any(marker in corpus for marker in markers)]
    for markers in missing:
        issues.append(f"missing command coverage marker: one of {list(markers)}")
    return not missing


def audit_scope_checks(evidence: Path, issues: list[str]) -> bool:
    corpus = evidence_text(evidence).casefold()
    missing = [marker for marker in SCOPE_MARKERS if marker.casefold() not in corpus]
    for marker in missing:
        issues.append(f"missing scope-check marker in evidence: {marker}")
    return not missing


def audit_repo_artifacts(root: Path, issues: list[str]) -> bool:
    passed = True
    for rel in REQUIRED_REPO_PATHS:
        path = root / rel
        if not path.exists():
            issues.append(f"missing required repository artifact: {rel}")
            passed = False
    return passed


def evidence_text(evidence: Path) -> str:
    parts: list[str] = []
    for path in sorted(evidence.rglob("*")):
        if path.suffix not in {".txt", ".log", ".md", ".json"} or not path.is_file():
            continue
        parts.append(path.read_text(encoding="utf-8", errors="replace"))
    return "\n".join(parts)


def read_text(path: Path, issues: list[str]) -> str:
    if not path.is_file():
        issues.append(f"missing plan: {path}")
        return ""
    return path.read_text(encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
