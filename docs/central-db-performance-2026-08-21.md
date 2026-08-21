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
- explicit `deep-index` on the 243-file mixed corpus: below 250 ms without configured OCR/ASR engines.
- stale search response plus background-refresh launch: below 50 ms median and below 30 ms added median latency.

## Results

| Path | Baseline median | Candidate median | Delta | Verdict |
| --- | ---: | ---: | ---: | --- |
| cold `prepare`, 2,000 files | 381.5 ms | 492.6 ms | +29.1% | PASS |
| warm `find`, 243 files | 61.1 ms | 67.7 ms | +10.7% | PASS |
| `deep-index`, 243 files | n/a | 113.9 ms | n/a | PASS |
| ready `search`, refresh disabled | n/a | 11.0 ms | n/a | PASS |
| stale `search`, refresh requested | n/a | 32.5 ms | +21.5 ms | PASS |

Small-corpus cold `prepare` increased from roughly 42–47 ms to 103–106 ms because the central database has fixed connection/schema/transaction overhead. The user-visible absolute cost remains about 0.1 seconds; the representative 2,000-file corpus stays within the 35% regression budget. `find` and stale-while-refresh remain below interactive latency thresholds.

## Reproduction

```bash
# Baseline binary
git worktree add --detach /tmp/jikji-benchmark-baseline 1f40919
(cd /tmp/jikji-benchmark-baseline && cargo build --release -p jikji-cli)

# Candidate binary
cargo build --release -p jikji-cli

# Each timing run uses a fresh JIKJI_DATA_DIR and the same generated corpus.
# 7 cold prepare samples, 15 find/search samples, and 5 deep-index samples;
# the tables report medians.
```

## Correctness gates run with the benchmark

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace` — 89 passed
- `cargo build --release -p jikji-cli`
- `PYTHONPATH=python/jikji/src .venv/bin/pytest python/jikji/tests tests/parity -q` — 141 passed

Conclusion: no blocking performance regression under the stated thresholds. The central SQLite design adds a measurable fixed prepare cost, but search latency, large-corpus prepare, explicit deep indexing, and stale-while-refresh stay within budget.
