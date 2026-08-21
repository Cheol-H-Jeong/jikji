# Rust Port Parity Report

Generated for Task 8 of `.omo/plans/rust-port-workplan.md`.

Evidence file: `.omo/evidence/rust-port-workplan/task-08-parity-benchmark.txt`

## Scope

The final parity harness compares the Python reference at
`<repo>` with the Rust release binary
`target/release/jikji` on:

- checked-in golden fixture scenarios under `tests/golden`
- one generated temporary corpus
- `prepare`, `search`, and `find` wall-clock timings
- shared Python evaluator benchmark path for Python-vs-Rust recall checks
- mutation failure proof for a changed golden JSON candidate order/key

## Result

The final parity harness result is **PASS**. Contract-sensitive CLI outcomes,
required generated artifact presence, required schema fields, search ranking,
find behavior, doctor behavior, clean JSON keys, clean safety, mutation failure
proof, and shared evaluator benchmark path checks all passed.

The run recorded wall-clock timings for `prepare`, `search`, and `find` only as
bounded local measurements from that invocation. They are not a claimed
performance guarantee or faster-than assertion.

Contract failures: none.

Generated artifact policy:

- Hard failures: missing required generated artifact classes, missing required
  artifact directories, malformed generated JSON/JSONL, missing documented
  fields, CLI JSON key differences, and search/find candidate-order differences.
- Hard failures: parser cache text files missing when Python generated them, or
  empty Rust parser cache text where Python generated non-empty cache text.
- Intentional non-parity: generated Markdown prose and validated generated
  JSON/JSONL prose may differ after required artifact presence, documented
  schema fields, and search/find behavior pass.
- Intentional non-parity: exact `.jikji/doc_text/sha256_*.txt` cache content may
  differ across parser implementations after required cache-file presence,
  non-empty text generation, documented JSON schemas, and search/ranking
  behavior pass.
- Intentional non-parity: `.jikji/wiki/sources/<stem>-<hash>.md` suffix hashes
  are implementation-specific, so parity compares semantic source stems and
  counts rather than exact filename hashes.

## Current ownership

The shipped Rust workspace is the primary implementation for prepare,
search/find/discover, agent installation, GUI routing, local evaluation,
Hermes execution/report comparison, value reports, BEIR/HippoCamp/EDiTh/
PublicData/WorkspaceBench/HardBench adapters, and native media metadata plus
configured Rust OCR/ASR engine execution. Rust CLI paths do not spawn Python.

The Python package remains a reference and parity oracle. Generated Markdown
prose, parser cache bytes, and wiki hash suffixes remain intentionally
implementation-specific under the parity policy above. Native media text
extraction requires an explicitly configured Rust-executed OCR/ASR engine;
without one, the safe default is metadata-only.

## Python source disposition

The installed/released `jikji` entry point is `crates/jikji-cli/src/main.rs`.
The Python tree is retained for reference behavior, golden/parity evaluation,
and legacy development only; the Rust binary does not import or execute it.

| Python file | Retained role | Rust replacement |
| --- | --- | --- |
| `__main__.py` | legacy/reference CLI | `jikji-cli` |
| `config.py`, `models.py`, `version.py` | reference data contracts | `jikji-core`, workspace package metadata |
| `scanner.py`, `metadata.py`, `agent_index.py`, `search_index.py` | reference prepare/index behavior | `jikji-index`, `jikji-search` |
| `discover.py`, `answer_pack.py`, `agent_brief.py`, `graph_query.py` | reference search/handoff contracts | `jikji-search` |
| `llm_wiki.py` | reference generated graph/wiki behavior | `jikji-index`, `jikji-search` graph modules |
| `gui.py` | reference GUI contract | `jikji-cli::gui_commands` |
| `agent_skill_install.py` | reference installer contract | `jikji-agent`, `jikji-cli::agent_commands` |
| `eval.py`, `holdout_eval.py`, `improvement_loop.py` | parity/evaluator oracle | `jikji-bench` |
| `hermes_bench.py`, `hermes_compare.py`, `hermes_answer_pack.py` | parity fixtures and historical report comparison | `jikji-hermes-bench`, `jikji-bench::hermes_compare` |
| `benchmark_value.py`, `benchmark_two_call.py` | historical value-report oracle | `jikji-bench::benchmark_value`, `benchmark_two_call` |
| `beir.py`, `hippocamp.py` | dataset parity/reference adapters | `jikji-public-datasets`, `jikji-cli` |
| `edith.py`, `publicdata_bench.py`, `workspacebench.py`, `hardbench.py` | public benchmark parity/reference adapters | `jikji-bench::public_sources`, `public_adapters`, `jikji-cli` |
| `__init__.py` | Python package namespace only | no runtime equivalent required |

`python/jikji/pyproject.toml` still exposes a separately installed legacy
Python console script for reference developers. Cargo installation and GitHub
release artifacts install the Rust `jikji` binary; installing the Python
reference package is an explicit alternative and can shadow any same-named
binary earlier on `PATH`.
