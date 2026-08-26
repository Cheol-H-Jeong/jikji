# Jikji 에이전트 검색 쿼리 재작성 분석 및 개선 방향

## 결론

현재 Jikji는 사용자의 자연어 문장을 검색 입력으로 보존하면서 shell 노이즈 제거, 고정 규칙 기반 variant 생성, 앵커 추출, 후보 재점수화를 수행한다. 그러나 제목 필수어, 본문 필수 개념, 정답 식별어, 부정 조건을 구조화해 검색용 쿼리를 새로 만드는 독립 단계는 아직 없다.

이번 문서는 현재 동작을 변경하지 않는 점검 결과와 다음 구현 change set의 계약을 정의한다.

## 1. 현재 호출 흐름

```text
jikji find ROOT "사용자 자연어 문장" --json
  → crates/jikji-cli/src/search_commands.rs::run_find
  → discover_payload(args)
  → crates/jikji-search/src/discover.rs::discover
  → DiscoverRequest::from
  → strip_shell_noise
  → query_variants
  → merge_candidates
  → search(root, variant)
  → 후보 dedupe·rescore·handoff payload
```

### 직접 확인한 근거

- `crates/jikji-cli/src/search_commands.rs:120-165`
  - `args.query`가 `discover()`에 그대로 전달된다.
  - 출력 payload에도 원래 query가 보존된다.
- `crates/jikji-search/src/discover.rs:33-88`
  - `DiscoverRequest`가 `retrieval_query`, `query_type`, `variants`, `retry_query`를 만든다.
  - payload에 `query`, `query_type`, `query_variants`, `search_plan`, `handoff_action`을 넣는다.
  - `llm_search_plan.rewrite_cycle`은 현재 `none`이다.
- `crates/jikji-search/src/discover.rs:100-123`
  - `strip_shell_noise()` 결과를 `query_variants()`에 넘긴다.
  - retry query는 첫 번째 변형(`variants[1]`)을 우선 사용한다.
- `crates/jikji-search/src/discover_query.rs:6-43`
  - shell 명령어/제어문자/일부 일반어를 제거한다.
  - 원문 variant를 항상 첫 번째로 유지한다.
  - `NDA`/`confidential`에만 고정 문자열을 추가한다.
  - 나머지는 ASCII 영숫자 `anchor_tokens()`를 추가한다.
  - variant는 최대 6개다.
- `crates/jikji-search/src/map_query.rs:3-24`
  - 토큰, 파일명 앵커, 인용어, 날짜 앵커를 추출한다.
- `crates/jikji-search/src/map_rescore.rs:9-198`
  - 파일명/경로/폴더/본문/evidence/희귀어/날짜/인용어를 사용해 후보를 재점수화한다.
  - 이는 검색 후 후보 판별이며, 의미 조건을 새 query로 재작성하는 단계는 아니다.
- `crates/jikji-search/src/discover_contract.rs:59-64`
  - 낮은 confidence에서 `rewrite_query_and_fallback_search`를 권고할 수 있다.
  - 실제 재작성 실행 대신 handoff 권고와 retry command를 반환한다.
- `skills/jikji/SKILL.md:48-55`
  - 공개 에이전트 인터페이스는 `jikji find ROOT "query" --json`이다.
  - 에이전트가 사용할 의미 기반 query rewriter는 skill 내부에 없다.

## 2. 재현 예시

입력:

```text
find the final vendor renewal agreement for ACME in FY25 excluding draft copies
```

현재 관찰된 payload 핵심:

```json
{
  "query": "find the final vendor renewal agreement for ACME in FY25 excluding draft copies",
  "query_type": "single_file",
  "query_variants": [
    "the final vendor renewal agreement for ACME in FY25 excluding draft copies",
    "2025 25 acme agreement copies draft excluding final for fy25 in renewal the vendor"
  ],
  "recommended_action": "verify_top_candidates",
  "handoff_action": "direct_use"
}
```

판정:

