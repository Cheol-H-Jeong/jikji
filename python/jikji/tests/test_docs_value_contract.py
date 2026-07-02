from __future__ import annotations

import json
from pathlib import Path


def _repo_root() -> Path:
    for candidate in Path(__file__).resolve().parents:
        if (candidate / "Cargo.toml").is_file() and (candidate / "README.md").is_file():
            return candidate
    raise RuntimeError("Cannot locate repository root")


ROOT = _repo_root()


def _read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_value_report_headline_accuracy_floor_passes_raw_parity():
    report = json.loads(_read("docs/jikji-value-report.json"))
    raw = report["modes"]["raw"]
    recommended = report["modes"]["jikji-one-call-raw-floor"]

    assert report["headline_strategy"] == "jikji-one-call-raw-floor"
    assert recommended["hit_at_1"] >= raw["hit_at_1"]
    assert recommended["hit_at_10"] >= raw["hit_at_10"]
    assert report["one_call_policy"]["calls_per_cycle"] == 1
    assert report["one_call_policy"]["raw_floor_rule"] == "select_raw_profile_if_one_call_hit_at_1_or_hit_at_10_is_lower_than_raw"
    assert report["two_call_policy"]["calls_per_cycle"] == 2


def test_landing_page_rotates_requested_hook_and_actual_numbers():
    html = _read("docs/jikji-value.html")

    assert "파일 하나 찾을 때마다 <span id=\"rotating-hook\"" in html
    assert "파일 하나 찾는데 <span id=\"rotating-hook\"" not in html
    assert "파일 하나 찾는데 (<span id=\"rotating-hook\"" not in html
    assert "38,650 토큰 쓰는 거" in html
    assert "57초 찾는 거" in html
    assert "11.7번 LLM 호출하는 거" in html
    assert "24원 쓰는 거" in html
    assert "평균 38,650 토큰 쓰는 거" not in html
    assert "평균 57초 찾는 거" not in html
    assert "평균 11.7번 LLM 호출하는 거" not in html
    assert "평균 24원 쓰는 거" not in html
    assert '"21,296,278 토큰 사용하는 거"' not in html
    assert '"520분동안 찾는 거"' not in html
    assert '"6,420번 LLM 호출하는 거"' not in html
    assert '"13,361원 사용하는 거"' not in html
    assert "id=\"rotating-hook\"" in html


def test_index_is_a_real_landing_page_not_only_redirect():
    html = _read("docs/index.html")

    assert "http-equiv=\"refresh\"" not in html
    assert "Jikji find" in html
    assert "파일 하나 찾을 때마다" in html
    assert "GitHub에서 바로 보기" in html
    assert "https://github.com/NomaDamas/jikji" in html
    assert "GitHub Pages 정적 호스팅" in html
    assert "jikji find ROOT \"query\" --json" in html
    assert "href=\"#agent-install\"" in html
    assert "에이전트 설치 지시문" in html
    assert "CLI 에이전트에게 아래 한 문장만 그대로 말하면 된다" in html
    assert "GitHub 저장소 https://github.com/NomaDamas/jikji 에서 Jikji를 설치하고" in html
    assert "mkdir -p ~/.local/share/jikji" not in html
    assert "href=\"./agent-installation.md\"" not in html
    assert "비교 실험 결과" in html
    assert "가장 큰 fullset 증거" not in html
    assert "Fullset value proof" not in html
    assert "호출/토큰/시간 장부 보기" in html
    assert "./jikji-benchmarks.html#usage-ledger" in html


def test_benchmark_report_has_actual_agent_usage_ledger():
    html = _read("docs/jikji-benchmarks.html")

    assert "id=\"usage-ledger\"" in html
    assert "벤치마크별 LLM 사용량 장부" in html
    assert "Input tokens" in html
    assert "Output tokens" in html
    assert "Seconds" in html
    assert "raw Hermes" in html
    assert "Jikji find" in html
    assert "19,799,362" in html
    assert "1,496,916" in html
    assert "31,231.883" in html
    assert "Media OCR/ASR" in html
    assert "not recorded" in html


def test_public_docs_expose_find_not_legacy_search_commands():
    readme = _read("README.md")
    public_paths = [
        "README.md",
        "skills/jikji/SKILL.md",
        "docs/agent-usage.md",
        "docs/agent-installation.md",
        "docs/local-agent-search-standard.md",
        "docs/index.html",
        "docs/jikji-value.html",
        "docs/jikji-benchmarks.html",
        "docs/hippocamp-rerun-report.md",
    ]
    combined = "\n".join(_read(path) for path in public_paths)

    assert "파일 하나 찾을 때마다 38,650 토큰" in readme
    assert "파일 하나당 447 토큰, 2.1초, 1회 호출" in readme
    assert "https://nomadamas.github.io/jikji/" in readme
    assert "docs/jikji-readme-hero.svg" in readme
    assert "GitHub Pages" in readme
    assert "Tell your CLI agent this one sentence" in readme
    assert "One-line install for a CLI agent" not in readme
    assert "agent-skill-install --agent all --json" in combined
    assert "파일 하나 찾는데 평균" not in readme
    assert "평균 447 토큰" not in readme
    assert "table is the fullset total" in readme
    assert "jikji find ROOT \"query\" --json" in combined
    assert "Hit@1 improves from `0.6697` to `0.7949`" in combined
    assert "Jikji find" in combined
    forbidden = [
        "jikji discover",
        "jikji brief",
        "jikji search",
        "Jikji one-call",
        "Jikji two-call",
        "Jikji answer-pack",
        "raw-floor",
        "answer-pack",
        "accuracy-first",
    ]
    for text in forbidden:
        assert text not in combined
