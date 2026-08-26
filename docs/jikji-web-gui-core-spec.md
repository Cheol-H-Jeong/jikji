# Jikji Web GUI 핵심 기능 명세

## 1. 목적과 범위

외부에서 개발하는 웹 GUI는 Jikji가 관리하는 로컬 지식베이스를 탐색·조회·검색·확장하는 사용자 인터페이스다. GUI는 파일 원본을 이동·삭제·개명하지 않는다. 실제 인덱싱과 검색의 권위 있는 실행자는 Jikji Rust CLI/API이며, 웹 GUI는 해당 API의 same-origin 클라이언트다.

### 필수 화면

1. **왼쪽 메인 뷰 — Local File Explorer**
   - 사용자가 지정한 canonical root 아래의 실제 로컬 파일 시스템을 Windows Explorer처럼 계층적으로 탐색한다.
   - 폴더 펼침/접기, 현재 경로 breadcrumb, 정렬, 파일 선택, 다중 선택을 지원한다.
   - 이 뷰는 현재 파일 시스템을 보여준다. 인덱스 목록과 동일한 데이터 소스를 재사용하지 않는다.
2. **왼쪽/중앙 보조 뷰 — Search Results**
   - 검색 결과를 리스트 또는 테이블로 표시한다.
   - 결과는 `jikji find` API가 반환한 인덱스 범위로 제한한다.
3. **오른쪽 메인 뷰 — Markdown Preview**
   - 선택한 파일에 대해 Jikji가 생성·캐시한 문서 텍스트를 Markdown으로 렌더링한다.
   - 원문 미리보기와 검색 근거를 구분하고, 검색어 일치 구간을 하이라이트한다.
4. **Index Scope / View Options**
   - `모든 파일`, `인덱스 안 됨`, `기본 인덱스`, `내용 인덱스` 필터를 단일 선택 또는 상호 배타 토글로 제공한다.
   - 각 파일/폴더에 기본 인덱스와 내용 인덱스 상태를 명시한다.

## 2. 인덱스 상태 모델

각 파일 또는 폴더는 다음 상태를 가진다.

| 상태 | 의미 | 검색 포함 |
|---|---|---|
| `unindexed` | Jikji 기본 인덱스에 없음 | 제외 |
| `basic` | 경로·이름·metadata가 기본 인덱스에 있음 | 이름/경로 검색 |
| `content` | 문서 본문, 미디어 추출 내용 또는 archive member 내용까지 인덱스됨 | 이름/경로/본문 검색 |
| `processing` | 기본 또는 내용 인덱싱 작업 중 | 작업 정책에 따름, 결과에는 제외 권장 |
| `failed` | 마지막 작업 실패. 원본은 보존 | 기존 유효 인덱스가 있으면 이전 범위 유지 |
| `stale` | 원본 변경 이후 인덱스가 오래됨 | 기존 결과를 먼저 반환하고 refresh 상태 표시 |

상태 우선순위는 `processing > failed/stale > content > basic > unindexed`가 아니라, UI가 **기본 상태**와 **작업 상태**를 별도 필드로 표시하는 방식으로 구현한다. 예: `scope=basic`, `job_state=processing`.

## 3. 왼쪽 File Explorer 계약

### 데이터 원천

`GET /api/files?path=<root-relative-path>`는 실제 파일 시스템을 읽어 반환한다. GUI는 이 응답으로만 탐색기 트리를 구성한다.

```json
{
  "root": "/abs/canonical/root",
  "path": "Documents",
  "entries": [
    {
      "path": "Documents/report.pdf",
      "name": "report.pdf",
      "type": "file|directory|symlink|other",
      "size": 12345,
      "mtime": 1780000000,
      "status": "unindexed|basic|content|processing|failed|stale"
    }
  ]
}
```

- `path`는 root-relative canonical 경로다.
- `..`, absolute path, root 밖 symlink는 표시·호출 모두 거부한다.
- 폴더는 lazy-load한다. 최초 root 목록만 받은 뒤 하위 폴더를 열 때 다시 요청한다.
- 정렬은 `type(directory first)`, `name`, `size`, `mtime`, `status`를 지원하고 서버 결과를 임의로 인덱스 목록으로 대체하지 않는다.
- 원본 파일이 사라지면 `missing` 상태를 표시하고 자동 삭제하지 않는다.

## 4. 별도 Jikji Index 목록 계약

탐색기와 분리된 색인 전용 패널은 `GET /api/indexed-files?path=<root-relative-prefix>`를 사용한다.

