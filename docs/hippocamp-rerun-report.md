# HippoCamp Benchmark Re-run Report

This report captures the full benchmark re-run after the prompt **token-diet**
pass landed on top of the earlier ranking/skill improvements:

1. The visible root agent map is a hidden dotfile (`.jikji_agent_map.md`).
2. The agent skill (`SKILL.md`) forces a Jikji search-first protocol.
3. The Hermes agent benchmark tracks LLM call count and **separate input
   (prompt) / output (completion) token usage** per task.
4. The ranking core was upgraded (filename component tokenization, rarity-sharpened BM25).
5. **Token diet (new):** the Jikji→agent handoff is now a compact map-first
   pass — `candidate-top-k` lowered from 10 to **5**, a **single** evidence
   snippet per candidate hard-truncated to **120 chars**, and a bounded 1-turn
   map handoff instead of multi-turn browsing. This drops both the per-prompt
   payload and the number of LLM calls.

Profiles: **Adam, Bei, Victoria** (HippoCamp `*_Subset`, 49 deterministic cases;
10 real-agent cases per profile).

## 1. Deterministic suite (lexical scorer, no LLM)

Aggregate over 49 cases (`jikji` = Jikji-assisted scorer, `raw` = naive
filesystem lexical search).

| Metric  | raw    | jikji  |
|---------|--------|--------|
| Hit@1   | 0.5102 | **0.6327** |
| Hit@3   | 0.6530 | **0.7551** |
| Hit@5   | 0.6939 | **0.8163** |
| MRR     | 0.6004 | **0.7047** |

Per-profile (`jikji` mode, top_k=10):

| Profile  | cases | Hit@1 | Hit@3 | Hit@5 | Hit@10 |
|----------|-------|-------|-------|-------|--------|
| Adam     | 18    | 0.7778 | 0.8889 | 0.9444 | 0.9444 |
| Bei      | 21    | 0.5238 | 0.6190 | 0.6190 | 0.7619 |
| Victoria | 10    | 0.6000 | 0.8000 | 1.0000 | 1.0000 |

## 2. Hermes real-agent benchmark (token-diet, 10 cases/profile)

Model: `openai/gpt-4o-mini` via `openrouter`; **10 cases/profile** (30 total);
`raw` max-turns 8; `jikji` = token-diet map-first 1-turn handoff
(`--candidate-top-k 5`, single 120-char evidence snippet). `raw` = agent must
browse the corpus; `jikji` = agent receives the compact Jikji candidate list
first.

Input/output tokens are reported separately (prompt = input, completion =
output).

| Profile  | mode  | Hit@10 | llm_calls | input (prompt) | output (completion) | total tokens |
|----------|-------|--------|-----------|----------------|---------------------|--------------|
| Adam     | raw   | 0.400 | 64  | 119,829 | 3,737 | 123,566 |
| Adam     | jikji | **0.900** | **10** | **21,018** | **889** | **21,907** |
| Bei      | raw   | 0.300 | 51  | 62,367  | 1,996 | 64,363  |
| Bei      | jikji | **0.800** | **10** | **26,396** | **565** | **26,961** |
| Victoria | raw   | 0.500 | 35  | 60,199  | 2,186 | 62,385  |
| Victoria | jikji | **1.000** | **10** | **18,204** | **895** | **19,099** |

Aggregate (30 cases/mode):

| mode  | Hit@10 | llm_calls | input (prompt) | output (completion) | total tokens |
|-------|--------|-----------|----------------|---------------------|--------------|
| raw   | 0.400 | 150 | 242,395 | 7,919 | 250,314 |
| jikji | **0.900** | **30** | **65,618** | **2,349** | **67,967** |

Observations:

- **Token diet works.** With Jikji the agent answers in a single bounded turn,
  so total tokens fall **250,314 → 67,967 (−72.8%, 3.7×)** and input tokens
  fall **242,395 → 65,618 (−72.9%)** versus raw browsing. The previous
  multi-turn brief handoff had been *more* expensive than raw; the map-first
  diet reverses that.
- **LLM calls drop 5×** (150 → 30, i.e. 1 call/case) — the dominant driver of
  the token savings.
- **Accuracy improves** in every profile (Hit@10 aggregate 0.400 → 0.900).
- **Sample size restored.** With 10 cases/profile, raw lands at a realistic
  ~0.40 aggregate (Adam 0.40, Bei 0.30, Victoria 0.50) instead of the earlier
  3-case fluke where raw scored 0% on Adam.

`llm_calls`, `prompt_tokens`, and `completion_tokens` are read per session from
the Hermes session store (`state.db` + session transcript) keyed on the
`session_id` emitted by each `hermes chat -Q` invocation, then summed per task
and per mode into the JSON report.

## Reproduce

```bash
# Deterministic suite
jikji hippocamp-suite .benchmarks/hippocamp-large --profiles Adam,Bei,Victoria

# Real-agent benchmark with token diet (per profile, 10 cases)
jikji hermes-bench .benchmarks/hippocamp-large/Adam_Subset \
  --eval-set .benchmarks/hippocamp-large/Adam_Subset_hippocamp_eval_set.jsonl \
  --modes raw,jikji-fast --cases 10 --max-turns 8 --fast-max-turns 1 \
  --candidate-top-k 5 --skills jikji \
  --provider openrouter --model openai/gpt-4o-mini --json
```

Machine-readable aggregate: [`hippocamp-rerun-report.json`](./hippocamp-rerun-report.json).
