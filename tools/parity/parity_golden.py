from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from parity_capture import _command_record
from parity_json import Json, _read_json

if TYPE_CHECKING:
    from parity_commands import ScenarioResult


def _assert_python_matches_checked_in(fixtures: Path, results: tuple[ScenarioResult, ...]) -> None:
    expected_manifest = _read_json(fixtures / "manifest.json")
    actual_scenarios = []
    for scenario in results:
        scenario_dir = fixtures / "scenarios" / scenario.name
        expected_commands = _read_json(scenario_dir / "commands.json")
        expected_cli_commands = _cli_command_records(expected_commands)
        actual_commands = [_command_record(pair.python) for pair in scenario.commands]
        if actual_commands != expected_cli_commands:
            raise RuntimeError(f"checked-in golden command mismatch: {scenario.name}")
        expected_files = _read_json(scenario_dir / "generated_files.json")
        actual_files = list(scenario.python_artifacts)
        actual_contract = _artifact_contract(actual_files)
        expected_contract = _artifact_contract(expected_files)
        if actual_contract != expected_contract:
            raise RuntimeError(
                f"checked-in golden artifact mismatch: {scenario.name}: "
                f"{_first_contract_diff(expected_contract, actual_contract)}"
            )
        actual_scenarios.append(
            {"name": scenario.name, "commands": len(expected_commands), "artifacts": actual_files}
        )
    actual_manifest = _manifest_without_digest(actual_scenarios)
    if _manifest_without_digest(expected_manifest["scenarios"]) != actual_manifest:
        raise RuntimeError("checked-in golden manifest scenario summary mismatch")


def _artifact_contract(rows: Json) -> list[dict[str, Json]]:
    if not isinstance(rows, list):
        raise RuntimeError("artifact list is not a JSON array")
    return [
        {"path": row["path"], "sha256": row["sha256"]}
        for row in rows
        if isinstance(row, dict) and isinstance(row.get("path"), str)
    ]


def _cli_command_records(records: Json) -> list[dict[str, Json]]:
    if not isinstance(records, list):
        raise RuntimeError("commands fixture is not a JSON array")
    return [
        record
        for record in records
        if isinstance(record, dict)
        and isinstance(record.get("command"), list)
        and record["command"]
        and record["command"][0] != "__mutate__"
    ]


def _first_contract_diff(
    expected: list[dict[str, Json]],
    actual: list[dict[str, Json]],
) -> str:
    for left, right in zip(expected, actual, strict=False):
        if left != right:
            return f"expected={left} actual={right}"
    return f"expected_count={len(expected)} actual_count={len(actual)}"


def _manifest_without_digest(scenarios: Json) -> dict[str, Json]:
    if not isinstance(scenarios, list):
        raise RuntimeError("manifest scenarios is not a JSON array")
    return {
        "schema_version": 1,
        "reference": "python",
        "scenarios": [
            {
                "name": item["name"],
                "commands": item["commands"],
                "artifacts": _artifact_contract(item["artifacts"]),
            }
            for item in scenarios
            if isinstance(item, dict)
        ],
    }
