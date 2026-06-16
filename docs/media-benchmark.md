# Media OCR/ASR benchmark (image / audio / video)

This benchmark validates Jikji's media text-extraction layer: image OCR, speech
transcription, and on-screen video text. It answers a single question — **can a
local agent answer questions whose answers exist only inside media files?**

A raw filesystem agent has no local OCR or speech-to-text, so it cannot. Jikji
extracts that text during `prepare` (via optional CPU backends), so a
Jikji-assisted agent can.

## Corpus

A bounded synthetic corpus of **12 files** with opaque filenames
(`asset_NN`, `clip_NN`, `reel_NN`) so nothing leaks through the filename or
container metadata; every answer lives only in the rendered/spoken/on-screen
content:

| Type | Count | Content source | Extraction backend |
|---|---:|---|---|
| Image (`.png`) | 4 | text rendered with Pillow | RapidOCR (ONNXRuntime, CPU) |
| Audio (`.wav`) | 4 | speech synthesized with piper-tts | faster-whisper (CTranslate2, CPU INT8) |
| Video (`.mp4`) | 4 | 2 on-screen-text reels + 2 speech reels | RapidOCR keyframes / faster-whisper audio track |

The eval set (`.benchmarks/media_bench/media_eval_set.jsonl`) has 10 cases, one
per content fact, with `expected_paths` pointing at the single owning file.

## Backends

The OCR and transcription backends are **optional** and auto-detected at
runtime; default installs stay metadata-only. Install them with:

```bash
pip install "jikji[media]"   # rapidocr-onnxruntime + faster-whisper
```

Transcription is opt-in (`JIKJI_ENABLE_TRANSCRIPTION=1`) and video keyframe OCR
is opt-in (`JIKJI_ENABLE_VIDEO_OCR=1`) because they are expensive.

## Reproduce

```bash
# 1. Build the corpus (needs Pillow, piper-tts, ffmpeg)
python .benchmarks/media_bench/build_corpus.py

# 2. Index with media extraction enabled. Raise --parse-timeout because the
#    first model load is slow.
JIKJI_ENABLE_TRANSCRIPTION=1 JIKJI_ENABLE_VIDEO_OCR=1 \
  JIKJI_WHISPER_MODEL=base.en JIKJI_VIDEO_OCR_FRAMES=3 \
  JIKJI_VIDEO_OCR_INTERVAL_SECONDS=1 \
  jikji prepare .benchmarks/media_bench/corpus --parse-timeout 600

# 3. Real-agent raw-vs-Jikji benchmark
jikji hermes-bench .benchmarks/media_bench/corpus \
  --eval-set .benchmarks/media_bench/media_eval_set.jsonl \
  --modes raw,jikji-fast --cases 10 --max-turns 8 --fast-max-turns 1 \
  --candidate-top-k 5 --skills jikji \
  --provider openrouter --model google/gemini-2.5-flash \
  --out .benchmarks/media_bench/hermes_media.json --json
```

## Results

Actual Hermes agent, `google/gemini-2.5-flash`, 10 cases/mode:

| Agent mode | Hit@1 | Hit@3 | MRR | llm_calls | input (prompt) | output (completion) | total tokens |
|---|---:|---:|---:|---:|---:|---:|---:|
| raw Hermes | 0.00 | 0.00 | 0.000 | 31 | 192,868 | 29,232 | 222,100 |
| Hermes + Jikji | **1.00** | **1.00** | **1.000** | **10** | **91,217** | **7,181** | **98,398** |

Per-scenario Hit@1:

| Scenario | Cases | raw | Jikji |
|---|---:|---:|---:|
| image OCR | 4 | 0 | **4** |
| audio ASR | 3 | 0 | **3** |
| video on-screen OCR | 1 | 0 | **1** |
| video speech ASR | 2 | 0 | **2** |

- **Accuracy:** raw 0/10 → Jikji **10/10** (correct file ranked #1 in every case).
- **Tokens:** 222,100 → 98,398 (**−55.7%, 2.26x**).
- **LLM calls:** 31 → 10 (**3.1x fewer**).

## Interpretation and honesty limits

- The raw agent literally cannot read media content locally, so 0/10 is the
  expected floor, not a tuned-down baseline. The value of the benchmark is
  showing that Jikji's `prepare` step makes media searchable at all.
- The corpus is **synthetic and small** (12 files / 10 cases). It is a capability
  and regression check for the OCR/ASR pipeline, not a difficulty benchmark.
- Transcription accuracy depends on the whisper model size (`base.en` here);
  codenames and exact numbers may be approximated, so eval queries rely on the
  high-signal content nouns that survive transcription.
- Machine-readable report: `.benchmarks/media_bench/hermes_media_report.json`.
