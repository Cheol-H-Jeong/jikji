# Public benchmark catalog for Jikji

Jikji is a non-destructive file-map and route layer for local agents. The most
useful benchmarks are therefore file-level or filesystem-level tasks, not only
chunk retrieval tasks.

## Recommended order

| Benchmark | Best use | Caveat |
|---|---|---|
| Korean public-data messy-folder benchmark | Korean spreadsheet/document-name/content search with human-ish noisy folders and actual Hermes raw vs Jikji comparison | Current builder uses Seoul Data Hub public XLSX fallback when data.go.kr is unavailable; XLSX-heavy |
| HippoCamp | Personal-computer style file search and agent QA | Full dataset is large; some tasks evaluate final QA more than retrieval |
| EDiTh / Véracier Industries | Enterprise PDFs, scanned/searchable/mixed formats, multilingual, multi-file answers | Public archive is ~1.5GB and only a few answer-key questions are explicit file-list retrieval |
| MIRACL-VISION materialized docs | Multilingual file-level retrieval regression | Materialized as Markdown, so raw lexical search is strong |
| BEIR materialized docs | Wide deterministic IR regression | Materialized as Markdown; not a parser stress test |
| SDS KoPub VDR | Korean public PDF page-level retrieval | Corpus parquet is very large; needs page-to-file conversion |
| UniDoc-Bench | Large PDF page/QA stress test | Multimodal/page-centric; needs file-level conversion |
| docx-corpus | DOCX parser/indexing scale stress | No retrieval QA; needs synthetic/label-derived eval |

## Korean public-data messy-folder run

Jikji includes a public-data builder that downloads public XLSX files, splits
them into train/valid/test, places them into human-ish messy folders, and writes
scenario-based file-retrieval eval sets.

```bash
jikji publicdata-build .benchmarks/publicdata_agent_bench/run_20260529 \
  --target-docs 90 --max-id 700 --cases 40 --json
jikji publicdata-suite .benchmarks/publicdata_agent_bench/run_20260529 \
  --target-docs 90 --max-id 700 --cases 40 --top-k 10 --json
jikji hermes-bench .benchmarks/publicdata_agent_bench/run_20260529/corpus/test \
  --eval-set .benchmarks/publicdata_agent_bench/run_20260529/eval/publicdata_test_eval.jsonl \
  --modes raw,jikji --cases 18 --candidate-top-k 10 \
  --skills jikji --yolo --json
```

Source honesty: the requested source family was Public Data Portal / KOGL Type 1.
The reproducible builder records Seoul Data Hub public XLSX downloads as the
actual accessible fallback source in the manifest, so this benchmark should be
described as Korean public-data XLSX, not verified KOGL Type 1.

Actual Hermes agent comparison on the 18-case test split:

```text
Agent mode       Cases  Hit@1   Hit@3   Hit@5   Hit@10  Seconds  Avg sec/case
---------------  -----  ------  ------  ------  ------  -------  ------------
raw Hermes          18  0.7778  0.8333  0.8333  0.8333  784.028        43.557
Hermes + Jikji      18  0.9444  1.0000  1.0000  1.0000  522.894        29.050
```

Train/valid-driven changes from this run: XLSX parsing now samples more
sheets/rows so content clues are available before agent search. Separately, this
benchmark builder filters generic spreadsheet clues such as `sheet`, `서울`,
`통계`, and `현황` from generated content queries for both raw and Jikji modes.
Actual-agent timings are workstation-, model-, and run-dependent.

## EDiTh bounded run

Jikji includes an EDiTh adapter:

```bash
jikji edith-summary .benchmarks/edith_public --json
jikji edith-suite .benchmarks/edith_public_run \
  --cases 3 --max-docs 42 --top-k 10 \
  --max-download-bytes 2000000000 --json
```

The adapter downloads only metadata first. For `edith-suite`, it stream-extracts
only selected PDFs from the public archive instead of storing the 1.5GB tarball.
The stream is bounded by `--max-download-bytes` and `--no-docs` can be used for
metadata/eval-set inspection without running a corpus benchmark.

Actual Hermes agent on the same subset is the primary comparison:

| Agent mode | Cases | Hit@5 | Hit@10 | Seconds | Avg sec/case |
|---|---:|---:|---:|---:|---:|
| raw Hermes | 3 | 1.0000 | 1.0000 | 152.777 | 50.926 |
| Hermes + Jikji | 3 | 1.0000 | 1.0000 | 120.852 | 40.284 |

Most recent bounded deterministic diagnostic on this workstation. This is secondary evidence only.

| Dataset | Mode | Cases | Hit@1 | Hit@3 | Hit@5 | Hit@10 | SetR@5 | MRR |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| EDiTh PDF subset | raw lexical diagnostic | 3 | 0.3333 | 0.6667 | 0.6667 | 0.6667 | 0.4000 | 0.4444 |
| EDiTh PDF subset | Jikji index diagnostic | 3 | 0.3333 | 1.0000 | 1.0000 | 1.0000 | 0.6667 | 0.6667 |

Interpretation: EDiTh is more realistic than Markdown-only corpora for Jikji's
parser/cache/route-layer purpose, but the current bounded file-list subset is
small. Treat it as a smoke benchmark and use HippoCamp/full EDiTh variants for
stronger claims.
