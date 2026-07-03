from __future__ import annotations

import re
from collections import Counter
from pathlib import Path
from typing import TYPE_CHECKING, Final

from parity_artifact_schema import REQUIRED_ARTIFACT_FILES, _artifact_schema_errors
from parity_json import Json

if TYPE_CHECKING:
    from parity_commands import CommandPair, ScenarioResult

REQUIRED_FIND_KEYS: Final = frozenset({
    "answer_paths", "candidates", "handoff_action", "paths", "query_variants",
    "raw_fallback_allowed", "retry_proof", "tool_call_policy",
})
WIKI_SOURCE_RE: Final = re.compile(r"^\.jikji/wiki/sources/(?P<stem>.+)-[0-9a-f]{12}\.md$")


def _contract_failures(results: tuple[ScenarioResult, ...]) -> list[str]:
    failures: list[str] = []
    for scenario in results:
        for pair in scenario.commands:
            failures.extend(_command_failures(pair))
        artifact_failures = scenario.artifact_summary.get("contract_failures")
        if isinstance(artifact_failures, list) and artifact_failures:
            failures.extend(f"{scenario.name}: generated artifact contract {failure}" for failure in artifact_failures)
    return failures


def _command_failures(pair: CommandPair) -> list[str]:
    failures: list[str] = []
    if pair.python.exit_code != pair.rust.exit_code:
        failures.append(
            f"{pair.scenario}/{pair.python.name}: exit code python={pair.python.exit_code} rust={pair.rust.exit_code}"
        )
    if isinstance(pair.python.stdout_json, dict) and not isinstance(pair.rust.stdout_json, dict):
        failures.append(f"{pair.scenario}/{pair.python.name}: Rust did not emit JSON object")
        return failures
    if isinstance(pair.python.stdout_json, dict) and isinstance(pair.rust.stdout_json, dict):
        missing = sorted(set(pair.python.stdout_json) - set(pair.rust.stdout_json))
        if missing:
            failures.append(f"{pair.scenario}/{pair.python.name}: Rust missing JSON keys {missing}")
        extra = sorted(set(pair.rust.stdout_json) - set(pair.python.stdout_json))
        if extra:
            failures.append(f"{pair.scenario}/{pair.python.name}: Rust extra JSON keys {extra}")
        if pair.python.name.startswith("find"):
            missing_required = sorted(REQUIRED_FIND_KEYS - set(pair.rust.stdout_json))
            if missing_required:
                failures.append(
                    f"{pair.scenario}/{pair.python.name}: Rust missing required find keys {missing_required}"
                )
        python_order = _ranked_paths(pair.python.stdout_json)
        rust_order = _ranked_paths(pair.rust.stdout_json)
        if (python_order or rust_order) and python_order != rust_order:
            failures.append(
                f"{pair.scenario}/{pair.python.name}: ranking mismatch python={python_order} rust={rust_order}"
            )
    return failures


def _ranked_paths(payload: dict[str, Json]) -> list[str]:
    candidates = payload.get("candidates")
    if isinstance(candidates, list) and candidates:
        return [
            item["path"]
            for item in candidates
            if isinstance(item, dict) and isinstance(item.get("path"), str)
        ]
    paths = payload.get("paths")
    if isinstance(paths, list):
        return [item for item in paths if isinstance(item, str)]
    return []


