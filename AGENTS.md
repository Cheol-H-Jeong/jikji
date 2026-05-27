# Jikji project guide

Jikji is a separate project from Folder1004.

## Product identity

- Jikji prepares local folders for AI-agent discovery without moving, renaming, or deleting user files.
- It creates text-first knowledge maps and parser caches under `.jikji/` plus a root `000_JIKJI_AGENT_MAP.md`.
- It is meant to be used by local agents such as Hermes/Codex through CLI commands or skills.
- Folder1004 remains the GUI product for physically organizing messy Desktop/Downloads-style folders.

## Safety boundary

- Default and expected behavior is non-destructive.
- Never reorganize user folders in this repo unless a future feature explicitly adds a separate, warned physical mode.
- `.jikji/` and `000_JIKJI_AGENT_MAP.md` are generated artifacts and may be regenerated.


## Generated artifact ownership

Jikji may regenerate only its documented artifacts: `.jikji/manifest.json`, `.jikji/*.jsonl`, `.jikji/search_index.sqlite`, `.jikji/agent_map.md`, `.jikji/agent_routes.md`, `.jikji/agent_skill_context.md`, `.jikji/human_guide.md`, `.jikji/doc_text/`, `.jikji/doc_meta/`, `.jikji/eval/`, `.jikji/.lock`, and root `000_JIKJI_AGENT_MAP.md`. Retired generated artifacts listed in `manifest.json` under `retired_cleanup_paths` may be removed during prepare. Do not delete arbitrary user-created files under `.jikji/`.

Local-agent standard docs live in `docs/local-agent-search-standard.md`, `docs/schema.md`, and `docs/agent-usage.md`.

## Current commands

```bash
jikji prepare /path/to/folder   # create/update .jikji and root map
jikji refresh /path/to/folder   # alias for prepare
jikji map /path/to/folder       # print generated map
jikji doctor /path/to/folder    # verify expected artifacts
```

Local dev:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .
.venv/bin/pip install pytest ruff
.venv/bin/ruff check src tests
.venv/bin/pytest -q
.venv/bin/python -m compileall -q src tests
```

## Resume context

The first implementation was split out of Folder1004 on 2026-05-23. It currently includes scanner, metadata collection, parser registry, document text caching, JSONL indexes, and Markdown map generation. Next useful work: stabilize packaging, improve incremental refresh, add watcher/daemon optionally, formalize Hermes/Codex skill docs, and add larger corpus benchmarks.
