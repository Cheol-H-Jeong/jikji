# Hard mixed public-document benchmark

This benchmark is designed for Jikji's core purpose: helping local agents find
files in a messy local workspace without RAG, embeddings, or moving files.

It crawls public KOGL (공공누리) resource attachments, downloads a bounded mix of
real document files, splits them into train/valid/test, materializes deep
human-ish folder trees, and writes no-leak file-search eval sets outside the
corpus roots.

## What it builds

```bash
jikji hardbench-suite .benchmarks/hard_mixed_kogl_20260603_v3 \
  --target-docs 180 --max-data-idx 180 --cases 240 --top-k 10 --json
```

The 2026-06-03 v3 run downloaded 180 public documents:

```text
Ext    Files
-----  -----
.pdf     150
.hwp      27
.hwpx      1
.pptx      1
.xlsx      1
```

Split and eval size:

```text
Split  Docs  Cases
-----  ----  -----
train   108    216
valid    36     72
test     36     72
```

Eval scenarios:

```text
Scenario                  Meaning
------------------------  ---------------------------------------------
body_rare_phrase          Find by parsed HWP/PDF/etc. body phrase
format_doc_type_semantic  Find by format + natural document type + clues
messy_folder_context      Find by remembered deep folder/path context
multi_clue_hard           Find by format plus multiple body/name clues
```

The builder filters obvious parser-noise tokens such as garbled HWP syllable
runs or very long no-space English strings before generating queries.

## Train/valid-driven improvements

The initial run showed Jikji was strong on parsed document-body clues but weak on
folder-context queries because map-backed scoring over-weighted body evidence and
under-weighted path/folder structure.

Implemented from train/valid only:

1. Folder/path-context query detection.
2. Stronger structural path/folder scoring in map-backed search.
3. Discount for memo/link decoys when the user asks for an original document.
4. Stronger format-mismatch discount.
5. Benchmark query-quality filter for parser noise and Korean doc-type labels.
6. Search-index schema v2: include bounded parsed document text in the
   Everything-style SQLite map so exact body clues in PDF/HWP/etc. are available
   to instant search without opening original files.
7. Compact exact-term scoring for no-space Korean phrases and date-like clues,
   improving multi-clue document retrieval without embeddings or RAG.

## Final deterministic test result

```text
Mode   Cases  Hit@1   Hit@3   Hit@5   Hit@10  MRR     Sec    Sec/case
-----  -----  ------  ------  ------  ------  ------  -----  --------
raw       72  0.2222  0.4722  0.5694  0.6528  0.3656  0.782    0.0109
Jikji     72  0.8750  0.9861  1.0000  1.0000  0.9317  3.555    0.0494
```

Jikji is slower than the simple raw lexical diagnostic, but both are far below
1 second per case. Under the user's threshold, the important result is recall:
Jikji improved final test Hit@5 by +0.4306 absolute and reached Hit@5 1.0000.

Per-scenario Jikji test result:

```text
Scenario                  Cases  Hit@5   Hit@10  MRR
------------------------  -----  ------  ------  ------
body_rare_phrase             23  1.0000  1.0000  0.9565
format_doc_type_semantic     16  1.0000  1.0000  0.8906
messy_folder_context         15  1.0000  1.0000  0.9667
multi_clue_hard              18  1.0000  1.0000  0.9074
```

Remaining weakness: Hit@1 is not perfect when many public documents share
near-identical policy vocabulary, but every held-out test query now places the
target in the top 5 map/search candidates.

## Actual Hermes sample

A small 8-case actual-agent sample was run because full actual-agent loops take
minutes per handful of cases.

```text
Agent mode              Cases  Hit@1   Hit@3   Hit@5   Hit@10  Seconds  Avg sec/case
----------------------  -----  ------  ------  ------  ------  -------  ------------
raw Hermes                  8  0.8750  0.8750  0.8750  0.8750  570.060        71.257
Hermes + Jikji brief        8  1.0000  1.0000  1.0000  1.0000  330.405        41.301
Hermes + Jikji fast         8  0.8750  0.8750  1.0000  1.0000  155.444        19.430
```

Interpretation: `jikji-fast` is the closer product-mode comparison for
agent-equipped search. Hermes is still invoked, but it receives a tiny ranked
map/search handoff and is told not to browse/list/grep the filesystem. On this
sample it preserved Hit@5/Hit@10 1.0000 and reduced elapsed time by about 3.67x
versus raw Hermes. Actual-agent timings are model/workstation/run dependent; use
the full 72-case deterministic test as the main regression signal and bounded
actual-agent runs as product sanity checks.

For speed-focused actual-agent testing, use map-first mode:

```bash
jikji hermes-bench .benchmarks/hard_mixed_kogl_20260603_v3/corpus/test \
  --eval-set .benchmarks/hard_mixed_kogl_20260603_v3/eval/hardbench_test_eval.jsonl \
  --modes raw,jikji-fast --cases 8 --candidate-top-k 10 \
  --fast-max-turns 1 --skills jikji --yolo --json
```

Here `raw` is raw Hermes without Jikji. `jikji-fast` is Hermes with a prebuilt
Jikji map/search candidate handoff; Hermes should choose from the ranked paths
instead of spending turns on exploratory browsing.
