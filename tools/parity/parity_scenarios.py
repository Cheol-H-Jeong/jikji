from __future__ import annotations

import shutil
from pathlib import Path

from golden_fixtures import (
    CliStep,
    GoldenScenario,
    build_answer_pack_shell,
    build_ascii_cjk,
    build_clean_safety,
    build_stale_index,
    build_structured_archive_media,
)


def _scenarios() -> tuple[GoldenScenario, ...]:
    return (
        GoldenScenario("ascii_cjk_paths", ()),
        GoldenScenario("structured_archive_media", ()),
        GoldenScenario("answer_pack_shell", ()),
        GoldenScenario("stale_index_find", ()),
        GoldenScenario("clean_safety", ()),
    )


def _generated_temp_scenario() -> GoldenScenario:
    return GoldenScenario(
        "generated_temp_corpus",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("search_generated", ("search", "{root}", "alpha needle", "--json")),
            CliStep("find_generated", ("find", "{root}", "renewal clause", "--json")),
            CliStep("doctor", ("doctor", "{root}", "--json")),
        ),
    )


def _build_scenario(name: str, root: Path) -> GoldenScenario:
    root.mkdir(parents=True, exist_ok=True)
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
        case "generated_temp_corpus":
            return _build_generated_temp_corpus(root)
        case unreachable:
            raise RuntimeError(f"unknown scenario: {unreachable}")


def _build_generated_temp_corpus(root: Path) -> GoldenScenario:
    (root / "alpha-renewal.md").write_text(
        "alpha needle renewal clause answer marker generated-1001",
        encoding="utf-8",
    )
    (root / "finance").mkdir(exist_ok=True)
    (root / "finance" / "invoice.csv").write_text(
        "vendor,marker\nACME,generated-invoice-2002\n",
        encoding="utf-8",
    )
    (root / "자료").mkdir(exist_ok=True)
    (root / "자료" / "회의.txt").write_text("서울 생성 코퍼스 marker generated-cjk-3003", encoding="utf-8")
    return _generated_temp_scenario()


def _copy_tree(source: Path, target: Path) -> None:
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target)
