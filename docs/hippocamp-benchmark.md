# HippoCamp Benchmark Adapter

HippoCamp is the first recommended external benchmark for Jikji because it is a
public personal-file-system benchmark with per-profile folders and QA annotations.

Jikji supports a bounded workflow so large media files are not downloaded by
accident:

```bash
jikji hippocamp-fetch ./benchmarks/hippocamp \
  --profile Adam \
  --split Subset \
  --max-files 120 \
  --max-file-bytes 10485760 \
  --max-total-bytes 262144000 \
  --json
```

The command downloads a bounded subset from Hugging Face into an inner root such
as:

```text
./benchmarks/hippocamp/Adam_Subset
```

The annotation file is stored outside that inner root, for example:

```text
./benchmarks/hippocamp/Adam_Subset.annotation.json
```

Keep annotations and eval sets outside the benchmark root. Jikji refuses
no-leak benchmarks when answer files such as `*_Subset.json`,
`*.annotation.json`, or `*_eval_set.jsonl` are visible inside the root.

Then prepare Jikji and import HippoCamp's QA annotations:

```bash
jikji prepare ./benchmarks/hippocamp/Adam_Subset --json
jikji hippocamp-import ./benchmarks/hippocamp/Adam_Subset \
  --annotation ./benchmarks/hippocamp/Adam_Subset.annotation.json \
  --cases 200 \
  --json
```

Run the raw-vs-Jikji benchmark:

```bash
jikji bench-run ./benchmarks/hippocamp/Adam_Subset \
  --eval-set ./benchmarks/hippocamp/Adam_Subset_hippocamp_eval_set.jsonl \
  --modes raw,jikji \
  --json
```

Outputs:

```text
../Adam_Subset_hippocamp_eval_set.jsonl
../hippocamp_import_report.json
.jikji/eval/hippocamp_benchmark_report.json
```

Modes:

- `raw`: scans the original filesystem, file names/paths, and native text-like
  files only. It intentionally does not use Jikji parser caches.
- `jikji`: searches Jikji indexes, parser text caches, and native text files
  through the Jikji evaluator.

Metrics:

- hit@1
- hit@3
- hit@5
- MRR
- seconds
- per-scenario breakdown

To verify benchmark stability after a scoring/index change:

```bash
jikji bench-iterate ./benchmarks/hippocamp/Adam_Subset \
  --eval-set ./benchmarks/hippocamp/Adam_Subset_hippocamp_eval_set.jsonl \
  --iterations 20 \
  --json
```

For a real Hermes agent run, ensure `.jikji/eval/` is not present in the root,
then use the external eval set:

```bash
jikji hermes-skill-install --json
jikji hermes-bench ./benchmarks/hippocamp/Adam_Subset \
  --eval-set ./benchmarks/hippocamp/Adam_Subset_hippocamp_eval_set.jsonl \
  --modes raw,jikji \
  --candidate-top-k 10 \
  --skills jikji \
  --json
```

`jikji` mode is tool-first: the benchmark injects the output of `jikji search`
as ranked candidates and tells Hermes not to browse the filesystem unless the
candidate list is empty or irrelevant. Use `jikji-passive` for the older
map-reading behavior.
