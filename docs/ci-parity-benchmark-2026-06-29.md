# Rust Port CI, Parity, and Benchmark Check (2026-06-29)

## Windows CI

Remote CI failure on `windows-latest` was traced to `clippy -D warnings`.
The Unix-only symlink safety test compiled out on Windows, leaving its imports
unused. The CI smoke step also used Bash-specific temp-file commands.

Fixes:

- Move Unix-only imports into the `#[cfg(unix)]` test body.
- Run the cross-OS CLI smoke check with PowerShell, which is available on all
  GitHub-hosted runners.

## Python/Rust Parity

Command:

```bash
python3 tools/parity/run_rust_vs_python.py \
  --python-repo <repo> \
  --rust-bin target/release/jikji \
  --fixtures tests/golden \
  --out .omo/evidence/rust-port-workplan/task-08-parity-benchmark.txt
```

Result: `PASS`, with no contract failures.

Wall-clock timing summary from this run:

```text
operation  python seconds  rust seconds
---------  --------------  ------------
prepare          0.106958      0.007567
search           0.096402      0.004280
find             0.095795      0.005046
```

Intentional non-parity remains limited to generated prose/cache bytes and
implementation-specific wiki slug hashes after schema, artifact, JSON contract,
and ranking checks pass.

## HippoCamp Victoria Subset

Dataset source:

```text
<hippocamp-victoria-subset>
```

The initial benchmark copied `Victoria_Subset` to `/tmp/jikji-hippo-victoria-rust`,
generated Rust eval JSONL files from `Victoria_Subset.json`, and ran local
deterministic `raw,jikji` retrieval with no network or LLM calls. A follow-up
audit on 2026-06-30 found that this Rust smoke path used a separate Rust
evaluator and collapsed HippoCamp's multi-path answers into single-target cases,
so the original recall table below is not a valid parity claim.

Prepare:

```text
files        137
folders       37
docs_parsed  111
real time   3.91s
```

Original first-target QA metric, retained as invalidated provenance:

```text
mode   cases  Hit@1   Hit@5   Hit@10  MRR
-----  -----  ------  ------  ------  ------
raw       11  0.0000  0.0000  0.0000  0.0000
Jikji     11  0.0000  0.3636  0.5455  0.1621
```

Original all target-file pairs metric, retained as invalidated provenance:

```text
mode   cases  Hit@1   Hit@5   Hit@10  MRR
-----  -----  ------  ------  ------  ------
raw      140  0.0071  0.0357  0.0714  0.0209
Jikji    140  0.0429  0.1857  0.2786  0.1038
```

Corrected audit result using one evaluator:

```text
under test     evaluator                         cases  Hit@1   Hit@5   Hit@10  MRR
-------------  --------------------------------  -----  ------  ------  ------  ------
Python Jikji   python_jikji.hippocamp.run_benchmark  11  0.7273  0.9091  1.0000  0.8056
Rust Jikji     python_jikji.hippocamp.run_benchmark  11  0.7273  0.9091  1.0000  0.8061
```

Command:

```bash
python3 tools/parity/compare_victoria_python_eval.py \
  --python-repo <repo> \
  --rust-bin target/release/jikji \
  --dataset <hippocamp-victoria-subset>/Victoria_Subset \
  --annotation <hippocamp-victoria-subset>/Victoria_Subset.json \
  --out docs/victoria-python-rust-eval-report.json
```

Interpretation: eval code is now identical for both implementations. Rust Jikji
now preserves Hit@1, Hit@5, Hit@10, and MRR on this Victoria HippoCamp subset,
including `hippocamp_profiling` cases. This is not the full Hermes-agent
benchmark; the existing fullset historical report remains
`docs/hippocamp-rerun-report.md`.
