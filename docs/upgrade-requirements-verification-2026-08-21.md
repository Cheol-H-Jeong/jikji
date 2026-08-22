# Jikji Upgrade Requirements Verification — 2026-08-21

## Plan

1. One isolated end-to-end CLI test owns a temporary root and `JIKJI_DATA_DIR`.
2. Verify central SQLite/no root sidecar, default cheap indexing, explicit deep indexing, stale-while-refresh, and agent retry/fallback in one lifecycle.
3. Run clippy, the full Rust workspace, release build, Python parity tests, and diff checks.

## Executable proof

The permanent test is:

```bash
cargo test -p jikji-cli --test upgrade_requirements_e2e -- --nocapture
```

Output:

```text
cargo test: 1 passed (1 suite, 0.45s)
```

The test executes these public CLI paths against one folder:

```text
jikji prepare ROOT --no-agent-rules --json
jikji find ROOT image.png|audio.wav|video.mp4|bundle.zip --no-background-refresh --json
jikji find ROOT configured-*-body-token --no-background-refresh --json
JIKJI_OCR_ENGINE=ENGINE JIKJI_ASR_ENGINE=ENGINE jikji deep-index ROOT --no-agent-rules --json
jikji search ROOT document-visible-token-771 --json --no-background-refresh
jikji search ROOT document-visible-token-771 --stale-after-seconds 0 --json
jikji find MISSING_ROOT missing-token --json
jikji find MISSING_ROOT missing-token --after-jikji-retry --retry-proof PROOF --json
jikji skill-export --json
```

Assertions:

- `JIKJI_DATA_DIR/jikji/index.sqlite` exists; `ROOT/.jikji` does not exist.
- Image/audio/video/archive filenames are searchable after default `prepare`.
- Their configured body tokens are absent from candidate evidence after default `prepare`.
- The same four body tokens are present and rank their source files after folder-scoped `deep-index`.
- Default TTL search reports `ready` and does not start refresh.
- `--stale-after-seconds 0` reports `stale_using_previous_index`, starts background refresh, and returns within 2 seconds rather than waiting for refresh completion.
- A missing DB returns `jikji_retry`, one retry, and no raw fallback; the proof-verified retry returns `raw_fallback_after_retry` with at most two raw commands.
- Exported skill text contains `Jikji Find First`, exactly-one retry language, and `deep-index` instructions.

## Full quality gate

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings    PASS
cargo test --workspace                                                  90 passed / 36 suites
cargo build --release -p jikji-cli                                      PASS
PYTHONPATH=python/jikji/src .venv/bin/pytest python/jikji/tests tests/parity -q
                                                                          141 passed
git diff --check                                                       PASS
```