```json
{
  "root": "/abs/canonical/root",
  "path": "",
  "source": "jikji_index",
  "entries": [
    {
      "path": "docs/report.pdf",
      "name": "report.pdf",
      "type": "file|directory",
      "status": "basic|content|stale|failed",
      "size": 12345,
      "indexed_at": 1780000000,
      "parse_status": "native_text|parser|unsupported|failed"
    }
  ],
  "statistics": {
    "files": 100,
    "folders": 12,
    "documents": 80,
    "chunks": 300,
    "content_files": 18,
    "failed": 1
  }
}
```

이 패널에는 실제 색인 artifact/central SQLite에 존재하는 항목만 표시한다. 탐색기에 보이는 `unindexed` 항목은 이 패널과 검색 결과에 나타나지 않는다.

## 5. View 옵션과 선택 작업

### 필터

- `all`: 실제 탐색기 항목 전체
- `unindexed`: `scope=unindexed`
- `basic`: `scope=basic` 이상이면서 `scope != content`인 항목
- `content`: `scope=content`

폴더 필터는 하위 항목 중 하나라도 조건을 만족하면 폴더를 표시하며, 폴더의 집계 상태(`3/12 content`)를 함께 표시한다.

### 선택 및 작업

다중 선택은 파일/폴더 목록에서 checkbox와 `aria-selected`로 표시한다.

- `기본 인덱스에 포함`: 선택 항목을 `POST /api/index-selection`에 `mode=basic`으로 전달
- `내용 인덱스에 포함`: 선택 항목을 `POST /api/index-selection`에 `mode=content`와 bounded media/archive 옵션으로 전달
- `작업 취소`: 지원 가능한 작업만 `POST /api/jobs/{id}/cancel`; 취소 불가 시 명시적으로 표시

요청/응답:

```http
POST /api/index-selection?mode=basic&token=...
Content-Type: application/json

{"paths":["docs/report.pdf","media/clip.mp4"],"media_ocr":false,"media_asr":false,"archive_max_entries":1000}
```

```json
{
  "job_id": "job-42",
  "state": "queued",
  "selected": 2,
  "mode": "basic",
  "progress": 0
}
```

GUI는 `GET /api/jobs/{job_id}`를 polling하여 `queued → running → completed|failed|cancelled`와 다음 진행 정보를 표시한다.

```json
{"job_id":"job-42","state":"running","progress":42,"completed":5,"total":12,"current_path":"docs/report.pdf","mode":"content","error":null}
```

작업 완료 후 탐색기와 색인 목록을 각각 새로고침한다. 원본 경로는 변경하지 않는다.

## 6. 검색

검색 입력은 `GET /api/find?q=<query>&top_k=<n>`으로 연결한다. GUI가 파일 시스템을 다시 crawl하거나 자체 검색하지 않는다.

결과 행 필드:

- `name`: 파일명 또는 폴더명
- `path`: root-relative 전체 경로
- `scope`: `basic` 또는 `content`
- `score`: Jikji 점수
- `evidence`: 매칭 근거
- `parse_status`: parser 상태
- `indexed_at`: 마지막 색인 시각

정렬은 `relevance`, `name`, `path`, `scope`, `indexed_at`별로 제공하며 relevance 외 정렬은 화면에서 수행해도 된다. 검색 결과는 반드시 Jikji가 반환한 path 집합에 한정된다.

상태:

- 빈 입력: `검색어를 입력하세요`
- 무결과: `색인된 파일에서 결과를 찾지 못했습니다`
- 색인 준비 전: `색인을 준비하는 중입니다` + 작업 상태
- timeout: `검색 시간이 초과되었습니다` + retry/status
- API 오류: `검색을 불러오지 못했습니다` + `request_id`, code, retry 가능 여부

## 7. 오른쪽 Markdown Preview

선택된 색인 결과는 `GET /api/preview?path=...&q=...`로 읽는다. 미리보기는 탐색기에서 임의로 읽은 raw text가 아니라 Jikji artifact/doc cache를 우선 사용한다.

```json
{
  "path": "docs/report.pdf",
  "format": "markdown",
  "content": "# Report\n\n...",
  "matches": [{"start":10,"end":18}],
  "match_unit": "utf16_code_unit",
  "scope": "content",
  "supported": true,
  "parse_status": "parser"
}
```

- Markdown headings, lists, tables, code blocks, links를 보기 좋게 렌더링한다.
- 서버가 반환한 content는 HTML 문자열로 주입하지 않는다. Markdown renderer의 허용 태그/URL scheme sanitization을 적용한다.
- `matches`는 UTF-16 code-unit offset으로 해석한다.
- binary/미지원/본문 미생성은 metadata와 `Preview unavailable` 사유를 표시한다.
- 선택 파일이 `basic`뿐이면 본문 대신 `기본 인덱스만 포함됨 — 내용 인덱싱` 안내를 표시한다.

