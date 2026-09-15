use std::net::{TcpListener, TcpStream};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use jikji_core::PrepareOptions;
use jikji_index::prepare;
use serde_json::json;

use crate::args::GuiArgs;
use crate::output::print_json;

mod http;
mod jobs;
mod routing;
mod scenarios;
mod token;

use routing::{GuiState, route_request};
use token::ManagementToken;

const INDEX_HTML: &str = r##"<!doctype html>
<html lang="ko">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="color-scheme" content="dark">
  <title>직지 라이브러리</title>
  <style>
    :root {
      --ink: oklch(92% 0.018 190); --muted: oklch(70% 0.025 190);
      --paper: oklch(16% 0.025 205); --panel: oklch(21% 0.028 205);
      --line: oklch(34% 0.035 205); --soft: oklch(27% 0.04 195);
      --accent: oklch(76% 0.14 190); --accent-dark: oklch(84% 0.12 190);
      --good: oklch(72% 0.13 145); --bad: oklch(74% 0.16 25);
      --r1: 4px; --r2: 8px; --r3: 12px;
      --shadow: 0 8px 24px oklch(5% 0.02 205 / .32);
      font-family: "Aptos", "Segoe UI", sans-serif; color: var(--ink); background: var(--paper);
    }
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; line-height: 1.5; }
    button, input, select { font: inherit; color: inherit; }
    button, input, select, .file-row { min-height: 40px; }
    button { border: 1px solid var(--line); border-radius: var(--r1); background: var(--panel); padding: 8px 12px; cursor: pointer; font-weight: 650; }
    button:hover { border-color: var(--accent); color: var(--accent-dark); }
    button.primary { border-color: var(--accent); background: var(--accent); color: var(--panel); }
    button.danger { color: var(--bad); }
    button:disabled { cursor: wait; opacity: .55; }
    :focus-visible { outline: 3px solid oklch(69% 0.13 65); outline-offset: 2px; }
    .topbar { display: flex; align-items: center; gap: 16px; padding: 16px 24px; border-bottom: 1px solid var(--line); background: var(--panel); }
    .brand { display: flex; align-items: baseline; gap: 10px; min-width: 190px; }
    .brand strong { font-family: Georgia, serif; font-size: 24px; letter-spacing: -.04em; }
    .brand span, .eyebrow { color: var(--muted); font-size: 12px; letter-spacing: .08em; text-transform: uppercase; }
    .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0; }
    .search-form { display: flex; gap: 8px; flex: 1; max-width: 760px; }
    input, select { width: 100%; border: 1px solid var(--line); border-radius: var(--r1); background: var(--paper); padding: 8px 12px; }
    .token-wrap { display: flex; align-items: center; gap: 8px; margin-left: auto; }
    .token-wrap input { width: 170px; }
    .stats { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; padding: 16px 24px; }
    .stat { min-width: 0; min-height: 72px; padding: 12px 16px; border: 1px solid var(--line); border-radius: var(--r2); background: var(--panel); }
    .stat b { display: block; margin-top: 4px; font-family: Georgia, serif; font-size: 20px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .deep-status { margin: 0 24px 16px; padding: 12px 16px; border: 1px solid var(--line); border-radius: var(--r2); background: var(--panel); }
    .deep-status-grid { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 8px; margin-top: 8px; color: var(--muted); font-size: 13px; }
    .deep-status-grid b { display: block; color: var(--ink); font-variant-numeric: tabular-nums; }
    .controls { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
    .control-field { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 13px; }
    .control-field input { width: 92px; min-height: 36px; }
    .control-field input[type="checkbox"] { width: 20px; }
    .control-field input[type="text"] { width: 180px; }
    .control-field input.find-wide { width: 220px; }
    .add-root-copy { color: var(--muted); }
    @media (max-width: 980px) { .deep-status-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
    @media (max-width: 640px) { .deep-status { margin-inline: 12px; } .deep-status-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
    .workspace { display: grid; grid-template-columns: minmax(250px, 1fr) minmax(250px, 1fr) minmax(340px, 1.35fr) minmax(320px, 1.35fr); min-height: calc(100vh - 137px); border-top: 1px solid var(--line); }
    .pane { min-width: 0; background: var(--panel); }
    .pane + .pane { border-left: 1px solid var(--line); }
    .pane-head { display: flex; align-items: center; gap: 8px; min-height: 56px; padding: 8px 16px; border-bottom: 1px solid var(--line); }
    .pane-head h2 { margin: 0; font-family: Georgia, serif; font-size: 18px; }
    .pane-head .spacer { flex: 1; }
    .pane-note { margin: 0; padding: 10px 16px; color: var(--muted); font-size: 12px; border-bottom: 1px solid var(--line); }
    .compact { padding: 6px 9px; }
    .root-select { margin: 12px 16px 4px; width: calc(100% - 32px); }
    .tree-path { padding: 8px 16px; color: var(--muted); font-size: 13px; overflow-wrap: anywhere; }
    .list { margin: 0; padding: 0 8px 16px; list-style: none; }
    .file-row, .result { width: 100%; border: 0; border-radius: var(--r1); background: transparent; text-align: left; }
    .file-row { display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 8px; padding: 6px 8px; font-weight: 500; }
    .file-row:hover, .file-row[aria-current="true"], .result:hover, .result[aria-current="true"] { background: var(--soft); color: var(--ink); }
    .file-row small { color: var(--muted); font-variant-numeric: tabular-nums; }
    .indexed-row { display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 8px; padding: 8px; }
    .indexed-row .path { color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }
    .indexed-row small { color: var(--good); font-size: 12px; }
    .pane-head { display: flex; align-items: center; gap: 8px; min-height: 56px; padding: 8px 16px; border-bottom: 1px solid var(--line); }
    .pane-head h2 { margin: 0; font-family: Georgia, serif; font-size: 18px; }
    .pane-head .spacer { flex: 1; }
    .compact { padding: 6px 9px; }
    .root-select { margin: 12px 16px 4px; width: calc(100% - 32px); }
    .tree-path { padding: 8px 16px; color: var(--muted); font-size: 13px; overflow-wrap: anywhere; }
    .list { margin: 0; padding: 0 8px 16px; list-style: none; }
    .file-row, .result { width: 100%; border: 0; border-radius: var(--r1); background: transparent; text-align: left; }
    .file-row { display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 8px; padding: 6px 8px; font-weight: 500; }
    .file-row:hover, .file-row[aria-current="true"], .result:hover, .result[aria-current="true"] { background: var(--soft); color: var(--ink); }
    .file-row small { color: var(--muted); font-variant-numeric: tabular-nums; }
    .results-meta { padding: 12px 16px 0; color: var(--muted); font-size: 13px; }
    .result { display: block; margin-top: 8px; padding: 12px; border: 1px solid transparent; }
    .result strong { display: block; overflow-wrap: anywhere; }
    .result .path, .result .evidence { color: var(--muted); font-size: 13px; overflow-wrap: anywhere; }
    .result .score { float: right; color: var(--accent-dark); font-variant-numeric: tabular-nums; }
    .preview { padding: 16px; }
    .preview-meta { display: flex; flex-wrap: wrap; gap: 8px 16px; margin-bottom: 12px; color: var(--muted); font-size: 13px; }
    pre { margin: 0; max-height: calc(100vh - 250px); overflow: auto; border: 1px solid var(--line); border-radius: var(--r2); background: var(--paper); padding: 16px; white-space: pre-wrap; overflow-wrap: anywhere; font: 13px/1.65 "Cascadia Mono", monospace; tab-size: 2; }
    mark { border-radius: 2px; background: oklch(68% 0.13 88); color: oklch(18% 0.03 75); }
    .state { margin: 16px; padding: 24px 16px; border: 1px dashed var(--line); border-radius: var(--r2); color: var(--muted); text-align: center; }
    .state strong { display: block; color: var(--ink); margin-bottom: 4px; }
    .error { margin: 0; padding: 10px 24px; background: oklch(27% 0.07 25); color: oklch(84% 0.1 25); border-bottom: 1px solid oklch(45% 0.1 25); }
    .error[hidden], .toast[hidden] { display: none; }
    .toast { position: fixed; right: 20px; bottom: 20px; z-index: 4; max-width: 360px; padding: 12px 16px; border-radius: var(--r2); color: var(--panel); background: var(--ink); box-shadow: var(--shadow); }
    dialog { width: min(440px, calc(100% - 32px)); border: 1px solid var(--line); border-radius: var(--r3); background: var(--panel); color: var(--ink); box-shadow: var(--shadow); }
    dialog::backdrop { background: oklch(25% 0.025 65 / .35); }
    dialog h2 { font-family: Georgia, serif; }
    dialog menu { display: flex; justify-content: flex-end; gap: 8px; padding: 16px 0 0; }
    dialog.preview-dialog { width: min(900px, calc(100% - 32px)); max-height: 90vh; }
    .preview-dialog-body { min-height: 320px; }
    .preview-dialog-body iframe, .preview-dialog-body object, .preview-dialog-body img { width: 100%; height: 62vh; border: 1px solid var(--line); border-radius: var(--r2); background: var(--paper); object-fit: contain; }
    .scenario-strip { margin: 0 24px 16px; padding: 12px 16px; border: 1px solid var(--line); border-radius: var(--r2); background: var(--panel); }
    .scenario-strip .controls { margin-top: 8px; }
    .busy::after { content: ""; display: inline-block; width: 12px; height: 12px; margin-left: 8px; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin .7s cubic-bezier(.4,0,.2,1) infinite; }
    @keyframes spin { to { transform: rotate(360deg); } }
    @media (max-width: 980px) {
      .topbar { flex-wrap: wrap; } .search-form { order: 3; max-width: none; flex-basis: 100%; }
      .stats { grid-template-columns: repeat(2, 1fr); }
      .workspace { grid-template-columns: minmax(220px, .8fr) minmax(360px, 1.2fr); }
      .preview-pane { grid-column: 1 / -1; border-left: 0 !important; border-top: 1px solid var(--line); }
      pre { max-height: 480px; }
    }
    @media (max-width: 640px) {
      .topbar, .stats { padding-left: 12px; padding-right: 12px; }
      .brand { min-width: 0; } .brand span { display: none; } .token-wrap { margin-left: 0; flex: 1; }
      .token-wrap input { width: 100%; } .stats { grid-template-columns: 1fr 1fr; }
      .workspace { display: block; } .pane + .pane { border-left: 0; border-top: 1px solid var(--line); }
      .pane { min-height: 320px; } .search-form button { padding-inline: 16px; }
    }
    @media (prefers-reduced-motion: reduce) { .busy::after { animation: none; } }
  </style>
</head>
<body>
  <header class="topbar">
    <div class="brand"><strong>Jikji</strong><span>로컬 인덱스 라이브러리</span></div>
    <form class="search-form" id="search-form" role="search">
      <label class="sr-only" for="query">인덱스된 파일 찾기</label>
      <input id="query" name="q" type="search" placeholder="계약서, 메모, 사람 이름 검색" autocomplete="off">
      <button class="primary" id="search-button" type="submit">찾기</button>
    </form>
    <div class="token-wrap"><label class="eyebrow" for="manage-token">관리 토큰</label><input id="manage-token" type="password" autocomplete="off" spellcheck="false" aria-describedby="token-help" placeholder="변경 시 필요"></div>
  </header>
  <p id="token-help" hidden>토큰은 이 탭에만 두고 로컬 변경 API로만 보냅니다.</p>
  <div class="error" id="global-error" role="alert" hidden></div>
  <section class="stats" aria-label="인덱스 상태">
    <div class="stat"><span class="eyebrow">인덱스 상태</span><b id="health">상태 불러오는 중…</b></div><div class="stat"><span class="eyebrow">인덱스 파일</span><b id="file-count">—</b></div><div class="stat"><span class="eyebrow">루트 용량</span><b id="root-size">—</b></div><div class="stat"><span class="eyebrow">마지막 인덱싱</span><b id="last-indexed">—</b></div>
  </section>
  <section class="deep-status" aria-labelledby="deep-title">
    <div class="pane-head"><h2 id="deep-title">딥 인덱싱</h2><span class="spacer"></span><span class="eyebrow" id="deep-state">실행 전</span></div>
    <p id="deep-copy" class="add-root-copy">미디어·압축 본문은 선택 사항입니다. OCR/ASR과 압축 처리 한도를 정한 뒤 시작하세요.</p>
    <div class="controls"><label class="control-field"><input id="media-ocr" type="checkbox"> OCR</label><label class="control-field"><input id="media-asr" type="checkbox"> ASR</label><label class="control-field">압축 항목 <input id="archive-entries" type="number" min="1" step="1" value="1000"></label><label class="control-field">항목 바이트 <input id="archive-entry-bytes" type="number" min="1" step="1048576" value="16777216"></label><label class="control-field">총 바이트 <input id="archive-total-bytes" type="number" min="1" step="1048576" value="134217728"></label></div>
    <div class="deep-status-grid" aria-live="polite"><span>항목<b id="deep-entries">—</b></span><span>바이트<b id="deep-bytes">—</b></span><span>경과<b id="deep-time">—</b></span><span>예상 비용<b id="deep-cost">—</b></span><span>미디어<b id="deep-media">—</b></span></div>
  </section>
  <section class="scenario-strip" aria-labelledby="scenario-title"><div class="pane-head"><h2 id="scenario-title">검색 시나리오</h2><span class="spacer"></span><span class="eyebrow" id="scenario-count">—</span></div><p class="pane-note">같은 <code>jikji find</code> 경로로 반복 검색합니다. 확장자 필터와 찾기 옵션은 시나리오 실행에 포함됩니다.</p><div class="controls"><select id="scenario-select" aria-label="검색 시나리오"><option>시나리오 불러오는 중…</option></select><button class="compact" id="scenario-run" type="button">시나리오 실행</button><label class="control-field" for="find-top-k">결과 수</label><select id="find-top-k" aria-label="찾기 결과 수"><option value="10">10</option><option value="20" selected>20</option><option value="50">50</option></select><label class="control-field"><input id="include-hidden" type="checkbox"> 숨김 파일</label><label class="control-field"><input id="include-sensitive" type="checkbox"> 민감 경로</label><label class="control-field"><input id="after-jikji-retry" type="checkbox"> 재시도 후</label></div><div class="controls"><label class="control-field"><input id="find-fresh" type="checkbox"> 강제 준비</label><label class="control-field"><input id="find-no-background-refresh" type="checkbox"> 백그라운드 새로고침 끄기</label><label class="control-field" for="find-exclude">제외</label><input id="find-exclude" class="find-wide" type="text" placeholder="glob,glob" aria-label="제외 패턴"><label class="control-field" for="find-retry-proof">재시도 증명</label><input id="find-retry-proof" type="text" placeholder="proof" aria-label="재시도 증명"><label class="control-field" for="find-stale-after">신선도(초)</label><input id="find-stale-after" type="number" min="0" step="1" placeholder="86400" aria-label="신선도 초"><label class="control-field" for="find-max-files">최대 파일</label><input id="find-max-files" type="number" min="1" step="1" placeholder="0" aria-label="최대 파일"><label class="control-field" for="find-parse-timeout">파싱 초</label><input id="find-parse-timeout" type="number" min="0" step="0.1" placeholder="5" aria-label="파싱 초"><label class="control-field"><input id="find-first" type="checkbox"> 첫 결과만</label><label class="control-field"><input id="find-auto-prepare" type="checkbox"> 자동 준비</label><label class="control-field"><input id="find-no-auto-prepare" type="checkbox"> 자동 준비 끄기</label><label class="control-field" for="find-max-hash-bytes">해시 바이트</label><input id="find-max-hash-bytes" type="number" min="0" step="1" placeholder="536870912" aria-label="최대 해시 바이트"></div></section>
  <section class="scenario-strip" aria-labelledby="ops-title"><div class="pane-head"><h2 id="ops-title">운영</h2><span class="spacer"></span><span class="eyebrow">원본 파일은 삭제하지 않습니다</span></div><p class="pane-note">doctor, map, graph, brief, clean을 현재 루트에서 실행합니다.</p><div class="controls"><button class="compact" id="ops-doctor" type="button">Doctor</button><button class="compact" id="ops-map" type="button">Map</button><button class="compact" id="ops-graph" type="button">Graph 상태</button><button class="compact" id="ops-graph-query" type="button">Graph 검색</button><button class="compact" id="ops-graph-explain" type="button">Graph 설명</button><button class="compact" id="ops-brief" type="button">Brief</button><button class="compact" id="ops-clean-dry" type="button">Clean 미리보기</button><button class="compact danger" id="ops-clean" type="button">Clean 실행</button></div></section>
  <main class="workspace" aria-label="직지 인덱스 브라우저">
    <nav class="pane explorer-pane" aria-labelledby="explorer-title">
      <div class="pane-head"><h2 id="explorer-title">파일 탐색</h2><span class="spacer"></span><button class="compact" id="refresh" type="button">새로고침</button></div>
      <p class="pane-note">로컬 폴더를 탐색합니다. 아직 인덱싱되지 않은 파일도 보입니다.</p>
      <label class="sr-only" for="root-select">인덱스 루트</label><div class="controls"><select class="root-select" id="root-select"><option>루트 불러오는 중…</option></select><button class="compact" id="add-root" type="button">폴더 추가</button></div>
      <div class="controls selection-controls"><button class="compact" id="index-basic" type="button">선택 항목 기본 인덱싱</button><button class="compact" id="index-content" type="button">선택 항목 내용 인덱싱</button></div>
      <div class="controls selection-controls"><label for="view-filter">보기</label><select id="view-filter"><option value="all">모든 파일</option><option value="unindexed">미인덱싱만</option><option value="basic">기본 인덱스</option><option value="content">내용 인덱스</option></select><label for="view-sort">정렬</label><select id="view-sort"><option value="name">이름</option><option value="size">크기</option><option value="mtime">수정일</option><option value="status">인덱스 상태</option></select></div>
      <div class="tree-path" id="tree-path">/</div><div class="eyebrow" id="folder-context" aria-live="polite">현재 폴더: /</div>
      <ul class="list" id="file-list" aria-live="polite"><li class="state"><strong>탐색기 불러오는 중</strong>로컬 파일을 읽는 중…</li></ul>
    </nav>
    <section class="pane indexed-pane" aria-labelledby="indexed-title">
      <div class="pane-head"><h2 id="indexed-title">인덱스 범위</h2><span class="spacer"></span><span class="eyebrow" id="indexed-count">—</span></div>
      <p class="pane-note">검색은 이 목록과 인덱싱된 내용만 대상으로 합니다.</p>
      <div class="tree-path" id="indexed-path">/</div>
      <ul class="list" id="indexed-list" aria-live="polite"><li class="state"><strong>인덱스 불러오는 중</strong>직지 아티팩트를 읽는 중…</li></ul>
    </section>
    <section class="pane" aria-labelledby="results-title">
      <div class="controls result-controls"><label for="result-sort">결과 정렬</label><select id="result-sort"><option value="relevance">관련도</option><option value="name">이름</option><option value="path">경로</option><option value="scope">인덱스 범위</option></select></div>
      <div class="pane-head"><h2 id="results-title">찾기 결과</h2><span class="spacer"></span><span class="eyebrow" id="confidence"></span></div>
      <div class="results-meta" id="results-meta">검색은 <code>jikji find</code>와 같은 경로입니다.</div>
      <ol class="list" id="results" aria-live="polite"><li class="state"><strong>찾기 준비</strong>위에서 파일명, 주제, 사람 이름을 입력하세요.</li></ol>
    </section>
    <aside class="pane preview-pane" aria-labelledby="preview-title">
      <div class="pane-head"><h2 id="preview-title">파일 내용</h2><span class="spacer"></span><button class="compact" id="download" type="button" disabled>다운로드</button><button class="compact" id="reveal" type="button" disabled>폴더 열기</button><button class="compact" id="open" type="button" disabled>원본 열기</button></div>
      <div class="preview" id="preview"><div class="state"><strong>파일을 선택하세요</strong>인덱싱된 결과를 선택하면 내용을 볼 수 있습니다.</div></div>
    </aside>
  </main>
  <footer class="pane-head" aria-label="인덱스 관리">
    <span class="eyebrow">폴더 작업</span><span class="eyebrow" id="job-status">대기</span><span class="spacer"></span><button id="cancel-job" class="compact" type="button" disabled>작업 취소</button>
    <button id="reindex" type="button">루트 재인덱싱</button><button id="reindex-folder" type="button">폴더 재인덱싱</button><button id="deep-index" type="button">루트 딥 인덱싱</button><button id="deep-target-enable" type="button">폴더 딥 인덱싱</button><button id="deep-target-disable" type="button">딥 폴더 중지</button><button id="remove-folder" class="danger" type="button">폴더 제거</button><button class="danger" id="remove-root" type="button">루트 제거</button>
  </footer>
  <div class="toast" id="toast" role="status" aria-live="polite" hidden></div>
  <dialog id="confirm-dialog" aria-labelledby="confirm-title" aria-describedby="confirm-copy">
    <h2 id="confirm-title">작업 확인</h2><p id="confirm-copy"></p>
    <menu><button id="confirm-cancel" type="button">취소</button><button class="danger" id="confirm-ok" type="button">확인</button></menu>
  </dialog>
  <dialog id="preview-dialog" class="preview-dialog" aria-labelledby="preview-dialog-title"><div class="pane-head"><h2 id="preview-dialog-title">파일 미리보기</h2><span class="spacer"></span><button class="compact" id="preview-dialog-close" type="button">닫기</button></div><div class="preview-dialog-body" id="preview-dialog-body"></div></dialog>
  <dialog id="ops-dialog" class="preview-dialog" aria-labelledby="ops-dialog-title"><div class="pane-head"><h2 id="ops-dialog-title">운영 결과</h2><span class="spacer"></span><button class="compact" id="ops-dialog-close" type="button">닫기</button></div><pre class="preview-dialog-body" id="ops-dialog-body"></pre></dialog>
  <script>
  (() => {
    "use strict";
    const $ = (id) => document.getElementById(id);
    const state = { root: "", folder: "", selected: "", selectedRoot: "", selectedPaths: new Set(), query: "", roots: [], confirmAction: null, request: {}, currentJob: "" };
    const number = new Intl.NumberFormat();
    const setText = (id, value) => { $(id).textContent = value == null || value === "" ? "—" : String(value); };
    const bytes = (value) => { const n = Number(value); if (!Number.isFinite(n)) return "—"; const u = ["B","KB","MB","GB","TB"]; let i=0,x=n; while(x>=1024&&i<u.length-1){x/=1024;i++;} return `${x>=10||i===0?x.toFixed(0):x.toFixed(1)} ${u[i]}`; };
    const date = (value) => { if (!value) return "—"; const d = new Date(typeof value === "number" && value < 1e12 ? value * 1000 : value); return Number.isNaN(d.valueOf()) ? String(value) : d.toLocaleString(); };
    const errorMessage = (error) => { const detail=error?.detail || error?.error_detail; if (detail?.message) return `${detail.message} (${detail.request_id || detail.code || "error"})`; if (error?.message) return error.message; return String(error); };
    function showError(error) { const el=$("global-error"); el.textContent=errorMessage(error); el.hidden=false; }
    function clearError() { $("global-error").hidden=true; $("global-error").textContent=""; }
    function toast(message) { const el=$("toast"); el.textContent=message; el.hidden=false; clearTimeout(toast.timer); toast.timer=setTimeout(()=>el.hidden=true,3200); }
    function params(values) { const out=new URLSearchParams(); Object.entries(values).forEach(([k,v])=>{if(v!==undefined&&v!==null&&v!=="")out.set(k,String(v));}); return out; }
    function checkedFlag(id){ return $(id)?.checked ? 1 : undefined; }
    function textValue(id){ const v=$(id)?.value?.trim(); return v?v:undefined; }
    function numberValue(id){ const raw=$(id)?.value?.trim(); if(!raw) return undefined; const n=Number(raw); return Number.isFinite(n)?n:undefined; }
    function findTopK(){ const n=Number($("find-top-k")?.value||20); return Number.isFinite(n)?Math.min(100,Math.max(1,n)):20; }
    function findOptions(extra={}){ return { top_k:findTopK(), include_hidden:checkedFlag("include-hidden"), include_sensitive:checkedFlag("include-sensitive"), after_jikji_retry:checkedFlag("after-jikji-retry"), fresh:checkedFlag("find-fresh"), no_background_refresh:checkedFlag("find-no-background-refresh"), exclude:textValue("find-exclude"), retry_proof:textValue("find-retry-proof"), stale_after_seconds:numberValue("find-stale-after"), max_files:numberValue("find-max-files"), parse_timeout:numberValue("find-parse-timeout"), first:checkedFlag("find-first"), auto_prepare:checkedFlag("find-auto-prepare"), no_auto_prepare:checkedFlag("find-no-auto-prepare"), max_hash_bytes:numberValue("find-max-hash-bytes"), ...extra }; }
    function prepareFlags(){ return { include_hidden:checkedFlag("include-hidden"), include_sensitive:checkedFlag("include-sensitive"), exclude:textValue("find-exclude"), max_files:numberValue("find-max-files"), parse_timeout:numberValue("find-parse-timeout"), max_hash_bytes:numberValue("find-max-hash-bytes") }; }
    function isTimeout(error){ return /시간 초과|timed out/i.test(String(error?.message||"")); }
    async function api(path, values={}, options={}) {
      const controller=new AbortController(); const slow=["/api/find","/api/scenario-run","/api/preview","/api/doctor","/api/map","/api/graph","/api/brief","/api/clean","/api/refresh","/api/reindex"]; const timeout=setTimeout(()=>controller.abort(),slow.some(prefix=>path.includes(prefix))?120000:15000);
      try {
        const url = options.query ? `${path}?${options.query}` : `${path}?${params(values)}`;
        const response = await fetch(url, { method: options.method || "GET", headers: { "Accept": "application/json" }, cache: "no-store", signal: controller.signal });
        const contentType = response.headers.get("content-type") || "";
        const payload = contentType.includes("json") ? await response.json() : await response.text();
        if (!response.ok) { const detail=payload && (payload.error_detail || payload.error); const message=typeof detail === "string" ? detail : detail?.message || `요청 실패 (${response.status})`; throw Object.assign(new Error(message), { detail: payload?.error_detail, status: response.status }); }
        return payload;
      } catch(error) { if (error?.name === "AbortError") throw new Error("요청이 시간 초과되었습니다. 다시 시도하세요."); throw error; }
      finally { clearTimeout(timeout); }
    }
    function token() { const value=$("manage-token").value.trim(); if (!value) { $("manage-token").focus(); throw new Error("GUI 시작 시 표시된 관리 토큰을 입력하세요."); } return value; }
    function listState(target, title, copy) { target.replaceChildren(); const li=document.createElement("li"); li.className="state"; const strong=document.createElement("strong"); strong.textContent=title; li.append(strong,document.createTextNode(copy)); target.append(li); }
    function statistics(payload) { return payload.statistics || payload.stats || payload.manifest?.statistics || {}; }
    function updateDeepStatus(payload) { const deep=payload.deep_index || payload.deepIndex || {}; setText("deep-state", deep.state || (payload.deep_index ? "완료" : "실행 전")); setText("deep-entries", deep.entries ?? deep.archive_entries ?? deep.entry_count); setText("deep-bytes", deep.bytes != null ? bytes(deep.bytes) : null); setText("deep-time", deep.elapsed_ms != null ? `${deep.elapsed_ms} ms` : (deep.seconds != null ? `${deep.seconds} s` : deep.elapsed)); setText("deep-cost", deep.estimated_cost ?? deep.resource_cost ?? deep.estimated_resource_cost); setText("deep-media", deep.media_index === true ? "OCR/ASR 사용" : (deep.media_index === false ? "사용 안 함" : deep.media || "—")); }
    function updateStats(payload) { const stats=payload.statistics || payload.stats || payload.manifest?.statistics || {}; const manifest=payload.manifest || {}; setText("health", payload.prepared ? "준비됨" : (payload.root ? "인덱싱 필요" : "인덱스 루트 없음")); setText("file-count", number.format(stats.files ?? stats.file_count ?? manifest.file_count ?? 0)); setText("root-size", bytes(stats.bytes ?? stats.total_bytes ?? manifest.total_bytes)); setText("last-indexed", date(stats.updated_at ?? stats.indexed_at ?? manifest.generated_at)); updateDeepStatus(payload); }
    function deepOptions() { return { media_ocr: $("media-ocr").checked, media_asr: $("media-asr").checked, archive_max_entries: Number($("archive-entries").value), archive_max_entry_bytes: Number($("archive-entry-bytes").value), archive_max_total_bytes: Number($("archive-total-bytes").value) }; }
    async function loadRoots() { const data=await api("/api/roots"); state.roots=Array.isArray(data.roots)?data.roots:[]; state.root=data.active_root || state.root || state.roots[0]?.root || ""; const select=$("root-select"); select.replaceChildren(); if(!state.roots.length){const option=document.createElement("option");option.textContent="인덱스된 루트 없음";select.append(option);select.disabled=true;return;} select.disabled=false;state.roots.forEach(item=>{const option=document.createElement("option");option.value=item.root;option.textContent=item.root;option.selected=item.root===state.root;select.append(option);}); }
    async function loadStatus() { try { const data=await api("/api/status"); state.root=data.root || state.root; updateStats(data); } catch(error) { setText("health","오류"); showError(error); throw error; } }
    function selectedPaths() { return [...state.selectedPaths]; }
    function updateSelectionButtons() { const disabled=state.selectedPaths.size===0; $("index-basic").disabled=disabled; $("index-content").disabled=disabled; }
    function toggleSelection(path, checked) { if (checked) state.selectedPaths.add(path); else state.selectedPaths.delete(path); updateSelectionButtons(); }
    function addSelectionControls() { document.querySelectorAll("#file-list button[data-path]").forEach(button=>{if(button.querySelector("input[type=checkbox]"))return;const check=document.createElement("input");check.type="checkbox";check.checked=state.selectedPaths.has(button.dataset.path);check.setAttribute("aria-label",`${button.dataset.path} 선택`);check.addEventListener("click",event=>event.stopPropagation());check.addEventListener("change",event=>toggleSelection(button.dataset.path,event.target.checked));button.prepend(check);});updateSelectionButtons(); }
    new MutationObserver(addSelectionControls).observe($("file-list"),{childList:true});
    async function loadIndexed(folder="") { setText("indexed-path",folder||"/");listState($("indexed-list"),"인덱스 불러오는 중","인덱스 경로를 읽는 중…");try{const data=await api("/api/indexed-files",{path:folder});const entries=Array.isArray(data.entries)?data.entries:[];const list=$("indexed-list");list.replaceChildren();setText("indexed-count",`${entries.length}개 항목`);if(folder){const li=document.createElement("li"),up=document.createElement("button");up.className="file-row";up.type="button";up.append(document.createTextNode("↰"),document.createTextNode("상위 인덱스 폴더"));up.addEventListener("click",()=>loadIndexed(folder.split("/").slice(0,-1).join("/")));li.append(up);list.append(li);}entries.forEach(entry=>{const li=document.createElement("li"),row=document.createElement("div"),icon=document.createElement("span"),name=document.createElement("strong"),path=document.createElement("span"),status=document.createElement("small");row.className="indexed-row";icon.textContent=entry.type==="directory"?"▸":"·";name.textContent=entry.name;path.className="path";path.textContent=entry.path;status.textContent=entry.status||"indexed";row.append(icon,name,path,status);if(entry.type==="file")row.addEventListener("click",()=>loadPreview(entry.path));li.append(row);list.append(li);});if(!entries.length)listState(list,folder?"인덱스 폴더가 비어 있습니다":"인덱스된 파일이 없습니다","루트를 준비하거나 새로고침하면 파일이 추가됩니다.");}catch(error){listState($("indexed-list"),"인덱스를 불러올 수 없습니다",`${errorMessage(error)} 새로고침으로 다시 시도하세요.`);showError(error);} }
    async function loadFiles(folder="") { state.folder=folder;setText("tree-path",folder||"/");setText("folder-context",`현재 폴더: ${folder||"/"}`);const list=$("file-list");listState(list,"폴더 불러오는 중","로컬 파일을 읽는 중…");try{const data=await api("/api/files",{path:folder});let entries=Array.isArray(data.entries)?data.entries:[];const filter=$("view-filter")?.value||"all";if(filter!=="all")entries=entries.filter(entry=>filter==="unindexed"?entry.scope==="unindexed":filter==="content"?entry.scope==="content":entry.scope==="basic");const sort=$("view-sort")?.value||"name";entries.sort((a,b)=>sort==="size"?(Number(b.size)||0)-(Number(a.size)||0):sort==="mtime"?(Number(b.mtime)||0)-(Number(a.mtime)||0):sort==="status"?String(a.scope||a.status).localeCompare(String(b.scope||b.status)):String(a.name||a.path).localeCompare(String(b.name||b.path)));list.replaceChildren();if(folder){const li=document.createElement("li"),up=document.createElement("button");up.className="file-row";up.type="button";up.append(document.createTextNode("↰"),document.createTextNode("상위 폴더"));up.addEventListener("click",()=>loadFiles(folder.split("/").slice(0,-1).join("/")));li.append(up);list.append(li);}entries.forEach(entry=>{const li=document.createElement("li"),button=document.createElement("button"),icon=document.createElement("span"),name=document.createElement("span"),size=document.createElement("small");button.type="button";button.className="file-row";button.dataset.path=entry.path;icon.textContent=entry.type==="directory"?"▸":"·";name.textContent=entry.name||entry.path;size.textContent=entry.type==="directory"?(entry.status||"directory"):bytes(entry.size);button.append(icon,name,size);button.addEventListener("click",()=>entry.type==="directory"?loadFiles(entry.path):loadPreview(entry.path));li.append(button);list.append(li);});addSelectionControls();if(!entries.length)listState(list,folder?"폴더가 비어 있습니다":"탐색기가 비어 있습니다",folder?"이 폴더에 항목이 없습니다.":"루트를 선택해 로컬 파일을 탐색하세요.");}catch(error){listState(list,isTimeout(error)?"폴더 요청 시간 초과":"파일을 불러올 수 없습니다",`${errorMessage(error)} 새로고침으로 다시 시도하세요.`);showError(error);} }
    function previewText(data) { const container=$("preview");container.replaceChildren();const meta=document.createElement("div");meta.className="preview-meta";[data.path,data.renderer||data.type,bytes(data.size),data.encoding].filter(Boolean).forEach(value=>{const span=document.createElement("span");span.textContent=value;meta.append(span);});container.append(meta);if(data.supported===false){const box=document.createElement("div");box.className="state";const strong=document.createElement("strong");strong.textContent="미리보기를 표시할 수 없습니다";box.append(strong,document.createTextNode(data.reason||"이 파일 형식은 안전하게 표시할 수 없습니다."));container.append(box);return;}if(data.binary_url && data.media_type && !data.content){const box=document.createElement("div");box.className="state";const strong=document.createElement("strong");strong.textContent="팝업에서 렌더링";box.append(strong,document.createTextNode(`${data.renderer||"renderer"}로 파일 내용을 표시합니다.`));container.append(box);return;}const pre=document.createElement("pre"),content=String(data.content||""),matches=Array.isArray(data.matches)?data.matches.slice().sort((a,b)=>a.start-b.start):[];const index=(value,units)=>{let offset=0;for(let i=0;i<value.length;){if(offset>=units)return i;const width=value.codePointAt(i)>0xffff?2:1;offset+=width;i+=width;}return value.length;};let cursor=0;matches.forEach(match=>{const start=index(content,Math.max(cursor,Number(match.start)||0)),end=index(content,Math.max(Number(match.end)||0,Number(match.start)||0));if(end<=start)return;pre.append(document.createTextNode(content.slice(cursor,start)));const mark=document.createElement("mark");mark.textContent=content.slice(start,end);pre.append(mark);cursor=end;});pre.append(document.createTextNode(content.slice(cursor)));container.append(pre); }
    async function loadPreview(path, root) { state.selected=path;state.selectedRoot=root||"";document.querySelectorAll("[data-path]").forEach(el=>el.setAttribute("aria-current",String(el.dataset.path===path && (el.dataset.root||"")===(root||""))));$("download").disabled=false;$("reveal").disabled=false;$("open").disabled=false;$("preview").replaceChildren();const loading=document.createElement("div");loading.className="state busy";loading.textContent="미리보기 불러오는 중";$("preview").append(loading);try{const data=await api("/api/preview",{path,q:state.query,...(root?{root}:{})});previewText(data);showPreviewDialog(data);}catch(error){$("preview").replaceChildren();const box=document.createElement("div");box.className="state";const strong=document.createElement("strong");strong.textContent=isTimeout(error)?"미리보기 시간 초과":"미리보기 실패";box.append(strong,document.createTextNode(`${errorMessage(error)} 파일을 다시 선택해 보세요.`));$("preview").append(box);showError(error);} }
    function showPreviewDialog(data) { const body=$("preview-dialog-body"); body.replaceChildren(); const meta=document.createElement("p"); meta.className="preview-meta"; meta.textContent=`${data.path||""} · ${data.renderer||"text"}`; body.append(meta); const binaryUrl=data.binary_url||`/api/preview/file?${params({path:data.path||state.selected,...(state.selectedRoot?{root:state.selectedRoot}:{})})}`; if(data.media_type && !(data.supported!==false && data.content)){const frame=document.createElement(String(data.media_type).startsWith("image/")?"img":"object");frame.src=binaryUrl; if(frame.tagName==="OBJECT")frame.type=data.media_type; frame.setAttribute("aria-label",`${data.path||"파일"} 미리보기`);body.append(frame);} else if(data.supported!==false && data.content){const pre=document.createElement("pre");pre.textContent=data.content;body.append(pre);} else {const note=document.createElement("div");note.className="state";note.textContent=data.reason||"사용할 수 있는 미리보기 렌더러가 없습니다.";body.append(note);} $("preview-dialog").showModal(); }
    async function loadScenarios() { try { const data=await api("/api/scenarios"); const select=$("scenario-select"); select.replaceChildren(); (data.scenarios||[]).forEach(item=>{const option=document.createElement("option");option.value=item.id;option.textContent=`${item.title} — ${item.purpose}`;option.dataset.query=item.query||"";option.dataset.extension=item.extension||"";select.append(option);}); setText("scenario-count",`${data.count||0}개 시나리오`); } catch(error) { showError(error); } }
    $("preview-dialog-close").addEventListener("click",()=>$("preview-dialog").close());
    $("scenario-run").addEventListener("click",runScenario);
    loadScenarios();
    async function runScenario() { const option=$("scenario-select").selectedOptions[0]; if(!option)return; const button=$("scenario-run"); button.disabled=true;button.classList.add("busy");state.query=option.dataset.query||"";$("query").value=state.query;listState($("results"),"시나리오 실행 중","선택한 직지 Find 경로를 실행합니다…");try{renderResults(await api("/api/scenario-run",findOptions({id:option.value,query:state.query,extension:option.dataset.extension||undefined})));toast(`시나리오 완료: ${option.textContent}`);}catch(error){showError(error);}finally{button.disabled=false;button.classList.remove("busy");} }
    function sortResults(items) { const mode=$("result-sort")?.value||"relevance"; return items.slice().sort((a,b)=>mode==="name"?String(a.name||a.path).localeCompare(String(b.name||b.path)):mode==="path"?String(a.path).localeCompare(String(b.path)):mode==="scope"?String(a.scope||a.status||"").localeCompare(String(b.scope||b.status||"")):Number(b.score||b.s||0)-Number(a.score||a.s||0)); }
    function renderResults(data) { const candidates=sortResults(Array.isArray(data.candidates)?data.candidates:[]);const list=$("results");list.replaceChildren();setText("confidence",data.confidence?`${data.confidence} 신뢰도`:"");setText("results-meta",`“${state.query}” 결과 ${candidates.length}건`);if(!candidates.length){listState(list,"인덱스된 일치 없음","더 짧은 구문, 파일명 조각, 또는 재인덱싱을 시도하세요.");return;}candidates.forEach((item,index)=>{const li=document.createElement("li"),button=document.createElement("button"),score=document.createElement("span"),title=document.createElement("strong"),path=document.createElement("div"),evidence=document.createElement("div");button.type="button";button.className="result";button.dataset.path=item.path||item.p||"";button.dataset.root=item.root||"";score.className="score";score.textContent=Number.isFinite(Number(item.score))?Number(item.score).toFixed(2):`#${index+1}`;title.textContent=item.name||(item.path||item.p||"").split("/").pop();path.className="path";path.textContent=item.root?`${(String(item.root).endsWith("/")?String(item.root).slice(0,-1):String(item.root))}/${item.path||item.p||""}`:(item.path||item.p||"");evidence.className="evidence";evidence.textContent=item.preview_snippet||(Array.isArray(item.evidence)?item.evidence[0]:"")||(Array.isArray(item.reasons)?item.reasons.join(" · "):"");button.append(score,title,path,evidence);button.addEventListener("click",()=>loadPreview(item.path||item.p,item.root));li.append(button);list.append(li);}); }
    function indexedFolderFor(path) { return path ? path.split("/").slice(0,-1).join("/") : ""; }
    async function find(event) { event.preventDefault(); const q=$("query").value.trim(); if(!q) { state.query=""; clearError(); setText("results-meta",""); setText("confidence",""); listState($("results"),"검색어를 입력하세요","파일명, 주제, 사람 이름을 입력하세요."); return; } state.query=q; clearError(); const button=$("search-button"); button.disabled=true; button.classList.add("busy"); listState($("results"),"찾는 중","인덱스된 문서 폴더를 검색합니다…"); try { renderResults(await api("/api/find",findOptions({q}))); } catch(error) { listState($("results"),isTimeout(error)?"찾기 시간 초과":"찾기에 실패했습니다",`${errorMessage(error)} 다시 검색해 보세요.`); showError(error); } finally { button.disabled=false;button.classList.remove("busy"); } }
    $("result-sort").addEventListener("change",()=>{if(state.query)$("search-form").dispatchEvent(new Event("submit",{cancelable:true}));});
    $("search-form").addEventListener("submit", find);
    $("root-select").addEventListener("change", switchRoot);
    async function switchRoot() { const path=$("root-select").value; if(!path)return; try { const data=await api("/api/root",{path,token:token()},{method:"POST"}); state.root=data.root||path;state.folder="";state.selected="";updateStats(data);await loadFiles();toast("활성 루트를 바꿨습니다."); } catch(error){showError(error);} }
    function confirmAction(title,copy,label,action){setText("confirm-title",title);setText("confirm-copy",copy);setText("confirm-ok",label);state.confirmAction=action;$("confirm-dialog").showModal();}
    $("add-root").addEventListener("click",()=>{const path=window.prompt("추가할 절대 폴더 경로를 입력하세요"); if(path) mutation("/api/root",{path},"add-root");});
    async function waitForJob(jobId) { state.currentJob=jobId; $("cancel-job").disabled=false; for (let attempt=0; attempt<120; attempt++) { const job=await api(`/api/jobs/${encodeURIComponent(jobId)}`); setText("job-status",`${job.state} · ${job.progress}%`); if (job.state === "completed") { state.currentJob=""; $("cancel-job").disabled=true; return job.result || job; } if (job.state === "failed") throw new Error(job.error?.message || "인덱스 작업 실패"); if (job.state === "cancelled") throw new Error("인덱스 작업이 취소되었습니다."); await new Promise(resolve=>setTimeout(resolve,1000)); } throw new Error("인덱스 작업 상태 확인이 시간 초과되었습니다. 다시 시도하세요."); }
    async function mutation(path, values, label) { clearError(); const button=$(label); button.disabled=true;button.classList.add("busy");try{let data=await api(path,{...values,token:token()},{method:"POST"});if(data.job_id)data=await waitForJob(data.job_id);if(data.prepared!==undefined)updateStats(data);await Promise.all([loadRoots(),loadStatus(),loadFiles(state.folder),loadIndexed(indexedFolderFor(state.selected))]);toast(`${button.textContent.trim()} 완료.`);}catch(error){setText("health","오류");showError(error);}finally{button.disabled=false;button.classList.remove("busy");} }
    $("refresh").addEventListener("click",()=>mutation("/api/refresh",{async:true,...prepareFlags()},"refresh"));
    async function cancelJob() { if(!state.currentJob)return; try { await api(`/api/jobs/${encodeURIComponent(state.currentJob)}/cancel`,{}, {method:"POST",query:`token=${encodeURIComponent(token())}`}); setText("job-status","cancelled"); state.currentJob=""; $("cancel-job").disabled=true; } catch(error) { showError(error); } }
    $("cancel-job").addEventListener("click",cancelJob);
    async function indexSelection(mode) { clearError(); const paths=selectedPaths(); if(!paths.length)return; const query=new URLSearchParams({mode,token:token()}); paths.forEach(path=>query.append("path",path)); const button=$(mode==="content"?"index-content":"index-basic"); button.disabled=true;button.classList.add("busy");try{let data=await api("/api/index-selection",{}, {method:"POST",query:query.toString()});if(data.job_id)data=await waitForJob(data.job_id);state.selectedPaths.clear();updateSelectionButtons();await Promise.all([loadStatus(),loadFiles(state.folder),loadIndexed(indexedFolderFor(state.selected))]);toast(`${mode} 인덱싱이 완료되었습니다.`);}catch(error){setText("health","오류");showError(error);}finally{button.disabled=false;button.classList.remove("busy");} }
    $("index-basic").addEventListener("click",()=>indexSelection("basic"));
    $("index-content").addEventListener("click",()=>indexSelection("content"));
    updateSelectionButtons();
    $("view-filter").addEventListener("change",()=>loadFiles(state.folder));
    $("view-sort").addEventListener("change",()=>loadFiles(state.folder));
    $("reindex-folder").addEventListener("click",()=>mutation("/api/reindex-folder",{path:state.folder,...prepareFlags()},"reindex-folder"));
    $("deep-index").addEventListener("click",()=>mutation("/api/deep-index",{...deepOptions(),async:true},"deep-index"));
    $("deep-target-enable").addEventListener("click",()=>mutation("/api/deep-index-target",{path:state.folder,enabled:true},"deep-target-enable"));
    $("deep-target-disable").addEventListener("click",()=>mutation("/api/deep-index-target",{path:state.folder,enabled:false},"deep-target-disable"));
    $("remove-folder").addEventListener("click",()=>confirmAction("인덱스된 폴더를 제거할까요?",`${state.folder || "/"} 경로를 직지 인덱스에서 제거합니다. 원본 파일은 삭제하지 않습니다.`,"폴더 제거",()=>mutation("/api/remove-folder",{path:state.folder},"remove-folder")));
    $("remove-root").addEventListener("click",()=>confirmAction("인덱스된 루트를 제거할까요?",`${state.root} 경로를 직지 중앙 인덱스에서 제거합니다. 원본 파일은 삭제하지 않습니다.`,"루트 제거",()=>mutation("/api/remove-root",{path:state.root},"remove-root")));
    $("confirm-cancel").addEventListener("click",()=>$('confirm-dialog').close()); $("confirm-ok").addEventListener("click",()=>{const action=state.confirmAction;$("confirm-dialog").close();state.confirmAction=null;if(action)action();});
    $("download").addEventListener("click",()=>{if(state.selected)location.assign(`/download?${params({path:state.selected,...(state.selectedRoot?{root:state.selectedRoot}:{})})}`);});
    $("reveal").addEventListener("click",async()=>{try{await api("/reveal",{path:state.selected,...(state.selectedRoot?{root:state.selectedRoot}:{}),token:token()},{method:"POST"});toast("파일 관리자에서 열었습니다.");}catch(error){showError(error);}});
    function showOps(title, payload){ setText("ops-dialog-title", title); $("ops-dialog-body").textContent = typeof payload === "string" ? payload : (payload && payload.markdown) ? payload.markdown : JSON.stringify(payload, null, 2); $("ops-dialog").showModal(); }
    async function runOps(id, path, values={}, options={}){ clearError(); const button=$(id); button.disabled=true; button.classList.add("busy"); try { const payload=options.method==="POST"?{...values,token:token()}:values; const data=await api(path, payload, options); showOps(button.textContent.trim(), data); } catch(error){ showError(error); } finally { button.disabled=false; button.classList.remove("busy"); } }
    $("ops-dialog-close").addEventListener("click",()=>$("ops-dialog").close());
    $("ops-doctor").addEventListener("click",()=>runOps("ops-doctor","/api/doctor"));
    $("ops-map").addEventListener("click",()=>runOps("ops-map","/api/map"));
    $("ops-graph").addEventListener("click",()=>runOps("ops-graph","/api/graph",{command:"status"}));
    $("ops-graph-query").addEventListener("click",()=>{const q=state.query||$("query").value.trim();if(!q){showError(new Error("Graph 검색에는 검색어가 필요합니다."));return;}runOps("ops-graph-query","/api/graph",{command:"query",q,top_k:findTopK()});});
    $("ops-graph-explain").addEventListener("click",()=>{if(!state.selected){showError(new Error("설명할 파일을 먼저 선택하세요."));return;}runOps("ops-graph-explain","/api/graph",{command:"explain",path:state.selected});});
    $("ops-brief").addEventListener("click",()=>{const q=state.query||$("query").value.trim(); if(!q){showError(new Error("Brief는 검색어가 필요합니다."));return;} runOps("ops-brief","/api/brief",findOptions({q}));});
    $("ops-clean-dry").addEventListener("click",()=>runOps("ops-clean-dry","/api/clean",{dry_run:1},{method:"POST"}));
    $("ops-clean").addEventListener("click",()=>confirmAction("Clean을 실행할까요?","인덱스를 정리합니다. 원본 파일은 삭제하지 않습니다.","Clean 실행",()=>runOps("ops-clean","/api/clean",{force:1},{method:"POST"})));
    $("reindex").addEventListener("click",()=>mutation("/api/reindex",{async:true,...prepareFlags()},"reindex"));
    $("open").addEventListener("click",async()=>{try{await api("/open",{path:state.selected,...(state.selectedRoot?{root:state.selectedRoot}:{}),token:token()},{method:"POST"});toast("원본 파일을 열었습니다.");}catch(error){showError(error);}});
    Promise.all([loadRoots(),loadStatus()]).then(()=>Promise.all([loadFiles(),loadIndexed()])).catch(error=>{setText("health","오류");showError(error);listState($("file-list"),isTimeout(error)?"직지 요청 시간 초과":"직지를 사용할 수 없습니다",`${errorMessage(error)} 새로고침으로 다시 시도하세요.`);});
    })();
  </script>
</body>
</html>"##;

pub(crate) fn run_gui(args: GuiArgs) -> jikji_core::Result<ExitCode> {
    if !is_loopback_host(&args.host) {
        return Err(invalid_input("GUI host must be loopback"));
    }
    if args.background && !args.serve_child {
        return spawn_background(args);
    }
    if args.prepare {
        prepare(&args.root, &PrepareOptions::default())?;
    }
    let root = args
        .root
        .canonicalize()
        .map_err(|source| jikji_core::io_error(&args.root, source))?;
    let token = match args.manage_token {
        Some(value) => ManagementToken::new(value),
        None => ManagementToken::generate()?,
    };
    // Warm the central SQLite BEFORE binding the port. This way, when the
    // hub ready check (TCP connect) succeeds, the expensive first open_database
    // + register_root (which can take 10s+ on a large index with 50k+ docs) has
    // already completed. Clients that connect immediately after "ready" will
    // see warm-DB response times (~1s for 18 roots) instead of queuing behind
    // the open.
    let _ = jikji_core::storage::open_database();
    let _ = jikji_core::storage::register_root(&root);
    let listener = TcpListener::bind((args.host.as_str(), args.port))
        .map_err(|source| jikji_core::io_error("<gui-bind>", source))?;
    let address = listener
        .local_addr()
        .map_err(|source| jikji_core::io_error("<gui-addr>", source))?;
    let url = format!("http://{}:{}", address.ip(), address.port());
    if args.json && !args.serve_child {
        print_json(&json!({
            "url": url,
            "root": root,
            "background": false,
            "manage_token": token.as_str()
        }))?;
    } else if !args.serve_child {
        println!("Jikji GUI: {url}");
    }
    serve_loop(listener, GuiState::new(root, token))
}

fn spawn_background(args: GuiArgs) -> jikji_core::Result<ExitCode> {
    let port = if args.port == 0 {
        reserve_loopback_port(&args.host)?
    } else {
        args.port
    };
    let token = ManagementToken::generate()?;
    let exe =
        std::env::current_exe().map_err(|source| jikji_core::io_error("<current-exe>", source))?;
    let mut command = Command::new(exe);
    command
        .arg("gui")
        .arg(&args.root)
        .arg("--host")
        .arg(&args.host)
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-open")
        .arg("--serve-child")
        .arg("--manage-token")
        .arg(token.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if args.prepare {
        command.arg("--prepare");
    }
    let child = command
        .spawn()
        .map_err(|source| jikji_core::io_error("<gui-spawn>", source))?;
    let url = format!("http://{}:{port}", args.host);
    wait_until_ready(&args.host, port)?;
    let payload = json!({
        "url": url,
        "pid": child.id(),
        "root": args.root,
        "background": true,
        "manage_token": token.as_str(),
        "cleanup": cleanup_command(child.id()),
    });
    if args.json {
        print_json(&payload)?;
    } else {
        println!("{url}");
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(windows)]
fn cleanup_command(pid: u32) -> String {
    format!("taskkill /PID {pid} /F /T")
}

#[cfg(not(windows))]
fn cleanup_command(pid: u32) -> String {
    format!("kill {pid}")
}

fn serve_loop(listener: TcpListener, state: GuiState) -> jikji_core::Result<ExitCode> {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let request_state = state.clone();
                thread::spawn(move || {
                    let _ = handle_stream(stream, &request_state);
                });
            }
            Err(source) => return Err(jikji_core::io_error("<gui-accept>", source)),
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn handle_stream(stream: TcpStream, state: &GuiState) -> jikji_core::Result<()> {
    let request = http::HttpRequest::read(&stream)?;
    let response = route_request(state, &request, INDEX_HTML);
    http::write_response(stream, &response)
}

fn reserve_loopback_port(host: &str) -> jikji_core::Result<u16> {
    let listener = TcpListener::bind((host, 0))
        .map_err(|source| jikji_core::io_error("<gui-port>", source))?;
    let port = listener
        .local_addr()
        .map_err(|source| jikji_core::io_error("<gui-port>", source))?
        .port();
    drop(listener);
    Ok(port)
}

fn wait_until_ready(host: &str, port: u16) -> jikji_core::Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if TcpStream::connect((host, port)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(invalid_input("GUI child did not become ready"))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn invalid_input(message: impl Into<String>) -> jikji_core::JikjiError {
    jikji_core::io_error(
        "<gui>",
        std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::INDEX_HTML;

    #[test]
    fn index_html_has_semantic_three_pane_shell_and_focusable_controls() {
        for landmark in ["<header", "<main", "<nav", "<section", "<aside", "<footer"] {
            assert!(
                INDEX_HTML.contains(landmark),
                "missing landmark: {landmark}"
            );
        }
        for control in [
            "id=\"search-form\"",
            "id=\"root-select\"",
            "id=\"folder-context\"",
            "id=\"indexed-list\"",
            "id=\"indexed-count\"",
            "id=\"index-basic\"",
            "id=\"index-content\"",
            "id=\"cancel-job\"",
            "id=\"refresh\"",
            "id=\"reindex\"",
            "id=\"reindex-folder\"",
            "id=\"deep-index\"",
            "id=\"deep-target-enable\"",
            "id=\"deep-target-disable\"",
            "id=\"remove-folder\"",
            "id=\"remove-root\"",
            "id=\"download\"",
            "id=\"reveal\"",
            "id=\"open\"",
            "id=\"confirm-dialog\"",
            "id=\"preview-dialog\"",
            "id=\"scenario-select\"",
            "id=\"find-top-k\"",
            "id=\"find-fresh\"",
            "id=\"find-exclude\"",
            "id=\"find-retry-proof\"",
            "id=\"find-stale-after\"",
            "id=\"find-no-background-refresh\"",
            "id=\"find-max-files\"",
            "id=\"find-parse-timeout\"",
            "id=\"find-first\"",
            "id=\"find-auto-prepare\"",
            "id=\"find-no-auto-prepare\"",
            "id=\"find-max-hash-bytes\"",
            "id=\"ops-doctor\"",
            "id=\"ops-graph-query\"",
            "id=\"ops-graph-explain\"",
            "id=\"ops-dialog\"",
            "파일 탐색",
            "인덱스 범위",
            "로컬 폴더를 탐색합니다",
            "검색은 이 목록과 인덱싱된 내용만 대상으로 합니다",
        ] {
            assert!(INDEX_HTML.contains(control), "missing control: {control}");
        }
        assert!(INDEX_HTML.contains(":focus-visible"));
        assert!(INDEX_HTML.contains("min-height: 40px"));
        assert!(INDEX_HTML.contains("@media (max-width: 640px)"));
    }

    #[test]
    fn index_html_calls_central_gui_routes_and_keeps_mutation_token_out_of_markup() {
        for route in [
            "/api/status",
            "/api/roots",
            "/api/files",
            "/api/indexed-files",
            "/api/index-selection",
            "/api/jobs/",
            "/cancel",
            "/api/find",
            "/api/preview",
            "/api/preview/file",
            "/api/scenario-run",
            "/api/doctor",
            "/api/map",
            "/api/graph",
            "/api/brief",
            "/api/clean",
            "/api/root",
            "/api/refresh",
            "/api/reindex",
            "/api/reindex-folder",
            "/api/deep-index",
            "/api/deep-index-target",
            "/api/remove-folder",
            "/api/remove-root",
            "/download",
            "/reveal",
            "/open",
        ] {
            assert!(INDEX_HTML.contains(route), "missing route: {route}");
        }
        for state in [
            "요청이 시간 초과되었습니다",
            "찾기에 실패했습니다",
            "폴더가 비어 있습니다",
            "현재 폴더:",
            "인덱스된 파일이 없습니다",
        ] {
            assert!(INDEX_HTML.contains(state), "missing state copy: {state}");
        }
        assert!(INDEX_HTML.contains("token:token()"));
        assert!(INDEX_HTML.contains("textContent"));
        assert!(!INDEX_HTML.contains("innerHTML"));
        assert!(!INDEX_HTML.contains("localStorage"));
        assert!(INDEX_HTML.contains("showModal"));
        assert!(
            INDEX_HTML.contains("endsWith(\"/\")"),
            "root display must avoid /.../ regex literals; MarkerAI rewrites (/ into invalid flags"
        );
        assert!(INDEX_HTML.contains("fresh:checkedFlag(\"find-fresh\")"));
        assert!(INDEX_HTML.contains("retry_proof:textValue(\"find-retry-proof\")"));
        assert!(INDEX_HTML.contains("stale_after_seconds:numberValue(\"find-stale-after\")"));
        assert!(INDEX_HTML.contains("first:checkedFlag(\"find-first\")"));
        assert!(INDEX_HTML.contains("auto_prepare:checkedFlag(\"find-auto-prepare\")"));
        assert!(INDEX_HTML.contains("no_auto_prepare:checkedFlag(\"find-no-auto-prepare\")"));
        assert!(INDEX_HTML.contains("max_hash_bytes:numberValue(\"find-max-hash-bytes\")"));
        assert!(
            !INDEX_HTML.contains("replace(/\\/$/"),
            "trailing-slash regex after ( is rewritten to invalid flags by /jikji prefixing"
        );
    }
}