- `find the`는 분류에는 사용되지만 검색 query에서 의미적으로 제거되지 않는다.
- `excluding draft copies`는 negative constraint가 되지 않고 단순 토큰으로 남는다.
- `final`, `vendor`, `renewal`, `agreement`, `ACME`, `FY25`는 필수어/선호어/식별어로 구분되지 않는다.
- `draft`와 `copies`를 제외해야 한다는 조건이 후보 필터 또는 penalty로 구조화되지 않는다.
- 따라서 현재는 “원문 + 앵커 재배열”에 가깝다.

## 3. z 코딩 에이전트 비교의 증거 경계

저장소 안에는 z 코딩 에이전트의 실제 `find`/`grep` 재작성 구현이 없다. 따라서 z의 구체적인 알고리즘은 **Unknown**이다.

현재 저장소에서 확정할 수 있는 비교 기준은 다음뿐이다.

- Jikji skill은 `jikji find ROOT "query" --json` first-action 규칙을 제공한다.
- Jikji Rust 검색기는 multi-query variant와 lexical/map scoring을 제공한다.
- 외부 z 에이전트가 검색 전에 별도 query rewrite를 수행한다면, Jikji에도 동일한 역할의 명시적 단계가 필요하다.

## 4. 권장 구조

기존 `discover()` 내부에 흩어진 heuristic을 더 늘리지 말고, 다음 단계를 별도 모듈로 둔다.

```text
original_query
  → shell/noise sanitizer
  → QueryRewritePlan
  → strict/relaxed/discriminator query variants
  → Jikji lexical/map/graph search
  → candidate constraint verification
  → high confidence direct use
  → no-result/ambiguous: 최대 1회 sharpened retry
  → retry 후에도 실패: 기존 handoff/raw-fallback 계약
```

권장 파일:

```text
crates/jikji-search/src/query_rewrite.rs
```

호출 위치:

```text
crates/jikji-search/src/discover.rs::DiscoverRequest::from
```

`searcher.rs`와 `map_rescore.rs`는 검색/점수화 책임을 유지하고, query rewrite module이 후보 점수화 로직을 중복하지 않도록 한다.

## 5. QueryRewritePlan 계약

입력:

```json
{
  "original_query": "2025년 공급업체 계약 갱신 문서 중 직원 공유 금지 조항이 있는 최종본",
  "root": "/abs/canonical/root",
  "top_k": 10
}
```

출력:

```json
{
  "original_query": "...",
  "retrieval_query": "공급업체 계약 갱신 2025 직원 공유 금지 최종본",
  "must_title_terms": ["공급업체", "계약", "갱신", "2025"],
  "must_content_terms": ["직원 공유 금지"],
  "discriminator_terms": ["최종본"],
  "exclude_terms": ["사본", "초안"],
  "query_variants": [
    {"query":"공급업체 계약 갱신 2025 직원 공유 금지 최종본","purpose":"strict"},
    {"query":"2025 계약 갱신 직원 공유 금지","purpose":"relaxed"}
  ],
  "rewrite_confidence": "medium",
  "rewrite_reasons": ["date", "document_type", "content_constraint", "exclusion"]
}
```

원문은 항상 보존하고, 로그에는 원문 자체 대신 hash만 남긴다.

## 6. 생성 규칙

### 제목/경로 필수어

- 날짜, 고유명사, 계약/보고서/회의록 같은 문서 유형, 명시된 파일명·확장자는 우선 보존한다.
- `최종`, `원본`, `갱신`, `정식`처럼 문서 판별에 직접 쓰인 단어는 preferred 또는 discriminator로 승격한다.
- “문서”, “파일”, “찾아줘”, “어디”, “which” 같은 요청 문법은 기본 retrieval query에서 제거한다.

### 본문 필수 개념

- `~조항`, `~내용`, `~을 포함`, `~에 대한` 뒤의 명사구를 content must term 후보로 만든다.
- 긴 자연어를 무리하게 하나의 exact phrase로 만들지 않고, 짧은 핵심 개념과 phrase variant를 함께 만든다.
- 본문 필수어는 filename hit만으로 충족한 것으로 판정하지 않는다.

### 식별어

- 고객명, 계약번호, 제품명, 날짜, 희귀한 조항 표현처럼 유사 문서 간 차이를 만드는 단어를 discriminator로 둔다.
- 검색 결과 evidence와 indexed body/map metadata에서 실제 출현 여부를 확인한다.

