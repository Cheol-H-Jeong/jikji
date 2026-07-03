from __future__ import annotations

from typing import Final

REQUIRED_CRATES: Final = (
    "jikji-core",
    "jikji-parser",
    "jikji-index",
    "jikji-search",
    "jikji-agent",
    "jikji-media-bridge",
    "jikji-cli",
)
REQUIRED_RUST_COMMANDS: Final = (
    "prepare", "refresh", "clean", "map", "doctor", "find", "search", "brief",
    "discover", "graph", "gui", "agent-skill-install", "hermes-skill-install",
    "codex-skill-install", "omx-skill-install", "claude-skill-install",
    "opencode-skill-install", "openclo-skill-install", "nanoclo-skill-install",
    "skill-export", "eval-generate", "eval-generate-realistic",
    "eval-generate-holdout", "eval", "bench-analyze", "hippocamp-import",
    "bench-run", "bench-iterate", "hippocamp-fetch", "beir-import",
    "beir-suite", "edith-summary", "edith-import", "edith-suite",
    "publicdata-build", "publicdata-suite", "workspacebench-build",
    "workspacebench-suite", "hardbench-build", "hardbench-suite",
    "hippocamp-suite", "hermes-bench", "hermes-compare",
    "benchmark-value-report",
)
PYTHON_BENCHMARK_COMPAT_RATIONALE: Final = {
    "eval-generate": "Python-only benchmark evaluator compatibility command",
    "eval-generate-realistic": "Python-only benchmark evaluator compatibility command",
    "eval-generate-holdout": "Python-only benchmark evaluator compatibility command",
    "eval": "Python-only benchmark evaluator compatibility command",
    "bench-analyze": "Python-only benchmark evaluator compatibility command",
    "hippocamp-import": "Python-only benchmark evaluator compatibility command",
    "bench-run": "Python-only benchmark evaluator compatibility command",
    "bench-iterate": "Python-only benchmark evaluator compatibility command",
    "hippocamp-fetch": "Python-only benchmark fixture compatibility command",
    "beir-import": "Python-only benchmark fixture compatibility command",
    "beir-suite": "Python-only benchmark suite compatibility command",
    "edith-summary": "Python-only benchmark fixture compatibility command",
    "edith-import": "Python-only benchmark fixture compatibility command",
    "edith-suite": "Python-only benchmark suite compatibility command",
    "publicdata-build": "Python-only benchmark fixture compatibility command",
    "publicdata-suite": "Python-only benchmark suite compatibility command",
    "workspacebench-build": "Python-only benchmark fixture compatibility command",
    "workspacebench-suite": "Python-only benchmark suite compatibility command",
    "hardbench-build": "Python-only benchmark fixture compatibility command",
    "hardbench-suite": "Python-only benchmark suite compatibility command",
    "hippocamp-suite": "Python-only benchmark suite compatibility command",
    "hermes-bench": "Python-only Hermes benchmark compatibility command",
    "hermes-compare": "Python-only Hermes report comparison compatibility command",
    "benchmark-value-report": "Python-only benchmark value report compatibility command",
}
