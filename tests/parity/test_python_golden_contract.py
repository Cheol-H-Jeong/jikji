from __future__ import annotations

import json
from pathlib import Path

GOLDEN_ROOT = Path(__file__).resolve().parents[1] / "golden" / "python"


def test_python_golden_manifest_covers_required_scenarios() -> None:
    manifest = json.loads((GOLDEN_ROOT / "manifest.json").read_text(encoding="utf-8"))
    names = {scenario["name"] for scenario in manifest["scenarios"]}

    assert {
        "ascii_cjk_paths",
        "structured_archive_media",
        "answer_pack_shell",
        "stale_index_find",
        "clean_safety",
    } <= names


def test_shell_noise_golden_does_not_accept_raw_dangerous_tokens() -> None:
    commands = json.loads(
        (GOLDEN_ROOT / "scenarios" / "answer_pack_shell" / "commands.json").read_text(
            encoding="utf-8"
        )
    )
    shell = next(command for command in commands if command["name"] == "find_shell_noise")
    names = {command["name"] for command in commands}
    variants = " ".join(shell["stdout_json"]["query_variants"]).casefold()

    assert {"find_shell_retry_forged", "find_shell_retry_exhausted"} <= names
    assert "rm" not in variants
    assert "-rf" not in variants
    assert "$(" not in variants
    assert shell["stdout_json"]["raw_fallback_allowed"] is False


def test_python_golden_artifacts_exist_and_parse() -> None:
    for scenario_dir in (GOLDEN_ROOT / "scenarios").iterdir():
        generated = json.loads((scenario_dir / "generated_files.json").read_text(encoding="utf-8"))
        commands = json.loads((scenario_dir / "commands.json").read_text(encoding="utf-8"))

        assert commands
        assert all("exit_code" in command for command in commands)
        assert isinstance(generated, list)


def test_stale_index_golden_keeps_mutation_and_previous_find() -> None:
    commands = json.loads(
        (GOLDEN_ROOT / "scenarios" / "stale_index_find" / "commands.json").read_text(
            encoding="utf-8"
        )
    )
    names = [command["name"] for command in commands]

    assert names == ["prepare", "mutate_after_prepare", "find_stale_previous"]


def test_root_agent_map_artifacts_are_present() -> None:
    for scenario_dir in (GOLDEN_ROOT / "scenarios").iterdir():
        generated = json.loads((scenario_dir / "generated_files.json").read_text(encoding="utf-8"))
        paths = {row["path"] for row in generated}
        if ".jikji_agent_map.md" not in paths:
            continue

        assert (scenario_dir / "artifacts" / ".jikji_agent_map.md").is_file()
