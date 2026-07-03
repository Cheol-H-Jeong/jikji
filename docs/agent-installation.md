# Agent Installation

Jikji is a non-destructive local file-discovery layer for AI agents. It prepares
an explicit root into `.jikji/` maps, parser caches, and search artifacts. It
does not move, rename, delete, or reorganize user files.

## Install From GitHub

### Cargo install

```bash
cargo install --git https://github.com/nomadamas/jikji jikji-cli
jikji --help
```

For local development from a checkout:

```bash
git clone https://github.com/nomadamas/jikji.git ~/.local/share/jikji/repo
cd ~/.local/share/jikji/repo
cargo install --path crates/jikji-cli
jikji --help
```

### GitHub release binary

Download the release archive for your OS from GitHub Releases, verify the
published SHA-256 checksum, and place the `jikji` executable on `PATH`.

Maintainer releases build OS binaries through the release workflow. Rust crates
publish automatically from the trusted `main` publishing workflow, and a
crates.io token-secret publish path is only a documented fallback, not the
normal release path. See `docs/release-publishing.md` for the required
crates.io trusted-publisher repository configuration.

### One-sentence agent install

CLI 에이전트에게 가장 쉽게 시킬 한 문장:

```text
GitHub 저장소 https://github.com/NomaDamas/jikji 에서 Jikji를 설치하고, 내 CLI 에이전트들이 `jikji find`를 바로 쓰도록 Jikji skill까지 연결해줘.
```

```bash
mkdir -p ~/.local/share/jikji
if [ ! -d ~/.local/share/jikji/repo/.git ]; then
  git clone https://github.com/nomadamas/jikji.git ~/.local/share/jikji/repo
fi
cd ~/.local/share/jikji/repo
git pull --ff-only
cargo install --path crates/jikji-cli
jikji --help
```

## Install The Skill

```bash
jikji agent-skill-install --agent all --json
jikji hermes-skill-install --json
jikji codex-skill-install --json
jikji claude-skill-install --json
jikji opencode-skill-install --json
```

For an arbitrary local agent:

```bash
jikji skill-export --dest /path/to/agent/skills/jikji/SKILL.md --json
```

## Required Agent Behavior

When an explicit root is available, local file/folder/document discovery starts
with:

```bash
jikji find ROOT "natural language file clue" --json
```

The agent follows `handoff_action`:

- `direct_use`: use `answer_paths[]` / `paths[]`; verify only top evidence.
- `jikji_retry`: run exactly one sharper `jikji find` retry.
- `raw_fallback_after_retry`: raw search is allowed only after that retry failed,
  stayed empty, or stayed clearly wrong.

Do not start by crawling with `ls`, `find`, `rg`, `grep`, `tree`, or broad
document opening.

## Root Preparation

`jikji prepare ROOT` writes `.jikji/` artifacts and a bounded routing block in
`AGENTS.md`, `CLAUDE.md`, and `.cursorrules` by default. Use
`--no-agent-rules` when a root should not receive those agent-routing blocks.

```bash
jikji prepare /mnt/work-drive --json
jikji prepare /mnt/work-drive --no-agent-rules --json
jikji refresh /mnt/work-drive --json
jikji doctor /mnt/work-drive --json
```

Skill install queues a low-impact prepare contract for common user material
folders and document-heavy folders under the user home directory. It does not
move, rename, delete, or reorganize user files. Provide explicit roots or disable
the post-install prepare when needed:

```bash
jikji agent-skill-install --agent all --prepare-root /mnt/work-drive --json
jikji agent-skill-install --agent all --prepare-root /mnt/work-drive --foreground-prepare --json
jikji agent-skill-install --agent all --no-prepare --json
```

The hidden compatibility command `jikji post-install-prepare ROOT --json` runs
queued roots in the foreground.

## Runtime Notes

The default Rust CLI does not require Python for `prepare`, `find`, `search`,
`doctor`, `map`, or GUI status/search. Eval generation, public benchmark
fixture commands, local benchmark smoke helpers, `hippocamp-fetch`,
`hermes-bench`, `hermes-compare`, and `benchmark-value-report` are Python-only
benchmark compatibility surfaces. They report Python-only status because
benchmark parity must use the same Python evaluator for both Python Jikji and
Rust Jikji. Image/audio/video OCR-ASR remains an explicit opt-in through the
Python media bridge.

Downstream tools can reuse split crates directly instead of shelling out:
`jikji-parser`, `jikji-index`, `jikji-search`, and `jikji-agent`. The
`jikji-bench` crate is internal and is not published.

## Benchmark Language

Use the public label `Jikji find` in reports and dashboards. Headline comparisons
should be raw local agent vs the same agent with Jikji find attached. Internal
benchmark implementation names should not appear in user-facing setup guidance.
