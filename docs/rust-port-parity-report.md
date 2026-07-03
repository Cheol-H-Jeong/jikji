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

## Rationale

The Python implementation remains the reference for public CLI JSON envelopes,
ranking order, required artifact presence, non-empty parser cache generation,
and documented generated artifact schemas. Implementation-specific Markdown
wording, validated generated JSON prose, exact parser cache text bytes, and wiki
source slug hash suffixes are recorded as intentional non-parity only after the
contract checks pass because they are not user-facing contracts in
`docs/schema.md` or `AGENTS.md`.
