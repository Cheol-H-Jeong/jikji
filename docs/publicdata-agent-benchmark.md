# Korean public-data local-agent benchmark

This benchmark is designed for Jikji's product target: helping a local agent find
files and file contents in a plausible local folder tree without moving or
renaming the user's files.

## What it builds

- Downloads public Korean open-data XLSX files.
- Splits them into train/valid/test.
- Materializes each split into noisy but plausible folders such as
  `받은자료/기관별/새 폴더`, `공공데이터/임시보관/확인필요`, and
  `01_업무공유/정리전/엑셀 원본`.
- Adds light clutter note files.
- Generates scenario-based file retrieval cases:
  - vague filename memory
  - lexical content clue
  - semantic description from title/description
  - folder-context clue
  - column/value clue from spreadsheet text

## Source honesty

The user-facing goal was Public Data Portal / KOGL Type 1. On this workstation,
direct `data.go.kr` access was not used as an assumption because it can require
credentials/session handling. The implemented reproducible fallback downloads
public XLSX files from Seoul Data Hub and records that fact in `manifest.json`.

Therefore, describe this run as **Korean public-data XLSX** unless a future
source adapter verifies KOGL Type 1 per downloaded file.

## Reproduce

```bash
jikji publicdata-build .benchmarks/publicdata_agent_bench/run_20260529 \
  --target-docs 90 --max-id 700 --cases 40 --json

jikji publicdata-suite .benchmarks/publicdata_agent_bench/run_20260529 \
  --target-docs 90 --max-id 700 --cases 40 --top-k 10 --json

jikji hermes-bench .benchmarks/publicdata_agent_bench/run_20260529/corpus/test \
  --eval-set .benchmarks/publicdata_agent_bench/run_20260529/eval/publicdata_test_eval.jsonl \
  --modes raw,jikji --cases 18 --candidate-top-k 10 \
  --timeout 150 --max-turns 10 \
  --skills jikji --yolo --json
```

## 2026-05-29 corpus

```text
Item              Count
----------------  -----
downloaded docs      90
train docs           54
valid docs           18
test docs            18
test eval cases      18
```

## Actual Hermes agent result

`raw Hermes` searched the original folders/files and was instructed not to read
Jikji artifacts. `Hermes + Jikji` received the query-specific `jikji brief`
candidate route sheet and could inspect original files only if needed.

```text
Agent mode       Cases  Hit@1   Hit@3   Hit@5   Hit@10  Seconds  Avg sec/case
---------------  -----  ------  ------  ------  ------  -------  ------------
raw Hermes          18  0.7778  0.8333  0.8333  0.8333  784.028        43.557
Hermes + Jikji      18  0.9444  1.0000  1.0000  1.0000  522.894        29.050
```

Interpretation: on this test split, Jikji made the same local agent both more
accurate and faster: +0.1666 absolute Hit@5/Hit@10 and about 1.50x lower elapsed
time.

Timing and actual-agent results are workstation-, model-, and run-dependent.
The ignored `.benchmarks/` artifacts preserve local evidence for this run; the
deterministic diagnostic is the cheaper regression check.

## Deterministic search-layer diagnostic

This is secondary evidence only; it checks the map/index layer before paying for
actual agent runs.

```text
Mode                      Cases  Hit@1   Hit@3   Hit@5   Hit@10  MRR     Seconds
------------------------  -----  ------  ------  ------  ------  ------  -------
raw lexical diagnostic       18  0.7222  0.8889  0.8889  0.9444  0.8111    0.123
Jikji index diagnostic       18  0.9444  1.0000  1.0000  1.0000  0.9722    0.423
```

Train/valid diagnostics are kept separate from the test headline. They informed
the generic parser/query-clue improvements but should not be reported as final
test evidence.

```text
Split  Mode                      Cases  Hit@1   Hit@3   Hit@5   Hit@10  MRR     Seconds
-----  ------------------------  -----  ------  ------  ------  ------  ------  -------
train  raw lexical diagnostic       40  0.4000  0.8250  0.8750  0.9250  0.6102    0.821
train  Jikji index diagnostic       40  0.6500  0.8500  1.0000  1.0000  0.7783    2.640
valid  raw lexical diagnostic       18  0.6667  0.8889  0.8889  0.8889  0.7685    0.120
valid  Jikji index diagnostic       18  0.8333  1.0000  1.0000  1.0000  0.9167    0.431
```

## Train/valid-driven improvements

The train/valid cases exposed two generic issues:

1. Spreadsheet clues like `sheet`, `서울`, `통계`, `현황`, `합계`, and `총계`
   were too generic for realistic file finding.
2. The XLSX parser sampled too few rows/sheets, which could hide useful
   spreadsheet content clues from the generated map/index.

The implemented changes are intentionally generic where they touch Jikji core:
the XLSX parser samples more sheet/row text before indexing. Separately, this
benchmark builder filters generic public-data query stop terms so its synthetic
queries are more realistic for both raw and Jikji modes. Jikji core still uses
lexical/file-map indexing only; no embedding model, vector DB, or RAG layer is
added.
