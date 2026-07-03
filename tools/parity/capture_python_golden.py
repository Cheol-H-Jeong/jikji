#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
# How to run:
# python3 tools/parity/capture_python_golden.py --python-repo /Users/jeffrey/Projects-dev/jikji --out tests/golden/python

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from golden_fixtures import (
    CliStep,
    GoldenScenario,
    apply_mutation,
    build_answer_pack_shell,
    build_ascii_cjk,
    build_clean_safety,
    build_stale_index,
    build_structured_archive_media,
)

Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]
TIME_KEYS = frozenset({"generated_at", "indexed_at", "mtime", "mtime_ns", "created", "modified"})
ARTIFACT_EXTENSIONS = frozenset({".json", ".jsonl", ".md", ".txt"})
TIMEOUT_S = 20


@dataclass(frozen=True, slots=True)
class CaptureArgs:
    python_repo: Path
    out: Path


@dataclass(frozen=True, slots=True)
class CommandRecord:
    name: str
    command: tuple[str, ...]
    exit_code: int
    stdout: str
    stderr: str
    stdout_json: Json
    actual_retry_proof: str


@dataclass(frozen=True, slots=True)
class Runtime:
    python_repo: Path
    root: Path
    retry_proof: str


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    _prepare_output_dir(args.out)
    with tempfile.TemporaryDirectory(prefix="jikji-python-golden-") as temp:
        temp_root = Path(temp)
        records = [_capture_scenario(args, temp_root, scenario) for scenario in _scenarios()]
    manifest = _write_capture_manifest(args.out, records)
    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    return 0


def _parse_args(argv: list[str]) -> CaptureArgs:
    values = dict(zip(argv[0::2], argv[1::2], strict=False))
    if set(values) != {"--python-repo", "--out"} or len(argv) != 4:
        raise SystemExit("usage: capture_python_golden.py --python-repo PATH --out PATH")
    python_repo = Path(values["--python-repo"]).expanduser().resolve()
    if not (_python_source_root(python_repo) / "jikji" / "__main__.py").exists():
        raise SystemExit(f"not a Jikji Python repo: {python_repo}")
    return CaptureArgs(python_repo=python_repo, out=Path(values["--out"]).resolve())


def _prepare_output_dir(out: Path) -> None:
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)


def _scenarios() -> tuple[GoldenScenario, ...]:
    return (
        GoldenScenario("ascii_cjk_paths", ()),
        GoldenScenario("structured_archive_media", ()),
        GoldenScenario("answer_pack_shell", ()),
        GoldenScenario("stale_index_find", ()),
        GoldenScenario("clean_safety", ()),
    )


def _capture_scenario(args: CaptureArgs, temp_root: Path, template: GoldenScenario) -> dict[str, Json]:
    root = temp_root / template.name
    root.mkdir()
    scenario = _rebuild_scenario(template.name, root)
    scenario_dir = args.out / "scenarios" / scenario.name
    scenario_dir.mkdir(parents=True)
    commands: list[CommandRecord] = []
    retry_proof = ""
    for step in scenario.steps:
        if step.args[0] == "__mutate__":
            apply_mutation(root, step.args[1])
            commands.append(CommandRecord(step.name, step.args, 0, "", "", None, ""))
            continue
        record = _run_cli(Runtime(args.python_repo, root, retry_proof), step)
        commands.append(record)
        if step.name == "find_shell_noise" and isinstance(record.stdout_json, dict):
            retry_proof = record.actual_retry_proof
            _assert_shell_noise_clean(record.stdout_json)
    _write_json(scenario_dir / "commands.json", [_command_json(command) for command in commands])
    artifacts = _capture_artifacts(root, scenario_dir)
    return {"name": scenario.name, "commands": len(commands), "artifacts": artifacts}


def _rebuild_scenario(name: str, root: Path) -> GoldenScenario:
    match name:
        case "ascii_cjk_paths":
            return build_ascii_cjk(root)
        case "structured_archive_media":
            return build_structured_archive_media(root)
        case "answer_pack_shell":
            return build_answer_pack_shell(root)
        case "stale_index_find":
            return build_stale_index(root)
        case "clean_safety":
            return build_clean_safety(root)
        case unreachable:
            raise RuntimeError(f"unknown scenario: {unreachable}")