def _artifact_diff_summary(
    python_rows: tuple[dict[str, Json], ...],
    rust_rows: tuple[dict[str, Json], ...],
    python_root: Path,
    rust_root: Path,
) -> dict[str, Json]:
    python_artifacts = {item["path"]: item for item in python_rows}
    rust_artifacts = {item["path"]: item for item in rust_rows}
    shared = sorted(set(python_artifacts) & set(rust_artifacts))
    digest_mismatch = [
        path
        for path in shared
        if python_artifacts[path]["sha256"] != rust_artifacts[path]["sha256"]
    ]
    schema_expected = _has_generated_artifact_contract(tuple(str(path) for path in python_artifacts))
    schema_errors, validated_paths = (
        _artifact_schema_errors(rust_root) if schema_expected else ([], frozenset())
    )
    source_non_parity, source_contract_failures = _wiki_source_policy(
        tuple(str(path) for path in python_artifacts),
        tuple(str(path) for path in rust_artifacts),
    )
    contract_missing = [
        path
        for path in sorted(set(python_artifacts) - set(rust_artifacts))
        if not _is_wiki_source_path(path)
    ]
    doc_text_errors = _doc_text_contract_errors(python_artifacts, rust_artifacts)
    schema_error_paths = {_artifact_error_path(error) for error in schema_errors}
    doc_text_error_paths = {_artifact_error_path(error) for error in doc_text_errors}
    contract_digest_mismatch = [
        path
        for path in digest_mismatch
        if not _is_non_contract_digest(path, validated_paths)
        and path not in schema_error_paths
        and path not in doc_text_error_paths
    ]
    non_contract_digest_mismatch = [
        path
        for path in digest_mismatch
        if _is_non_contract_digest(path, validated_paths) and path not in doc_text_error_paths
    ]
    extra_generated = [
        path
        for path in sorted(set(rust_artifacts) - set(python_artifacts))
        if not schema_expected and _is_generated_artifact_path(path)
    ]
    contract_failures = _dedupe(
        [
            *contract_missing,
            *schema_errors,
            *doc_text_errors,
            *source_contract_failures,
            *extra_generated,
            *contract_digest_mismatch,
        ]
    )
    return {
        "python_count": len(python_artifacts),
        "rust_count": len(rust_artifacts),
        "missing_in_rust": sorted(set(python_artifacts) - set(rust_artifacts)),
        "extra_in_rust": sorted(set(rust_artifacts) - set(python_artifacts)),
        "digest_mismatch": digest_mismatch,
        "non_contract_digest_mismatch": non_contract_digest_mismatch,
        "intentional_non_parity": [*source_non_parity, *non_contract_digest_mismatch],
        "contract_failures": contract_failures,
    }


def _has_generated_artifact_contract(paths: tuple[str, ...]) -> bool:
    for rel in paths:
        if _is_generated_artifact_path(rel):
            return True
    return False


def _wiki_source_policy(
    python_paths: tuple[str, ...],
    rust_paths: tuple[str, ...],
) -> tuple[list[str], list[str]]:
    python_stems = _wiki_source_stems(python_paths)
    rust_stems = _wiki_source_stems(rust_paths)
    if python_stems == rust_stems:
        if python_stems:
            return ([f"wiki source slug hash differs for stems {sorted(python_stems)}"], [])
        return ([], [])
    return (
        [],
        [
            "wiki source semantic mismatch "
            f"python={dict(sorted(python_stems.items()))} rust={dict(sorted(rust_stems.items()))}"
        ],
    )


def _wiki_source_stems(paths: tuple[str, ...]) -> Counter[str]:
    stems: Counter[str] = Counter()
    for path in paths:
        match = WIKI_SOURCE_RE.match(path)
        if match:
            stems[match.group("stem")] += 1
    return stems


def _is_wiki_source_path(path: str) -> bool:
    return WIKI_SOURCE_RE.match(path) is not None


def _is_generated_artifact_path(path: str) -> bool:
    if path in REQUIRED_ARTIFACT_FILES:
        return True
    if _is_wiki_source_path(path):
        return True
    return path.startswith((".jikji/doc_text/sha256_", ".jikji/doc_meta/sha256_"))


def _is_non_contract_digest(path: str, validated_paths: frozenset[str]) -> bool:
    suffix = Path(path).suffix
    if _is_doc_text_path(path):
        return True
    if suffix == ".md":
        return True
    if suffix in {".json", ".jsonl"}:
        return path in validated_paths
    return False


def _doc_text_contract_errors(
    python_artifacts: dict[str, dict[str, Json]],
    rust_artifacts: dict[str, dict[str, Json]],
) -> list[str]:
    errors: list[str] = []
    for path, python_artifact in sorted(python_artifacts.items()):
        if not _is_doc_text_path(path):
            continue
        rust_artifact = rust_artifacts.get(path)
        if rust_artifact is None:
            continue
        python_bytes = _artifact_byte_count(python_artifact)
        rust_bytes = _artifact_byte_count(rust_artifact)
        if python_bytes > 0 and rust_bytes <= 0:
            errors.append(f"{path}: doc_text cache is empty")
    return errors


def _artifact_byte_count(artifact: dict[str, Json]) -> int:
    value = artifact.get("bytes")
    if isinstance(value, int) and not isinstance(value, bool):
        return value
    return 0


def _is_doc_text_path(path: str) -> bool:
    return path.startswith(".jikji/doc_text/sha256_") and path.endswith(".txt")


def _dedupe(items: list[str]) -> list[str]:
    return list(dict.fromkeys(items))


def _artifact_error_path(error: str) -> str:
    return error.split(":", 1)[0]
