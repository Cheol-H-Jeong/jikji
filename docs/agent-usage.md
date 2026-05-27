# Using Jikji from a Local Agent

Install from a checkout:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .
```

Agent flow:

1. Start with the tool-first command. It auto-prepares an explicit root when
   the instant index is missing:

```bash
.venv/bin/jikji search /path/to/folder "natural language file clue" --top-k 10 --json
```

2. If the result is empty or clearly insufficient, inspect the route guides and
   indexes:

```bash
cat /path/to/folder/000_JIKJI_AGENT_MAP.md
cat /path/to/folder/.jikji/agent_routes.md
rg "keyword" /path/to/folder/.jikji/*.jsonl
```

3. Search parser-extracted document text only when needed:

```bash
rg "keyword" /path/to/folder/.jikji/doc_text
```

4. Search native text files in the original tree as a final fallback:

```bash
rg "keyword" /path/to/folder --glob '!**/.jikji/**'
```

Do not move, rename, delete, or reorganize source files while using Jikji for
search.

Recommended `.gitignore` for indexed roots that are also Git repositories:

```gitignore
.jikji/
000_JIKJI_AGENT_MAP.md
```


## Evaluate search quality

After preparing a root, generate and run a local search eval set:

```bash
jikji eval-generate /path/to/folder --cases 80 --json
jikji eval /path/to/folder --json
```

Use `.jikji/eval/eval_report.json` to compare hit@k and MRR before/after map or
indexing changes.