def _run_cli(runtime: Runtime, step: CliStep) -> CommandRecord:
    command = tuple(
        str(runtime.root) if item == "{root}" else runtime.retry_proof if item == "{retry_proof}" else item
        for item in step.args
    )
    env = {**os.environ, "PYTHONPATH": str(_python_source_root(runtime.python_repo))}
    completed = subprocess.run(
        (sys.executable, "-m", "jikji.__main__", *command),
        cwd=runtime.python_repo,
        env=env,
        text=True,
        capture_output=True,
        timeout=TIMEOUT_S,
        check=False,
    )
    actual_retry_proof = _extract_retry_proof(completed.stdout)
    proof_for_output = actual_retry_proof or runtime.retry_proof
    stdout = _normalize_text(completed.stdout, runtime.root, proof_for_output)
    stderr = _normalize_text(completed.stderr, runtime.root, proof_for_output)
    parsed = _parse_stdout_json(stdout)
    normalized_command = tuple(_normalize_text(item, runtime.root, proof_for_output) for item in command)
    return CommandRecord(
        step.name,
        normalized_command,
        completed.returncode,
        stdout,
        stderr,
        parsed,
        actual_retry_proof,
    )


def _python_source_root(python_repo: Path) -> Path:
    monorepo_source = python_repo / "python" / "jikji" / "src"
    if (monorepo_source / "jikji" / "__main__.py").exists():
        return monorepo_source
    return python_repo / "src"


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


def _normalize_json(value) -> Json:
    if value is None or isinstance(value, bool | int | float | str):
        return value
    if isinstance(value, list):
        return [_normalize_json(item) for item in value]
    if isinstance(value, dict):
        return {
            str(key): "<TIMESTAMP>" if str(key) in TIME_KEYS else _normalize_json(item)
            for key, item in value.items()
        }
    raise TypeError(f"unsupported JSON value: {type(value).__name__}")


def _normalize_text(text: str, root: Path, retry_proof: str = "") -> str:
    normalized = text
    for root_form in sorted({str(root), str(root.resolve())}, key=len, reverse=True):
        normalized = normalized.replace(root_form, "<SCENARIO_ROOT>")
    normalized = re.sub(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})",
        "<TIMESTAMP>",
        normalized,
    )
    if retry_proof:
        normalized = normalized.replace(retry_proof, "<RETRY_PROOF>")
    return normalized


def _assert_shell_noise_clean(payload: dict[str, Json]) -> None:
    variants = payload.get("query_variants")
    joined = " ".join(item for item in variants if isinstance(item, str)) if isinstance(variants, list) else ""
    forbidden = {"rm", "-rf", "$(", "semi;"}
    leaked = [token for token in forbidden if token in joined.casefold()]
    if leaked:
        raise RuntimeError(f"shell-noise tokens leaked into query variants: {leaked}")


def _command_json(record: CommandRecord) -> dict[str, Json]:
    return {
        "name": record.name,
        "command": list(record.command),
        "exit_code": record.exit_code,
        "stdout": record.stdout,
        "stderr": record.stderr,
        "stdout_json": record.stdout_json,
    }


def _capture_artifacts(root: Path, scenario_dir: Path) -> list[dict[str, Json]]:
    artifacts_dir = scenario_dir / "artifacts"
    artifacts_dir.mkdir()
    rows: list[dict[str, Json]] = []
    for path in sorted((root / ".jikji").rglob("*")) + sorted(root.glob(".jikji_agent_map.md")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if path.suffix in ARTIFACT_EXTENSIONS:
            text = _normalized_artifact_text(path, root)
            target = artifacts_dir / rel.replace("/", "__")
            target.write_text(text, encoding="utf-8")
            digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
        else:
            digest = "<BINARY_ARTIFACT>"
        rows.append({"path": rel, "sha256": digest, "bytes": path.stat().st_size})
    _write_json(scenario_dir / "generated_files.json", rows)
    return rows


def _normalized_artifact_text(path: Path, root: Path) -> str:
    text = _normalize_text(path.read_text(encoding="utf-8", errors="replace"), root)
    if path.suffix == ".json":
        return json.dumps(_normalize_json(json.loads(text)), ensure_ascii=False, indent=2, sort_keys=False) + "\n"
    if path.suffix == ".jsonl":
        lines = [json.dumps(_normalize_json(json.loads(line)), ensure_ascii=False, sort_keys=False) for line in text.splitlines() if line]
        return "\n".join(lines) + ("\n" if lines else "")
    return text


def _write_capture_manifest(out: Path, scenarios: list[dict[str, Json]]) -> dict[str, Json]:
    digest = hashlib.sha256()
    for path in sorted(out.rglob("*")):
        if path.is_file():
            digest.update(path.relative_to(out).as_posix().encode("utf-8"))
            digest.update(path.read_bytes())
    manifest: dict[str, Json] = {
        "schema_version": 1,
        "reference": "python",
        "scenarios": scenarios,
        "deterministic_hash": digest.hexdigest(),
    }
    _write_json(out / "manifest.json", manifest)
    return manifest


def _write_json(path: Path, value: Json) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
