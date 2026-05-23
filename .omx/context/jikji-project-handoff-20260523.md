# Jikji handoff — 2026-05-23

## Decision

Jikji is now a separate project for non-destructive local file/document knowledge maps for agents. Folder1004 is not the agent-indexing product; it remains a Windows/Linux GUI file organizer that physically moves messy files into organized folders.

## Current implementation

- Python package: `src/jikji`
- CLI entrypoint: `jikji`
- Commands: `prepare`, `refresh`, `map`, `doctor`
- Generated artifacts: `.jikji/` and `000_JIKJI_AGENT_MAP.md`
- No user file moves/renames/deletes.
- Parser-required documents can have text cached under `.jikji/doc_text` so agents can use `rg` without reparsing each time.

## Validation evidence at split

- `ruff check src tests`: passed
- `pytest -q`: passed (`2 passed` at creation time)
- `python -m compileall -q src tests`: passed
- CLI smoke: `jikji prepare <tmp> --json` generated `.jikji`, root map, and document text cache.

## Folder1004 boundary

Do not reintroduce Jikji behavior into Folder1004. If code is shared later, use a shared library or explicit dependency, not a default Folder1004 mode.
