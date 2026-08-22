from __future__ import annotations

from typing import Final

REQUIRED_CRATES: Final = (
    "jikji-core",
    "jikji-parser",
    "jikji-index",
    "jikji-search",
    "jikji-agent",
    "jikji-media-bridge",
    "jikji-bench",
    "jikji-hermes-bench",
    "jikji-public-datasets",
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
PYTHON_BENCHMARK_COMPAT_RATIONALE: Final = {}
