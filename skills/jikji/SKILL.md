---
name: jikji
description: Use Jikji to find local files, folders, metadata, and parsed document text through a non-destructive prebuilt agent map/search index.
---

# Jikji Local File Discovery Skill

Use this skill in any coding, desktop, or local-agent environment when the user
asks you to find, identify, compare, or verify local files, folders, filenames,
metadata, or document contents under an explicit root path. Jikji is for agent
discovery, not physical file organization.

When this skill is installed, it should be selected automatically for local
file/folder/document discovery requests in any agent that supports skills,
persistent instructions, project memory, or prompt snippets. Do not wait for the
user to mention "Jikji" if the task is plainly about finding local paths,
filenames, metadata, or document contents under a bounded root.

Trigger examples:

- "Find the proposal PDF in this project."
- "Search this drive for the document that mentions the vendor renewal."
- "Which local file contains this phrase?"
- "Locate the spreadsheet about last quarter's budget."


## If Jikji is missing, install it from GitHub

When a local file-discovery task needs Jikji but the `jikji` CLI is not available, the agent should install it from GitHub instead of giving up:

```bash
mkdir -p ~/.local/share/jikji
if [ ! -d ~/.local/share/jikji/repo/.git ]; then
  git clone https://github.com/nomadamas/jikji.git ~/.local/share/jikji/repo
fi
cd ~/.local/share/jikji/repo
git pull --ff-only
python3 -m venv .venv
.venv/bin/pip install -e .
.venv/bin/jikji --help
```

Then use `~/.local/share/jikji/repo/.venv/bin/jikji` for all commands, or add that venv's `bin` directory to the agent session PATH. For Hermes/Codex/CLI agents, this bootstrap is part of the skill protocol whenever the CLI is absent and network access is available.

## Absolute rule: Jikji search FIRST, never crawl blind

When Jikji is installed and a root has a `.jikji/` index, you MUST treat
`jikji discover` / `jikji find` / `jikji brief --compact` / `jikji search` as the **mandatory first action** for any local
file/folder/document discovery request. This is non-negotiable:
The goal is not to avoid LLM calls blindly; the goal is to beat raw grep/find
exploration on accuracy while reducing calls/tokens. Use Jikji first, then use
LLM judgment, query rewrites, and targeted original-file verification when the
first deterministic result is not sufficient.

- **NEVER** start by running `grep`, `rg`, `ls`, `find`, `fd`, `cat`, `tree`,
  or any manual filesystem crawl to locate a file. Jikji has already built the
  map; re-crawling wastes turns and is strictly slower.
- Your **first tool call** for general file discovery must be:
  `jikji discover /explicit/root "natural language clue" --top-k 20 --json`
  Use `jikji find … --first` only when the task is definitely a single-file path lookup.
  Use `jikji brief … --compact --json` when extra evidence/wiki/cache hints are needed.
- For JSON `discover` / `brief`, accept `paths[]` or `candidates[].p` as the working answer. Open at
  most the top 1–3 candidates, or `candidates[].wiki`/`cache`, only to verify.
- `grep`/`rg`/`ls`/`find` are permitted **only** as a last resort, and **only
  after** Jikji returned an empty or clearly-wrong candidate list.
- If you catch yourself about to run a raw search command before calling Jikji,
  stop and call `jikji brief`/`jikji search` instead.

## Safety contract

- Never move, rename, delete, or reorganize source files.
- Never scan all drives by default; require or infer a bounded explicit root.
- Treat `.jikji/doc_text/` as sensitive because it may contain extracted document
  text.
- Do not commit `.jikji/` or `.jikji_agent_map.md` unless the user explicitly
  wants generated artifacts tracked.
- Open original files only for final verification after Jikji has returned
  likely paths.

## Fast agent protocol

Default to the smallest deterministic command that answers the task:
For a single file path, default to `find`:

```bash
jikji find /explicit/root "natural language file clue" --first
```

`find` performs a cheap source-tree freshness check before searching. If files
were added/deleted/renamed since prepare, it refreshes Jikji artifacts in the
foreground and then returns the path; do not manually crawl first to compensate
for possible staleness.

```bash
jikji brief /explicit/root "natural language file clue" --top-k 10 --compact --json
```

Use `search` when you only need ranked candidates:

```bash
jikji search /explicit/root "natural language file clue" --top-k 10 --json
```

Interpretation:

- `candidates[].p` is the relative path to return or inspect.
- `ev`, `terms`, and `intents` explain why the candidate was ranked.
- `wiki` points to a compact LLM Wiki source page; `cache` points to parser text when available.
- Preserve paths exactly as returned.

Use non-compact `brief` only when the compact graph route is insufficient.

## Direct handoff rule

If the candidate list is plausible, do **not** perform a new broad `find`, `ls`,
`rg`, or manual filesystem crawl. Use Jikji's ranked paths directly and verify
only the top candidates when needed.

This is the intended speed benefit: Jikji has already done the repeated file-map
work before the agent receives the task.

## Prepare/refresh/admin commands

`brief` and `search` can auto-prepare a missing explicit root. Use these admin
commands when the user asks for setup, refresh, diagnostics, or cleanup:

```bash
jikji prepare /explicit/root --json
jikji refresh /explicit/root --json
jikji doctor /explicit/root --json
jikji map /explicit/root
jikji clean /explicit/root --dry-run --json
jikji clean /explicit/root --json
```

## Human GUI handoff

Jikji remains a CLI/agent skill. If the user asks to see or manage the Jikji state visually, start the loopback dashboard in the background and send the URL:

```bash
jikji gui /explicit/root --background --json
```

Return the JSON `url` as the clickable local link. The dashboard shows prepare status, LLM Wiki/knowledge graph counts, artifact presence, refresh/root-switch controls, and optional search/open/download actions.

## Fallback route

Only when compact `brief`/`search` is empty or clearly wrong:

```bash
cat /explicit/root/.jikji_agent_map.md
cat /explicit/root/.jikji/wiki/index.md
rg "keyword" /explicit/root/.jikji/graph_routes.jsonl
cat /explicit/root/.jikji/agent_routes.md
rg "keyword" /explicit/root/.jikji/*.jsonl
rg "keyword" /explicit/root/.jikji/doc_text
rg "keyword" /explicit/root --glob '!**/.jikji/**'
```

Use parser-extracted `.jikji/doc_text/` for PDF/HWP/HWPX/Office document bodies.
Search native text-like files in original locations as a final fallback.

## Evaluation

To test whether Jikji helps on a root:

```bash
jikji eval-generate /explicit/root --cases 80 --json
jikji eval /explicit/root --json
```

For actual-agent comparison with Hermes:

```bash
jikji hermes-skill-install --json
jikji hermes-bench /benchmark/root \
  --eval-set /external/eval.jsonl \
  --modes raw,jikji-fast,jikji-direct \
  --candidate-top-k 10 --skills jikji --json
```

`raw` means no Jikji. `jikji-fast` gives Hermes a compact map-first handoff.
`jikji-direct` measures the tool/skill behavior where the agent accepts Jikji's
ranked candidates without an extra exploratory chat turn.