## 8. 기본/내용 인덱싱 정책

기본 인덱스는 모든 지원 경로의 이름·상대 경로·size·mtime·type·metadata를 bounded하게 기록한다. 기본 prepare는 원본 media/audio/video bytes나 archive member 본문을 검색 corpus에 넣지 않는다.

내용 인덱스는 사용자가 명시적으로 요청한 선택 항목에만 적용한다.

- 문서: parser가 추출한 Markdown/text와 chunk metadata
- 이미지: OCR 엔진 결과가 있을 때만 OCR text, 실패 시 metadata와 실패 사유
- 오디오/비디오: ASR 엔진 결과가 있을 때만 transcript, 실패 시 metadata와 실패 사유
- archive: zip/tar/7z/rar 정책에 따른 bounded member path·content
- 각 작업은 `archive_max_entries`, `archive_max_entry_bytes`, `archive_max_total_bytes`, media size/time limits를 강제한다.
- 예상 비용과 실제 `entries`, `bytes`, `elapsed`, `failed`를 표시한다.

## 9. 보안·동시성·오류

- GUI는 same-origin API만 호출한다. Rust upstream loopback bind는 외부에 노출하지 않는다.
- 모든 mutation은 인증 세션, management token/handoff, canonical root boundary, serialized mutation lock을 거친다.
- token, 원문 본문, 전체 로컬 경로를 로그에 남기지 않는다.
- 모든 요청 오류는 `code`, 사용자 메시지, `request_id`, `retryable`, `details`를 제공한다.
- 작업 중복 실행을 막고, refresh 실패 시 기존 유효 index를 보존한다.
- UI는 `loading`, `success`, `empty`, `error`를 별도 상태로 관리하며 `checking`을 종결 상태로 사용하지 않는다.

## 10. 수용 기준

1. 왼쪽 File Explorer에서 root 아래 실제 폴더를 펼치고 접을 수 있으며, 색인되지 않은 파일도 `unindexed`로만 표시된다.
2. 별도 Jikji Index 패널에는 artifact/SQLite에 실제 존재하는 root·폴더·파일만 표시된다.
3. 필터 `all/unindexed/basic/content`를 전환하면 목록과 집계가 일치한다.
4. 하나 또는 여러 항목을 선택해 basic/content 인덱싱을 요청하면 job progress와 완료/실패 상태가 표시된다.
5. `jikji find` 결과는 색인된 path/name/content만 반환하며 relevance/name/path/scope 정렬이 가능하다.
6. 결과 선택 시 오른쪽에 sanitized Markdown preview, metadata, match highlight가 표시된다.
7. 빈 입력, 무결과, 준비 전, timeout, 403, 502, malformed/binary preview가 서로 다른 안내 상태를 보인다.
8. refresh/reindex/deep-index 실패 시 원본 파일과 기존 유효 색인이 보존된다.
9. 실제 MarkerAI 경로 또는 배포된 same-origin reverse proxy에서 API URL, 상태 코드, UI 상태를 재현할 수 있다.
10. Rust API와 central SQLite가 권위 있는 데이터 원천이며 GUI 동작에 Python/Node runtime이 필요하지 않다.

## 11. 구현 시 확인할 기존 Jikji API와 변경 필요 항목

현재 계약을 그대로 사용할 수 있는 API:

- `GET /api/status`, `GET /api/roots`
- `GET /api/files?path=...`
- `GET /api/find?q=...`, `GET /api/preview?path=...&q=...`
- `POST /api/refresh`, `POST /api/reindex-folder`, `POST /api/deep-index-target`
- `POST|DELETE /api/remove-folder`, `POST|DELETE /api/remove-root`
- `GET /api/jobs/{id}`

외부 GUI 요구를 충족하려면 Rust API에 추가하거나 확장해야 하는 계약:

- `GET /api/indexed-files?path=...`에 `scope`, parse status, indexed time, statistics 추가
- `POST /api/index-selection`으로 다중 선택 basic/content 작업 추가
- 선택 작업용 job progress와 cancellation 계약 추가
- Markdown/doc-cache preview endpoint 추가 또는 기존 `/api/preview`를 artifact 우선으로 확장
- `GET /api/status`에 files/folders/documents/chunks/content_files/failed/stale와 health enum 추가
- 모든 Rust 오류를 표준 error envelope로 통일

이 문서는 외부 GUI 개발자가 구현할 핵심 사용자 계약이며, 기존 embedded Rust SPA를 다시 확장하는 지시가 아니다.