### 부정 조건

- `제외`, `말고`, `아닌`, `사본 없이`, `초안 제외`, `메일 제외`를 `exclude_terms`로 분리한다.
- 후보가 exclude term을 filename/path/body에 포함하면 제거하거나 강한 penalty를 준다.
- 부정어 자체(`제외`, `말고`)는 retrieval term으로 보내지 않는다.

## 7. 과도한 재작성 방지

- 짧은 고유 파일명·정확한 계약번호·따옴표 query는 원문을 strict query로 그대로 사용한다.
- synonym expansion은 고정된 안전 사전만 사용하고 무제한 생성하지 않는다.
- 기본 variant는 `strict`, `relaxed` 두 개, 필요 시 `discriminator` 하나까지로 제한한다.
- variant 총량은 기존 상한 6개를 넘지 않는다.
- rewrite confidence가 low이면 원문을 보존한 채 기존 검색으로 진행하고, 자동 의미 추론으로 조건을 추가하지 않는다.

## 8. 결과 기반 정제와 재시도

1. strict 검색
2. 후보별 `must_title_hits`, `must_content_hits`, `discriminator_hits`, `exclude_hits` 계산
3. 다음 중 하나면 relaxed query를 정확히 한 번 실행한다.
   - 후보 0개
   - 필수 조건 충족 후보 0개
   - 상위 후보 간 constraint score 차이가 임계값 미만
4. relaxed에도 실패하면 기존 `jikji_retry`/`retry_proof` handoff를 반환한다.
5. retry 후 raw fallback은 기존 계약처럼 허용된 경우에만 수행한다.

반환 후보 예시:

```json
{
  "path": "contracts/ACME_2025_renewal_final.pdf",
  "constraint_score": 0.92,
  "must_title_hits": ["ACME", "2025", "renewal"],
  "must_content_hits": ["employee sharing prohibited"],
  "discriminator_hits": ["ACME-2025-17"],
  "exclude_hits": []
}
```

## 9. 로깅·검증

로그 허용 필드:

```text
request_id
root_key_hash
query_type
rewrite_confidence
variant_count
selected_variant_hash
candidate_count
constraint_match_rate
duration_ms
```

로그 금지:

- 사용자 원문 전체
- 문서 본문
- token
- debug mode가 아닌 전체 로컬 경로

payload에는 `query_rewrite.applied`, `original_query_hash`, `selected_variant`, 조건 개수와 retry 여부만 노출한다.

## 10. 평가 사례와 수용 기준

필수 평가 사례:

- 날짜 + 문서 유형 + 본문 조항
- `final`/`draft`/`copy` 구분
- filename에 없고 PDF/HWP body에만 있는 조건
- 고객명/계약번호 discriminator
- 한국어 무공백 조항
- `사본 제외`, `메일 제외` negative constraint
- 0건 strict → relaxed 1회
- 다수 후보 → discriminator 1회
- shell noise/악성 입력
- 정확한 짧은 파일명 query의 불필요한 rewrite 방지

수용 기준:

1. `original_query`와 `retrieval_query`가 분리된다.
2. title/content/discriminator/exclude 조건을 별도 확인할 수 있다.
3. 후보별 조건 충족률과 negative violation이 반환된다.
4. strict/relaxed/discriminator variant가 결정론적이다.
5. 자동 재검색은 요청당 최대 1회다.
6. 기존 `answer_paths`, `handoff_action`, `retry_proof`, `raw_fallback_allowed`와 호환된다.
7. query rewrite는 검색 점수화와 중복 구현되지 않는다.
8. hardbench/HippoCamp의 folder-context, body-clue, decoy 사례에 회귀가 없다.

## 11. 구현 범위 판단

현재 단계에서 필요한 것은 heuristic을 즉시 대규모로 추가하는 것이 아니다. 먼저 `QueryRewritePlan` 계약과 평가 fixture를 고정한 뒤, 별도 change set에서 규칙 기반 extractor를 구현하고, 그 다음 필요할 때만 제한된 LLM rewrite adapter를 검토한다. LLM을 도입하더라도 원문 보존, JSON schema 검증, output truncation/retry, per-field sanitization, 호출 상한을 필수로 둔다.
