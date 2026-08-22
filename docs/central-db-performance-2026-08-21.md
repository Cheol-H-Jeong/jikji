# Central DB Upgrade Performance Check — 2026-08-21

## Environment

- Linux x86_64, AMD Ryzen AI MAX+ 395
- Rust release binaries (`cargo build --release -p jikji-cli`)
- Baseline: `1f40919` (pre-central-DB Rust)
- Candidate: current central-SQLite/deep-index/stale-refresh implementation
- Method: same generated corpora, isolated `JIKJI_DATA_DIR`, median wall time

## Acceptance thresholds

- 2,000-file cold `prepare`: no more than 35% slower; must remain below 1 second.
- warm `find`: no more than 20% slower; must remain below 100 ms.
- explicit `deep-index` on the 240-file mixed corpus: below 250 ms without configured OCR/ASR engines.
- stale search response plus background-refresh launch: below 50 ms median and below 50 ms added median latency.

## Results

| Path | Baseline median | Candidate median | Delta | Verdict |
| --- | ---: | ---: | ---: | --- |
| cold `prepare`, 2,000 files | 373.8 ms | 465.0 ms | +24.4% | PASS |
| warm `find`, 240 files | 57.6 ms | 63.1 ms | +9.5% | PASS |
| `deep-index`, 240 files | n/a | 75.9 ms | n/a | PASS |
| ready `search`, refresh disabled | n/a | 9.9 ms | n/a | PASS |
| stale `search`, refresh requested | n/a | 47.8 ms | +36.6 ms | PASS |

Raw samples are stored in the JSON output of the reproduction script. The same
script was run twice after the SQLite `synchronous=NORMAL` optimization; the
second run gave `prepare +23.7%`, `find +1.6%`, `deep-index 74.9 ms`, and stale
refresh added `36.6 ms`. The former fixed-connection overhead is measurable,
but all paths remain below the stated absolute latency budgets.

## Comparison basis

These are the same operation classes used by the existing parity benchmark
(`prepare`, `search`, `find`) documented in `docs/ci-parity-benchmark-2026-06-29.md`;
the new rows add the upgrade-specific `deep-index` and stale-while-refresh paths.
The baseline is the pre-central-DB Rust release at `1f40919`; the candidate is
the current release. Both were built with `cargo build --release -p jikji-cli`
on Linux x86_64 / AMD Ryzen AI MAX+ 395, using fresh `JIKJI_DATA_DIR` values.

## Reproduction

```bash
git worktree add --detach /tmp/jikji-benchmark-baseline 1f40919
(cd /tmp/jikji-benchmark-baseline && cargo build --release -p jikji-cli)
cargo build --release -p jikji-cli
python3 tools/benchmark/compare_central_db.py \
  --baseline /tmp/jikji-benchmark-baseline/target/release/jikji \
  --candidate target/release/jikji \
  --out /tmp/jikji-benchmark.json
```

The script generates a deterministic 240-file corpus and a 2,000-file corpus,
then runs 5 cold prepares, 15 finds, 5 deep-index operations, and 15 ready plus
15 stale searches. It prints raw samples and medians.

## Correctness gates run with the benchmark

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace` — 90 passed
- `cargo build --release -p jikji-cli`
- `PYTHONPATH=python/jikji/src .venv/bin/pytest python/jikji/tests tests/parity -q` — 141 passed
- `cargo test -p jikji-cli --test upgrade_requirements_e2e` — 1 passed

Conclusion: no blocking performance regression. The largest measured change is
2,000-file cold `prepare` at +24.4%, within the 35% budget; search remains below
100 ms and stale refresh remains below 50 ms median / added latency budget.
