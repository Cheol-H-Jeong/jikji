---
name: jikji
description: Use Jikji first only for bounded local file/folder discovery with an unknown target path or a corpus-content search; explicit Jikji administration is also supported.
---

# Jikji Local File Discovery Skill

## Scope Gate: Jikji Find First

Before calling Jikji, require ALL of these conditions:

1. The task has local file/folder discovery intent, including locating filenames,
   metadata, or documents by their contents.
2. The target path is unknown, or the task requires a corpus-content search to
   identify matching local files or documents.
3. An explicit or reasonably inferred bounded root is available.

A root's existence alone, or the words "file" or "search" alone, is not enough.
Do not use keyword matching to block legitimate discovery queries. Select this
skill for the intent of the discovery substep, not every task in a repository.

Do not gate general QA, explanations, coding advice, web search, known-path
reads or edits, file stats, process/git/build/test/package commands, directory
listings (`ls`), or LSP symbol navigation behind Jikji. A targeted text lookup
inside a known file is not corpus discovery. In a mixed task, apply this gate
only to the discovery substep; later reads, edits, and tests proceed normally.
Explicit Jikji admin requests are allowed separately and need no preliminary find.

Examples:

- Use Jikji: "Find last year's budget spreadsheet in ~/Documents."
- Use Jikji: "Which documents under /work/contracts mention renewal?"
- Mixed task: "Find the report and summarize it" uses Jikji to locate the
  report, then reads the identified report to summarize it.
- No Jikji: "Read /work/docs/report.pdf" or "Edit src/main.rs."
- No Jikji: "Explain Rust ownership", "Search the web for Rust docs",
  "Run cargo test", "Show git status", "Stat report.pdf", "List /work",
  "Install a package", or "Go to this symbol's definition with LSP."
- Explicit admin: "Refresh Jikji's index for /work/docs" runs refresh directly.

## Discovery Protocol

When the scope gate holds, the first discovery call MUST be:

```bash
jikji find /explicit/root "natural language file clue" --json
```

Do not begin that discovery with raw `grep`, `rg`, `glob`, `find`, `fd`, `ls`,
`cat`, or `tree`. This is not a general ban on those tools.

`jikji find` searches existing indexes only. Inspect `index_status`,
`handoff_action`, `answerability`, `retry_proof`, and `tool_call_policy`.

If the index is missing, stale, unavailable, or the result is empty/clearly
wrong, run exactly one sharper Jikji retry using `next_commands[]` (or an
alternate query), with `--after-jikji-retry --retry-proof RETRY_PROOF` when
supported. Never run a second Jikji retry. Raw fallback is allowed only when
that retry yields `handoff_action=raw_fallback_after_retry` and permits it.

Interpret the JSON contract within this discovery request only:

- `answer_paths[]` is the primary ordered answer list; `paths[]` is the public
  path list. `candidates[]` is the merged top-k slate.
- `evidence_pack[].next_read` and `candidates[].next_read` identify bounded
  verification targets: `cache`, `wiki`, `original`, or `none`.
- `handoff_action=direct_use` or `answerability=answerable_from_payload` means
  accept the payload and do not repeat discovery with raw tools.
- `handoff_action=jikji_retry` means run the one sharper discovery retry.
- `index_status` values such as `missing`, `stale`, or `failure` are recovery
  signals, not permission for immediate raw discovery.
- `max_jikji_retries` is `1` before fallback; `max_raw_fallback_commands` is
  the fallback command ceiling. `raw_fallback_allowed=false` prohibits raw
  discovery fallback, not subsequent reading, editing, or testing.
- If `agent_should_not_rerank` is true, preserve Jikji's order.

## Stop Rule: Stop Discovery, Not The Task

Retry proof and stop restrictions apply only to the discovery substep.
When `tool_call_policy.stop_after_find` is true, do not make another discovery
call to re-find or re-rank sufficient results. Interpret
`tool_call_policy.forbidden_tools` only as restrictions on repeated discovery,
not blanket tool bans. Within discovery, use `allowed_followups` for bounded
verification of the top path or return `answer_paths[]`/`paths[]` as-is.

Known-path reads, edits, tests, and other requested work remain allowed after
successful discovery, even if their tools appear in `forbidden_tools`.
Another discovery call requires `stop_after_find=false` and an explicit
`jikji_retry` or `raw_fallback_after_retry` handoff.

For a single path-only answer, `--first` is optional; the default protocol is
`jikji find ROOT "query" --json`.

## Availability And Authorized Setup

If Jikji is missing, report that limitation; do not automatically clone, pull,
or install it. Installation requires explicit user authorization, including
any installation outside the repository. If no command can run, report that
no retry proof is available rather than pretending the retry succeeded.

If the root is not prepared, run `jikji prepare /explicit/root --json` only
when the user authorizes setup. Discovery itself does not authorize indexing
new locations or changing the environment.

## Safety Contract

- Never move, rename, delete, or reorganize source files as part of discovery.
  This does not block a separately requested edit of an identified file.
- Never scan all drives by default; require or infer a bounded root.
- Treat `.jikji/doc_text/` as sensitive extracted document text.
- Do not commit `.jikji/` or `.jikji_agent_map.md` unless explicitly requested.
- Original files may be read directly when their paths are already known,
  including for the requested work after discovery.

## Admin Commands

For explicit setup, refresh, diagnostics, or cleanup requests, without a find:

```bash
jikji prepare /explicit/root --json
jikji refresh /explicit/root --json
jikji doctor /explicit/root --json
jikji map /explicit/root
jikji clean /explicit/root --dry-run --json
jikji clean /explicit/root --json
```

For an explicit visual management request:

```bash
jikji gui /explicit/root --background --json
```

Return the JSON `url`. The dashboard provides status, refresh/root-switch
controls, and optional search/open/download actions.

## Bounded Discovery Fallback

Only after the discovery JSON contract permits raw fallback, use its bounded
command allowance. Prefer `.jikji_agent_map.md`, `.jikji/wiki/index.md`,
`.jikji/graph_routes.jsonl`, and parser-extracted `.jikji/doc_text/` evidence
before searching original document bodies. Search native text-like files in
original locations as the final discovery fallback. These restrictions do not
apply to normal known-path reads or edits.

For explicitly authorized media OCR/ASR or archive-member body indexing:

```bash
jikji deep-index /explicit/root --json
```

Default `prepare` indexes document and code contents; media and archives remain
metadata/name searchable. `deep-index` enables configured `JIKJI_OCR_ENGINE`
or `JIKJI_ASR_ENGINE` and bounded safe archive parsing incrementally.

## Evaluation

Compare the same agent with and without Jikji for actual local discovery tasks.
Label the Jikji side `Jikji find`.
