---
name: jikji
description: Prepare an explicit local folder for agent search with Jikji, then search generated indexes and document text caches without moving user files.
---

# Jikji Local Search Skill

Use this skill when you need to discover files or document text inside a local
folder the user has explicitly provided.

## Safety

- Never move, rename, delete, or reorganize source files.
- Never scan all drives by default; require an explicit root path.
- Treat `.jikji/doc_text/` as sensitive because it may contain extracted document
  text.

## Workflow

Fast path for finding files:

```bash
jikji search /explicit/root "natural language file clue" --top-k 10 --json
```

Use the returned candidate paths first. Do not manually grep large `.jikji`
JSONL files unless `jikji search` returns no useful candidate.

Prepare or refresh the map only when the root has not been prepared or may be
stale:

```bash
jikji prepare /explicit/root --json
jikji doctor /explicit/root
jikji search /explicit/root "natural language file clue" --top-k 10 --json
```

Fallback: search generated indexes directly only when the fast path is not
enough:

```bash
rg "keyword" /explicit/root/.jikji/*.jsonl
jq 'select(.ext==".pdf")' /explicit/root/.jikji/document_index.jsonl
```

Search parser-required document text:

```bash
rg "keyword" /explicit/root/.jikji/doc_text
```

Search native text files in the original tree:

```bash
rg "keyword" /explicit/root --glob '!**/.jikji/**'
```

Open source files only through paths found in Jikji index rows.


## Evaluation

Use Jikji's local evaluator when you need to measure whether the current map and
indexes help find files. It creates only generated artifacts under `.jikji/eval/`.

```bash
jikji eval-generate /explicit/root --cases 80 --json
jikji eval /explicit/root --json
```

The generated cases cover filename lookup, lexical content lookup, semantic-style
natural-language descriptions, and file-description lookup. Metrics include
hit@1, hit@3, hit@5, MRR, and per-scenario breakdowns.


## HippoCamp raw-vs-Jikji benchmark

To compare raw filesystem lookup with Jikji-assisted lookup on a public
personal-file benchmark:

```bash
jikji hippocamp-fetch ./benchmarks/hippocamp --profile Adam --split Subset --json
jikji prepare ./benchmarks/hippocamp/Adam_Subset --json
jikji hippocamp-import ./benchmarks/hippocamp/Adam_Subset \
  --annotation ./benchmarks/hippocamp/Adam_Subset.annotation.json \
  --json
jikji bench-run ./benchmarks/hippocamp/Adam_Subset \
  --eval-set ./benchmarks/hippocamp/Adam_Subset_hippocamp_eval_set.jsonl \
  --modes raw,jikji \
  --json
```

Use bounded download flags (`--max-files`, `--max-file-bytes`,
`--max-total-bytes`) before increasing corpus size.

Keep HippoCamp annotations/eval sets outside the benchmark root. For actual
Hermes runs, install the skill with `jikji hermes-skill-install` and pass an
external eval set to `jikji hermes-bench`.
