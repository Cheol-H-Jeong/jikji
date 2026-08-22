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
- warm `search`: absolute median below 25 ms; relative delta is reported but fixed process/DB startup dominates at this scale.
- explicit `deep-index` on the 240-file mixed corpus: below 250 ms without configured OCR/ASR engines.
- stale search response plus background-refresh launch: below 50 ms median and below 50 ms added median latency.

## Results

| Path | Baseline median | Candidate median | Delta | Comparison | Verdict |
| --- | ---: | ---: | ---: | --- | --- |
| cold `prepare`, 2,000 files | 408.4 ms | 474.6 ms | +16.2% | pre-central-DB `1f40919` | PASS |
| warm `find`, 240 files | 57.0 ms | 65.2 ms | +14.5% | pre-central-DB `1f40919` | PASS |
| warm `search`, 240 files | 8.2 ms | 11.0 ms | +33.7% | pre-central-DB `1f40919` | PASS: 11 ms absolute |
| `deep-index`, 240 files | n/a | 75.0 ms | n/a | no pre-upgrade command; absolute budget 250 ms | PASS |
| ready `search`, refresh disabled | n/a | 11.4 ms | n/a | candidate internal control | PASS |
| stale `search`, refresh requested | n/a | 19.0 ms | +7.6 ms | no pre-upgrade stale-refresh path; compare ready control | PASS |

The complete raw sample arrays for this final run are committed at
`docs/central-db-performance-raw-2026-08-21.json`. `deep-index` and stale
refresh did not exist in baseline `1f40919`, so they have no historical baseline:
`deep-index` is judged against the 250 ms absolute budget, while stale refresh is
judged against the candidate ready-search control and 50 ms response/added-latency budgets.

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
then runs 5 cold prepares, 15 finds, 15 baseline/candidate searches, 5 deep-index
operations, and 15 ready plus 15 stale searches. It prints every raw sample and median.

## Correctness gates run with the benchmark

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace` — 90 passed
- `cargo build --release -p jikji-cli`
- `PYTHONPATH=python/jikji/src .venv/bin/pytest python/jikji/tests tests/parity -q` — 141 passed
- `cargo test -p jikji-cli --test upgrade_requirements_e2e` — 1 passed

Conclusion: no blocking performance regression. `prepare` and `find` remain
within their relative budgets; `search` is +33.7% but only 11.0 ms absolute,
and the new `deep-index`/stale-refresh paths remain inside their absolute budgets.
