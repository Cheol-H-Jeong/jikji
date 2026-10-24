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

const INDEX_HTML: &str = r###"<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>Jikji · 로컬 파일 찾기</title>
<style>
:root{--bg:#f3f5f8;--surface:#fff;--ink:#172033;--muted:#667085;--line:#dfe5ec;--soft:#eef2f6;--accent:#135f8a;--accent-strong:#0c496b;--accent-soft:#e7f1f7;--good:#16764a;--warn:#8c5700;--danger:#b7352b;--shadow:0 2px 10px #17203312;font-family:ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;color:var(--ink);background:var(--bg)}
*{box-sizing:border-box}body{margin:0;min-width:320px;line-height:1.5}button,input,select{font:inherit;color:inherit}button,input,select{min-height:40px}button{border:1px solid var(--line);border-radius:6px;background:var(--surface);padding:8px 12px;cursor:pointer;font-weight:650;transition:border-color .15s ease,background .15s ease}button:hover{border-color:var(--accent);color:var(--accent-strong)}button.primary{background:var(--accent);border-color:var(--accent);color:#fff}button.primary:hover{background:var(--accent-strong);color:#fff}button.danger{color:var(--danger)}button:disabled{opacity:.5;cursor:not-allowed}input,select{width:100%;border:1px solid var(--line);border-radius:6px;background:var(--surface);padding:8px 10px}:focus-visible{outline:3px solid #f0a51a;outline-offset:2px}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}
.topbar{position:sticky;top:0;z-index:5;display:flex;align-items:center;gap:16px;padding:12px clamp(16px,3vw,40px);background:#fff;border-bottom:1px solid var(--line)}.brand{min-width:170px}.brand strong{font-size:21px;letter-spacing:-.04em}.brand span{display:block;color:var(--muted);font-size:12px}.search-form{display:flex;gap:8px;flex:1;max-width:760px}.search-form input{font-size:16px}.token-wrap{margin-left:auto;min-width:160px}
.shell{width:min(1480px,100%);margin:auto;padding:24px clamp(16px,3vw,40px) 40px}.error{padding:10px 14px;margin-bottom:16px;border:1px solid #efb8b4;border-radius:6px;color:var(--danger);background:#fff5f4}.hero{display:flex;justify-content:space-between;align-items:end;gap:20px;margin-bottom:18px}.eyebrow{color:var(--muted);font-size:11px;letter-spacing:.06em;text-transform:uppercase}.hero h1{margin:2px 0 4px;font-size:clamp(24px,3vw,34px);letter-spacing:-.04em}.hero p{margin:0;color:var(--muted)}.hero-actions,.toolbar,.controls{display:flex;flex-wrap:wrap;align-items:center;gap:8px}.stats{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px;margin-bottom:18px}.stat{min-width:0;padding:13px 14px;border:1px solid var(--line);border-radius:6px;background:var(--surface);box-shadow:var(--shadow)}.stat b{display:block;margin-top:3px;overflow-wrap:anywhere}.status-bar{display:flex;justify-content:space-between;gap:12px;padding-top:16px;color:var(--muted);font-size:12px}
.tabs{display:flex;gap:4px;border-bottom:1px solid var(--line);margin-bottom:16px}.tab{border:0;border-radius:6px 6px 0 0;background:transparent;color:var(--muted)}.tab[aria-selected=true]{color:var(--accent-strong);background:var(--surface);box-shadow:0 -2px 0 var(--accent) inset}.tab-panel[hidden]{display:none}.workspace{display:grid;grid-template-columns:minmax(230px,.8fr) minmax(300px,1fr) minmax(320px,1.2fr);gap:14px;align-items:start}.pane{min-width:0;background:var(--surface);border:1px solid var(--line);border-radius:6px;box-shadow:var(--shadow);overflow:hidden}.pane-head{display:flex;align-items:center;gap:8px;min-height:52px;padding:9px 14px;border-bottom:1px solid var(--line)}.pane-head h2{margin:0;font-size:15px}.spacer{flex:1}.pane-note{margin:0;padding:10px 14px;color:var(--muted);font-size:12px;border-bottom:1px solid var(--line);overflow-wrap:anywhere}.controls{padding:10px 14px}.controls>*{min-width:0}.list{list-style:none;margin:0;padding:6px}.file-row,.result{display:flex;align-items:center;gap:8px;width:100%;min-height:44px;border:0;border-bottom:1px solid var(--soft);border-radius:4px;background:transparent;text-align:left;padding:9px 8px;overflow:hidden}.file-row:hover,.result:hover{background:var(--accent-soft)}.file-row>*:not(input),.result>*{min-width:0}.file-row .name,.result .name{flex:1;overflow-wrap:anywhere;word-break:break-word}.file-row .meta{color:var(--muted);font-size:12px;white-space:nowrap}.result{display:grid;grid-template-columns:auto minmax(0,1fr);align-items:start}.result .score{grid-row:span 2;color:var(--accent);font-size:12px}.result strong,.result .path,.result .evidence{min-width:0;overflow-wrap:anywhere;word-break:break-word}.result .path,.result .evidence{grid-column:2;color:var(--muted);font-size:12px}.state{padding:28px 16px;text-align:center;color:var(--muted)}.preview-body{min-height:280px;max-height:620px;overflow:auto;padding:14px}.preview-body pre{margin:0;max-width:100%;white-space:pre-wrap;overflow-wrap:anywhere;font:13px/1.7 ui-monospace,SFMono-Regular,monospace}.preview-meta{margin-bottom:10px;padding:9px;border-radius:4px;background:var(--soft);color:var(--muted);font-size:12px;overflow-wrap:anywhere}.preview-body object{width:100%;height:520px;border:0}.results-meta{padding:9px 14px;color:var(--muted);font-size:12px;border-bottom:1px solid var(--line)}
.filter-grid,.advanced-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;padding:12px 14px}.filter-grid label,.advanced-grid label{display:flex;align-items:center;gap:7px;color:var(--muted);font-size:13px;min-width:0}.filter-grid label:not(:has(input[type=checkbox])),.advanced-grid label:not(:has(input[type=checkbox])){display:block}.filter-grid input[type=checkbox],.advanced-grid input[type=checkbox]{width:18px;min-width:18px;height:18px}.panel-box{margin-top:14px;border:1px solid var(--line);border-radius:6px;background:var(--surface)}.panel-box>h2{font-size:15px;margin:0;padding:12px 14px;border-bottom:1px solid var(--line)}details{margin-top:14px;border:1px solid var(--line);border-radius:6px;background:var(--surface)}summary{cursor:pointer;padding:12px 14px;font-weight:700}details[open] summary{border-bottom:1px solid var(--line)}.toast{position:fixed;right:20px;bottom:20px;z-index:9;max-width:min(360px,calc(100vw - 32px));padding:12px 16px;border-radius:6px;color:#fff;background:var(--ink);box-shadow:var(--shadow)}dialog{width:min(620px,calc(100vw - 24px));max-height:calc(100vh - 24px);padding:0;border:1px solid var(--line);border-radius:8px;box-shadow:0 24px 70px #0003}dialog::backdrop{background:#0f172a66}dialog>h2,dialog>p,dialog>menu{margin:0;padding:14px}dialog>menu{display:flex;justify-content:flex-end;gap:8px;border-top:1px solid var(--line)}.preview-dialog-body{max-height:70vh;overflow:auto;padding:14px;white-space:pre-wrap;overflow-wrap:anywhere}.preview-dialog-body pre{white-space:pre-wrap;overflow-wrap:anywhere}
@media(max-width:1050px){.topbar{flex-wrap:wrap}.search-form{order:3;flex-basis:100%;max-width:none}.token-wrap{margin-left:auto}.workspace{grid-template-columns:minmax(210px,.8fr) minmax(0,1fr)}.preview-pane{grid-column:1/-1}.stats{grid-template-columns:repeat(2,minmax(0,1fr))}}
@media(max-width:680px){.shell{padding:16px 12px 28px}.topbar{padding:12px}.brand{min-width:0}.brand span{display:none}.token-wrap{flex:1;min-width:120px}.hero{display:block}.hero-actions{margin-top:12px}.tabs{overflow:auto}.workspace{display:block}.pane{margin-bottom:12px}.stats{gap:8px}.filter-grid,.advanced-grid{grid-template-columns:1fr}.status-bar{display:block}.status-bar>*{display:block;margin-top:4px}}
@media(prefers-reduced-motion:reduce){*{scroll-behavior:auto!important;transition:none!important}}
input[type=checkbox]{width:18px;min-width:18px;min-height:18px;height:18px;flex:0 0 18px}.workspace{grid-template-columns:minmax(0,1fr) minmax(0,1fr);margin-top:16px}.pane-head{flex-wrap:wrap}.pane{overflow:visible}.file-row{overflow:visible}.search-form input{min-width:0}.controls>select{max-width:100%}.topbar>*{min-width:0}.file-row .name{min-width:0}.file-row .meta{white-space:normal}.file-row input{flex-shrink:0}.stats{box-shadow:none}.stat{box-shadow:none}.state strong{display:block}#folder-context{overflow-wrap:anywhere;flex-basis:100%}.filter-grid label,.advanced-grid label{overflow-wrap:anywhere}@media(max-width:800px){.workspace{grid-template-columns:minmax(0,1fr)}}@media(max-width:680px){.workspace{display:block}.pane-head button{flex:1}.topbar{flex-wrap:wrap}}
.file-item{display:flex;align-items:center;gap:8px}.file-item>input{margin:12px}.file-item .file-row{flex:1;min-width:0}.preview-body object{display:block;width:100%;height:480px;border:0}.preview-body img{display:block;max-width:100%;height:auto;margin:0 auto}[hidden]{display:none!important}</style></head><body>
<header class="topbar"><div class="brand"><strong>Jikji</strong><span>로컬 인덱스 운영 콘솔</span></div><form class="search-form" id="search-form" role="search"><label class="sr-only" for="query">인덱스된 파일 찾기</label><input id="query" name="q" type="search" placeholder="계약서, 메모, 사람 이름 검색" autocomplete="off"><button class="primary" id="search-button" type="submit">검색</button></form><div class="token-wrap"><label class="sr-only" for="manage-token">관리 토큰</label><input id="manage-token" type="password" autocomplete="off" placeholder="관리 토큰"></div></header>
<main class="shell"><div id="global-error" class="error" role="alert" hidden></div><div class="hero"><div><span class="eyebrow">LOCAL INDEX CONTROL CENTER</span><h1>파일을 찾고, 인덱스를 운영합니다</h1><p>검색을 중심으로 필요한 운영 기능만 단계적으로 표시합니다.</p></div><div class="hero-actions"><button id="refresh" type="button">새로고침</button><button id="settings-open" type="button">설정</button><button id="add-root" type="button">루트 추가</button></div></div>
<section class="stats" aria-label="인덱스 상태"><div class="stat"><span class="eyebrow">상태</span><b id="health">불러오는 중…</b></div><div class="stat"><span class="eyebrow">인덱스 파일</span><b id="file-count">—</b></div><div class="stat"><span class="eyebrow">루트 용량</span><b id="root-size">—</b></div><div class="stat"><span class="eyebrow">마지막 인덱싱</span><b id="last-indexed">—</b></div></section>
<nav class="tabs" role="tablist" aria-label="콘솔 영역"><button class="tab" id="tab-search" role="tab" aria-selected="true" aria-controls="panel-search">검색</button><button class="tab" id="tab-index" role="tab" aria-selected="false" aria-controls="panel-index">인덱스 관리</button><button class="tab" id="tab-ops" role="tab" aria-selected="false" aria-controls="panel-ops">고급 도구</button></nav>
<section id="panel-search" class="tab-panel" role="tabpanel" aria-labelledby="tab-search"><details id="search-options"><summary>검색 필터와 정렬</summary><div class="filter-grid"><label>결과 수<select id="find-top-k"><option>10</option><option selected>20</option><option>50</option></select></label><label>확장자<input id="find-extension" type="text" placeholder="pdf, hwp"></label><label>그룹<select id="result-group"><option value="none">그룹 없음</option><option value="extension">확장자</option><option value="folder">폴더</option></select></label><label>정렬<select id="result-sort"><option value="relevance">관련도</option><option value="name">이름</option><option value="extension">확장자</option></select></label><label><input id="include-hidden" type="checkbox"> 숨김 파일 포함</label><label><input id="include-sensitive" type="checkbox"> 민감 파일 포함</label><label><input id="after-jikji-retry" type="checkbox"> 재시도 후 검색</label><label><input id="find-fresh" type="checkbox"> 최신 준비</label><label><input id="find-no-background-refresh" type="checkbox"> 백그라운드 갱신 안 함</label><label>제외<input id="find-exclude" type="text" placeholder="glob 패턴"></label><label>최대 파일 수<input id="find-max-files" type="number" min="1"></label><label>파싱 제한<input id="find-parse-timeout" type="number" min=".1"></label><label><input id="find-first" type="checkbox"> 첫 결과만</label><label><input id="find-auto-prepare" type="checkbox"> 자동 준비</label><label><input id="find-no-auto-prepare" type="checkbox"> 자동 준비 안 함</label><label>해시 바이트<input id="find-max-hash-bytes" type="number" min="1"></label><input id="retry-proof" type="hidden"><input id="find-stale-after" type="hidden"></div></details><div class="workspace"><section class="pane results-pane" aria-labelledby="results-title"><div class="pane-head"><h2 id="results-title">검색 결과</h2><span class="spacer"></span><span class="eyebrow" id="confidence"></span></div><div id="results-meta" class="results-meta">검색어를 입력하세요</div><ol class="list" id="results"><li class="state">파일명, 주제, 사람 이름을 검색하세요</li></ol></section><aside class="pane preview-pane" aria-labelledby="preview-title"><div class="pane-head"><h2 id="preview-title">미리보기</h2><span class="spacer"></span><button id="download" type="button" disabled>다운로드</button><button id="reveal" type="button" disabled>폴더 열기</button><button id="open" type="button" disabled>원본 열기</button></div><div class="preview-body" id="preview"><div class="state">검색 결과나 파일을 선택하세요</div></div></aside></div></section>
<section id="panel-index" class="tab-panel" role="tabpanel" aria-labelledby="tab-index" hidden><div class="workspace"><nav class="pane explorer-pane" aria-labelledby="explorer-title"><div class="pane-head"><h2 id="explorer-title">파일 탐색</h2><span class="spacer"></span><span class="eyebrow" id="folder-context">/</span></div><p class="pane-note" id="tree-path">루트 /</p><div class="controls"><label class="sr-only" for="root-select">인덱스 루트</label><select id="root-select"><option>루트 불러오는 중…</option></select></div><div class="controls"><button id="up-folder" type="button">상위 폴더</button><button id="index-basic" type="button" disabled>기본 인덱싱</button><button id="index-content" type="button" disabled>내용 인덱싱</button><button id="exclude-selection" type="button">선택 제외</button><button id="include-selection" type="button">제외 해제</button></div><div class="controls"><label for="view-filter">보기</label><select id="view-filter"><option value="all">모든 파일</option><option value="unindexed">미인덱스</option><option value="content">내용 인덱스</option><option value="basic">기본 인덱스</option></select><label for="view-sort">정렬</label><select id="view-sort"><option value="name">이름</option><option value="size">크기</option><option value="mtime">수정일</option><option value="status">상태</option></select></div><ul class="list" id="file-list"><li class="state">파일을 불러오는 중</li></ul></nav><section class="pane indexed-pane" aria-labelledby="indexed-title"><div class="pane-head"><h2 id="indexed-title">인덱스 범위</h2><span class="spacer"></span><span class="eyebrow" id="indexed-count">—</span></div><p class="pane-note" id="indexed-path">/</p><div class="controls"><button id="up-indexed" type="button">상위 폴더</button></div><ul class="list" id="indexed-list"><li class="state">인덱스를 불러오는 중</li></ul></section></div></section>
<section id="panel-ops" class="tab-panel" role="tabpanel" aria-labelledby="tab-ops" hidden><div class="panel-box"><h2>현재 루트 작업</h2><div class="controls"><button id="reindex-toolbar" class="primary" type="button">루트 재인덱싱</button><button id="reindex-folder" type="button">현재 폴더 재인덱싱</button><button id="deep-index" type="button">루트 딥 인덱싱</button><button id="deep-target-enable" type="button">현재 폴더 딥 대상 등록</button><button id="deep-target-disable" type="button">딥 대상 해제</button><button id="remove-folder" class="danger" type="button">폴더 제거</button><button id="remove-root" class="danger" type="button">루트 제거</button><span id="job-status" class="eyebrow">대기</span><button id="cancel-job" type="button" disabled>작업 취소</button></div><p class="pane-note">주기적 스케줄러·클라우드 OAuth는 지원하지 않습니다. 동기화된 로컬 폴더를 루트로 추가하세요. 선택·폴더 인덱싱도 현재 서버에서는 전체 루트를 처리합니다. 딥 대상 등록은 대상 설정만 저장합니다.</p></div><details id="advanced-ops"><summary>진단·시나리오·정리 도구</summary><div class="controls"><select id="scenario-select"><option>시나리오 불러오는 중…</option></select><button id="scenario-run" type="button">시나리오 실행</button><span id="scenario-count" class="eyebrow">—</span><button id="ops-doctor" type="button">Doctor</button><button id="ops-map" type="button">Map</button><button id="ops-graph" type="button">Graph 상태</button><button id="ops-graph-query" type="button">Graph 검색</button><button id="ops-graph-explain" type="button">Graph 설명</button><button id="ops-brief" type="button">Brief</button><button id="ops-clean-dry" type="button">Clean 미리보기</button><button id="ops-clean" class="danger" type="button">Clean 실행</button></div><div class="controls"><label><input id="media-ocr" type="checkbox"> OCR</label><label><input id="media-asr" type="checkbox"> ASR</label><label>최대 MB<input id="media-max-mb" type="number" min="1"></label><label>최대 항목<input id="archive-max-entries" type="number" min="1"></label><label>항목 바이트<input id="archive-max-entry-bytes" type="number" min="1"></label><label>전체 바이트<input id="archive-max-total-bytes" type="number" min="1"></label></div><div class="controls"><span id="deep-state" class="eyebrow">실행 전</span><span id="deep-entries">—</span><span id="deep-bytes">—</span><span id="deep-time">—</span><span id="deep-cost">—</span><span id="deep-media">—</span></div></details></section>
<footer class="status-bar"><span>안전한 로컬 작업 · 원본 파일은 삭제하지 않습니다</span><span>관리 작업은 토큰으로 보호됩니다</span></footer></main><div class="toast" id="toast" role="status" aria-live="polite" hidden></div>
<dialog id="confirm-dialog" aria-labelledby="confirm-title" aria-describedby="confirm-copy"><h2 id="confirm-title">작업 확인</h2><p id="confirm-copy"></p><menu><button id="confirm-cancel" type="button">취소</button><button id="confirm-ok" class="danger" type="button">확인</button></menu></dialog><dialog id="settings-dialog" aria-labelledby="settings-title"><h2 id="settings-title">인덱싱 설정</h2><p class="pane-note">다음 인덱싱부터 적용됩니다. LLM과 주기적 스케줄러는 지원하지 않습니다.</p><div class="advanced-grid"><label>알고리즘<select id="settings-algorithm"><option value="basic">basic</option><option value="content">content</option><option value="deep">deep</option></select></label><label>확장자<input id="settings-extensions" type="text" placeholder="pdf,hwp,txt"></label><label>제외 패턴<input id="settings-exclude" type="text" placeholder="glob,glob"></label><label>최대 파일 바이트<input id="settings-max-file-bytes" type="number" min="1"></label><label>최대 파일 수<input id="settings-max-files" type="number" min="1"></label><label>최대 해시 바이트<input id="settings-max-hash-bytes" type="number" min="1" disabled title="현재 서버에서 변경을 지원하지 않습니다"></label><label>파싱 제한<input id="settings-parse-timeout" type="number" min=".1"></label><label>주기(초)<input id="settings-schedule" type="number" min="1" disabled placeholder="지원하지 않음"></label><label><input id="settings-include-hidden" type="checkbox"> 숨김 파일 포함</label><label><input id="settings-include-sensitive" type="checkbox"> 민감 파일 포함</label></div><menu><button id="settings-cancel" type="button">취소</button><button id="settings-save" class="primary" type="button">설정 저장</button></menu></dialog><dialog id="ops-dialog" aria-labelledby="ops-dialog-title"><h2 id="ops-dialog-title">운영 결과</h2><pre id="ops-dialog-body" class="preview-dialog-body"></pre><menu><button id="ops-dialog-close" type="button">닫기</button></menu></dialog>
<script>(()=>{"use strict";const $=id=>document.getElementById(id),state={root:"",folder:"",indexedFolder:"",selected:"",selectedRoot:"",selectedPaths:new Set(),query:"",roots:[],results:[],confirmAction:null,currentJob:""},number=new Intl.NumberFormat();
const setText=(id,v)=>$(id).textContent=v==null||v===""?"—":String(v),bytes=v=>{const n=Number(v);if(!Number.isFinite(n))return"—";const u=["B","KB","MB","GB","TB"];let i=0,x=n;while(x>=1024&&i<4){x/=1024;i++}return`${x>=10||i===0?x.toFixed(0):x.toFixed(1)} ${u[i]}`},date=v=>{if(!v)return"—";const d=new Date(typeof v==="number"&&v<1e12?v*1000:v);return Number.isNaN(d.valueOf())?String(v):d.toLocaleString()},showError=e=>{const x=$("global-error");x.textContent=e?.message||String(e);x.hidden=false},clearError=()=>$("global-error").hidden=true,toast=m=>{const x=$("toast");x.textContent=m;x.hidden=false;clearTimeout(toast.t);toast.t=setTimeout(()=>x.hidden=true,3200)},params=o=>{const p=new URLSearchParams;Object.entries(o).forEach(([k,v])=>{if(v===undefined||v===null||v==="")return;if(Array.isArray(v)){v.forEach(item=>p.append(k,String(item)))}else p.set(k,String(v))});return p},token=()=>{const v=$("manage-token").value.trim();if(!v){$("manage-token").focus();throw Error("관리 작업에는 GUI 관리 토큰이 필요합니다.")}return v},txt=id=>$(id)?.value?.trim()||undefined,num=id=>{const v=Number($(id)?.value);return Number.isFinite(v)&&$(id)?.value!==""?v:undefined},flag=id=>$(id)?.checked?1:undefined;
async function api(path,values={},opt={}){const c=new AbortController,t=setTimeout(()=>c.abort(),path.includes("/api/find")||path.includes("/api/preview")||opt.method==="POST"?120000:30000);try{const u=opt.query?`${path}?${opt.query}`:`${path}?${params(values)}`,r=await fetch(u,{method:opt.method||"GET",headers:{Accept:"application/json"},cache:"no-store",signal:c.signal}),ct=r.headers.get("content-type")||"",p=ct.includes("json")?await r.json():await r.text();if(!r.ok)throw Error(typeof p==="string"?p:p?.error_detail?.message||p?.error?.message||(typeof p?.error==="string"?p.error:"")||`요청 실패 (${r.status})`);return p}catch(e){if(e.name==="AbortError")throw Error("요청이 시간 초과되었습니다. 다시 시도하세요.");throw e}finally{clearTimeout(t)}}
function findOptions(extra={}){return{top_k:num("find-top-k")||20,extension:txt("find-extension"),include_hidden:flag("include-hidden"),include_sensitive:flag("include-sensitive"),after_jikji_retry:flag("after-jikji-retry"),fresh:flag("find-fresh"),no_background_refresh:flag("find-no-background-refresh"),exclude:txt("find-exclude"),retry_proof:txt("retry-proof"),stale_after_seconds:num("find-stale-after"),max_files:num("find-max-files"),parse_timeout:num("find-parse-timeout"),first:flag("find-first"),auto_prepare:flag("find-auto-prepare"),no_auto_prepare:flag("find-no-auto-prepare"),max_hash_bytes:num("find-max-hash-bytes"),...extra}}function listState(target,title,copy){target.replaceChildren();const li=document.createElement(/^(UL|OL)$/.test(target.tagName)?"li":"div"),s=document.createElement("strong");li.className="state";s.textContent=title;li.append(s,document.createTextNode(copy));target.append(li)}
function stats(p){const s=p.statistics||p.stats||{};const sig=p.manifest?.source_tree_signature||{};setText("health",p.prepared?"준비됨":p.root?"인덱싱 필요":"루트 없음");setText("file-count",number.format(s.files??s.file_count??sig.files??0));setText("root-size",bytes(sig.total_size??s.bytes??s.total_bytes));setText("last-indexed",date(p.manifest?.generated_at??s.updated_at))}
async function loadRoots(){const d=await api("/api/roots"),a=Array.isArray(d.roots)?d.roots:[];state.roots=a;state.root=d.active_root||state.root||a[0]?.root||"";const s=$("root-select");s.replaceChildren();a.forEach(x=>{const o=document.createElement("option");o.value=x.root;o.textContent=x.root;o.selected=x.root===state.root;s.append(o)});s.disabled=!a.length;if(!a.length){const o=document.createElement("option");o.textContent="인덱스된 루트 없음";s.append(o)}}async function loadStatus(){const d=await api("/api/status");state.root=d.root||state.root;stats(d)}
function selectedPaths(){return[...state.selectedPaths]}function updateSelection(){["index-basic","index-content"].forEach(id=>$(id).disabled=state.busy||!state.selectedPaths.size)}
async function loadFiles(folder=state.folder){state.folder=folder;setText("tree-path",`루트 /${folder?` · ${folder}`:""}`);setText("folder-context",folder||"/");const l=$("file-list");listState(l,"폴더 불러오는 중","로컬 파일을 읽는 중…");try{const d=await api("/api/files",{path:folder});let es=Array.isArray(d.entries)?d.entries:[],f=$("view-filter").value,s=$("view-sort").value;if(f!=="all")es=es.filter(e=>e.type==="directory"||(f==="unindexed"?e.scope==="unindexed":f==="content"?e.scope==="content":e.scope==="basic"));es.sort((a,b)=>s==="size"?(Number(b.size)||0)-(Number(a.size)||0):s==="mtime"?(Number(b.mtime)||0)-(Number(a.mtime)||0):s==="status"?String(a.scope||a.status).localeCompare(String(b.scope||b.status)):String(a.name||a.path).localeCompare(String(b.name||b.path)));l.replaceChildren();if(folder){const li=document.createElement("li"),b=document.createElement("button");b.type="button";b.className="file-row";b.textContent="↩ 상위 폴더";b.addEventListener("click",()=>loadFiles(folder.split("/").slice(0,-1).join("/")));li.append(b);l.append(li)}es.forEach(e=>{const li=document.createElement("li"),b=document.createElement("button");b.type="button";b.className="file-row";b.dataset.path=e.path;const name=document.createElement("span");name.className="name";name.textContent=`${e.type==="directory"?"▸ ":""}${e.name||e.path}`;const meta=document.createElement("span");meta.className="meta";meta.textContent=e.type==="directory"?e.status||"폴더":bytes(e.size);if(e.type==="directory"||e.type==="file"){const c=document.createElement("input");c.type="checkbox";c.checked=state.selectedPaths.has(e.path);c.setAttribute("aria-label",`${e.path} 선택`);c.addEventListener("click",ev=>ev.stopPropagation());c.addEventListener("change",ev=>{ev.target.checked?state.selectedPaths.add(e.path):state.selectedPaths.delete(e.path);updateSelection()});li.append(c)}b.append(name,meta);b.addEventListener("click",ev=>{if(ev.target?.type==="checkbox")return;e.type==="directory"?loadFiles(e.path):loadPreview(e.path)});li.className="file-item";li.append(b);l.append(li)});if(!es.length)listState(l,"폴더가 비어 있습니다","루트를 선택해 로컬 파일을 탐색하세요.");updateSelection()}catch(e){listState(l,"파일을 불러올 수 없습니다",e.message);showError(e)}}
async function loadIndexed(folder=state.indexedFolder){state.indexedFolder=folder;setText("indexed-path",folder||"/");const l=$("indexed-list");listState(l,"인덱스 불러오는 중","인덱스 경로를 읽는 중…");try{const d=await api("/api/indexed-files",{path:folder}),es=Array.isArray(d.entries)?d.entries:[];l.replaceChildren();setText("indexed-count",`${es.length}개 항목`);if(folder){const li=document.createElement("li"),b=document.createElement("button");b.type="button";b.className="file-row";b.textContent="↩ 상위 폴더";b.addEventListener("click",()=>loadIndexed(folder.split("/").slice(0,-1).join("/")));li.append(b);l.append(li)}es.forEach(e=>{const li=document.createElement("li"),b=document.createElement("button");b.className="file-row";b.type="button";const n=document.createElement("span");n.className="name";n.textContent=`${e.type==="directory"?"▸ ":""}${e.name}`;const m=document.createElement("span");m.className="meta";m.textContent=e.status||"indexed";b.append(n,m);b.addEventListener("click",()=>e.type==="directory"?loadIndexed(e.path):loadPreview(e.path));li.append(b);l.append(li)});if(!es.length)listState(l,"인덱스된 파일이 없습니다","루트를 준비하거나 새로고침하세요.")}catch(e){listState(l,"인덱스를 불러올 수 없습니다",e.message);showError(e)}}
function sortResults(){const mode=$("result-sort").value;state.results=state.results.slice().sort((a,b)=>mode==="name"?String(a.name||a.path).localeCompare(String(b.name||b.path)):mode==="extension"?String(a.extension||a.path.split(".").pop()).localeCompare(String(b.extension||b.path.split(".").pop())):mode==="mtime"?(Number(b.mtime)||0)-(Number(a.mtime)||0):mode==="size"?(Number(b.size)||0)-(Number(a.size)||0):Number(b.score||b.s||0)-Number(a.score||a.s||0));renderResults()}
function renderResults(){const l=$("results");l.replaceChildren();const groups=$("result-group").value,items=state.results;if(groups!=="none"){const buckets=new Map;items.forEach(x=>{const p=x.path||x.p||"",key=groups==="folder"?p.split("/").slice(0,-1).join("/")||"/":(x.extension||p.split(".").pop()||"(확장자 없음)");if(!buckets.has(key))buckets.set(key,[]);buckets.get(key).push(x)});for(const [key,rows] of buckets){const h=document.createElement("li");h.className="pane-note";h.textContent=`${key} · ${rows.length}건`;l.append(h);rows.forEach((x,i)=>appendResult(l,x,i))}}else items.forEach((x,i)=>appendResult(l,x,i));if(!items.length)listState(l,"일치하는 파일이 없습니다","더 짧은 검색어 또는 인덱싱 정책을 확인하세요.");setText("results-meta",`“${state.query}” 결과 ${items.length}건 · ${groups==="none"?"그룹 없음":"그룹별 표시"}`)}
function appendResult(l,x,i){const li=document.createElement("li"),b=document.createElement("button"),title=document.createElement("strong"),path=document.createElement("div"),ev=document.createElement("div"),sc=document.createElement("span");b.className="result";b.type="button";const p=x.path||x.p||"";b.dataset.path=p;sc.className="score";sc.textContent=Number.isFinite(Number(x.score||x.s))?Number(x.score||x.s).toFixed(2):`#${i+1}`;title.className="name";title.textContent=x.name||p.split("/").pop();path.className="path";path.textContent=x.root?`${x.root}/${p}`:p;ev.className="evidence";ev.textContent=x.preview_snippet||x.evidence?.[0]||x.reasons?.join(" · ")||"메타데이터 없음";b.append(sc,title,path,ev);b.addEventListener("click",()=>loadPreview(p,x.root));li.append(b);l.append(li)}
async function find(e){e.preventDefault();activateTab("search");const q=$("query").value.trim();if(!q){state.query="";listState($("results"),"검색어를 입력하세요","파일명, 주제, 사람 이름을 입력하세요.");return}state.query=q;const seq=++findSeq,btn=$("search-button");btn.disabled=true;btn.setAttribute("aria-busy","true");clearError();listState($("results"),"찾는 중","인덱스된 문서를 검색합니다…");try{const d=await api("/api/find",findOptions({q}));if(seq!==findSeq)return;state.results=Array.isArray(d.candidates)?d.candidates:[];setText("confidence",d.confidence?`${d.confidence} 신뢰도`:"");sortResults();clearError()}catch(x){if(seq!==findSeq)return;listState($("results"),"찾기에 실패했습니다",x.message);showError(x)}finally{if(seq===findSeq){btn.disabled=false;btn.removeAttribute("aria-busy")}}}
let previewSeq=0,findSeq=0,previewUrl="";
async function excludeSelection(excluded){const paths=selectedPaths();if(!paths.length){toast("먼저 항목을 선택하세요.");return}await mutation("/api/exclude-selection",{path:paths,excluded},excluded?"exclude-selection":"include-selection")}
async function loadPreview(path,root){activateTab("search");const seq=++previewSeq;state.selected=path;state.selectedRoot=root||"";$("download").disabled=false;["reveal","open"].forEach(id=>state.busy?$(id).dataset.busyPrev="false":$(id).disabled=false);listState($("preview"),"미리보기 불러오는 중","안전한 렌더러를 준비합니다…");try{const d=await api("/api/preview",{path,q:state.query,...root?{root}:{} });if(seq!==previewSeq)return;const c=$("preview");c.replaceChildren();if(previewUrl){URL.revokeObjectURL(previewUrl);previewUrl=""}const m=document.createElement("div");m.className="preview-meta";m.textContent=[d.path,d.renderer||d.type,bytes(d.size),d.encoding].filter(Boolean).join(" · ");c.append(m);if(d.supported===false){const x=document.createElement("div");x.className="state";x.textContent=d.reason||"지원하지 않는 미리보기 형식입니다.";c.append(x);return}if(d.binary_url&&d.media_type&&!d.content){const safeUrl=new URL(d.binary_url,location.href);if(safeUrl.origin!==location.origin||safeUrl.pathname!=="/api/preview/file")throw Error("안전하지 않은 미리보기 주소입니다.");if(root)safeUrl.searchParams.set("root",root);const label=`${d.path||"파일"} 미리보기`,wait=document.createElement("div");wait.className="state";wait.textContent="문서를 변환하는 중…";c.append(wait);const r=await fetch(safeUrl.href,{cache:"no-store"});if(seq!==previewSeq)return;if(!r.ok){let msg=`미리보기를 만들지 못했습니다 (${r.status})`;try{const j=await r.json();msg=j?.error||msg}catch{}throw Error(msg)}const blob=await r.blob();if(seq!==previewSeq)return;if(!blob.size)throw Error("미리보기 내용이 비어 있습니다. 다운로드로 확인하세요.");previewUrl=URL.createObjectURL(blob);wait.remove();if(d.media_type.startsWith("image/")){const o=document.createElement("img");o.src=previewUrl;o.alt=label;o.addEventListener("error",()=>{o.replaceWith(Object.assign(document.createElement("div"),{className:"state",textContent:"이미지를 표시할 수 없습니다. 다운로드로 확인하세요."}))});c.append(o);return}const o=document.createElement("object");o.type=d.media_type;o.data=previewUrl;o.setAttribute("aria-label",label);const fb=document.createElement("div");fb.className="state";fb.textContent="이 브라우저에서 PDF를 표시할 수 없습니다. 다운로드 또는 열기를 사용하세요.";o.append(fb);c.append(o);return}const p=document.createElement("pre");p.textContent=d.content||"";c.append(p)}catch(x){if(seq!==previewSeq)return;listState($("preview"),"미리보기 실패",x.message);showError(x)}}
async function saveSettings(){clearError();try{const q=new URLSearchParams({token:token(),root:state.root,algorithm:$("settings-algorithm").value,extensions:$("settings-extensions").value,exclude:$("settings-exclude").value,include_hidden:String($("settings-include-hidden").checked),include_sensitive:String($("settings-include-sensitive").checked),max_file_bytes:$("settings-max-file-bytes").value,max_files:$("settings-max-files").value,parse_timeout:$("settings-parse-timeout").value});await api("/api/settings",{},{method:"POST",query:q.toString()});$("find-exclude").value=$("settings-exclude").value;$("settings-dialog").close();toast("인덱싱 설정을 저장했습니다.")}catch(e){showError(e)}}async function loadSettings(){try{const d=await api("/api/settings",state.root?{root:state.root}:{}),s=d.settings||{};$("settings-algorithm").value=s.algorithm||"basic";$("settings-extensions").value=(s.extensions||[]).join(",");$("settings-exclude").value=(s.exclusions||[]).join(",");$("find-exclude").value=$("settings-exclude").value;$("settings-max-file-bytes").value=s.max_file_bytes??"";$("settings-max-files").value=s.max_files??"";$("settings-max-hash-bytes").value=s.max_hash_bytes??"";$("settings-parse-timeout").value=s.parse_timeout_seconds??"";$("settings-schedule").value="";$("settings-include-hidden").checked=!!s.include_hidden;$("settings-include-sensitive").checked=!!s.include_sensitive}catch(e){showError(e)}}
async function loadScenarios(){try{const d=await api("/api/scenarios"),s=$("scenario-select");s.replaceChildren();(d.scenarios||[]).forEach(x=>{const o=document.createElement("option");o.value=x.id;o.textContent=`${x.title||x.id} — ${x.purpose||""}`;o.dataset.query=x.query||"";o.dataset.extension=x.extension||"";s.append(o)});setText("scenario-count",`${d.count||0}개 시나리오`)}catch(e){showError(e)}}
async function runOps(path,values={},method="GET"){try{const d=await api(path,method==="POST"?{...values,token:token()}:values,{method});$("ops-dialog-body").textContent=typeof d==="string"?d:JSON.stringify(d,null,2);$("ops-dialog").showModal();$("ops-dialog-close").focus()}catch(e){showError(e)}}async function waitForJob(id){state.currentJob=id;$("cancel-job").disabled=false;for(let i=0;i<120;i++){const j=await api(`/api/jobs/${encodeURIComponent(id)}`);setText("job-status",`${j.state||"running"} · ${j.progress??"—"}%`);if(j.state==="completed"){state.currentJob="";$("cancel-job").disabled=true;return j.result||j}if(j.state==="failed"||j.state==="cancelled"){state.currentJob="";$("cancel-job").disabled=true;throw Error(j.error?.message||"작업이 취소되었습니다.")}await new Promise(r=>setTimeout(r,1000))}throw Error("작업 상태 확인 시간이 초과되었습니다.")}
const busyIds=["refresh","add-root","remove-root","remove-folder","reindex-toolbar","reindex-folder","deep-index","deep-target-enable","deep-target-disable","exclude-selection","include-selection","index-basic","index-content","reveal","open","root-select","ops-clean"];function setBusy(on){state.busy=on;document.body.toggleAttribute("aria-busy",on);busyIds.forEach(id=>{const x=$(id);if(!x)return;if(on){x.dataset.busyPrev=String(x.disabled);x.disabled=true}else if(x.dataset.busyPrev!==undefined){x.disabled=x.dataset.busyPrev==="true";delete x.dataset.busyPrev}})}function resetSelection(){state.folder="";state.indexedFolder="";state.selectedPaths.clear();state.selected="";state.selectedRoot="";previewSeq++;if(previewUrl){URL.revokeObjectURL(previewUrl);previewUrl=""}$("download").disabled=$("open").disabled=$("reveal").disabled=true;listState($("preview"),"파일을 선택하세요","")}async function reloadAll(){await loadRoots();await Promise.all([loadStatus(),loadFiles(state.folder),loadIndexed(state.indexedFolder)])}async function mutation(path,values,id){if(state.busy){toast("다른 작업이 진행 중입니다. 완료 후 다시 시도하세요.");if(path==="/api/root")loadRoots().catch(showError);return}const rootChange=path==="/api/root"||path==="/api/remove-root";setBusy(true);clearError();let ok=false;try{let d=await api(path,{...values,token:token()},{method:"POST"});if(rootChange)resetSelection();if(d.job_id)d=await waitForJob(d.job_id);ok=true;toast(`${$(id)?.textContent||"작업"} 완료`)}catch(e){showError(e)}finally{setBusy(false);if(!state.selected)$("open").disabled=$("reveal").disabled=true;updateSelection()}try{await reloadAll();if(ok)clearError()}catch(e){showError(e)}}
function confirmAction(copy,action){state.confirmAction=action;$("confirm-copy").textContent=copy;$("confirm-dialog").showModal();$("confirm-cancel").focus()}function closeConfirm(){$("confirm-dialog").close();state.confirmAction=null}
function activateTab(id){document.querySelectorAll(".tab").forEach(t=>{const on=t.id===`tab-${id}`;t.setAttribute("aria-selected",on);t.tabIndex=on?0:-1});document.querySelectorAll(".tab-panel").forEach(p=>p.hidden=p.id!==`panel-${id}`)}
$("search-form").addEventListener("submit",find);$("result-sort").addEventListener("change",sortResults);$("result-group").addEventListener("change",renderResults);$("view-filter").addEventListener("change",()=>loadFiles());$("view-sort").addEventListener("change",()=>loadFiles());document.querySelectorAll(".tab").forEach(t=>t.addEventListener("click",()=>activateTab(t.id.slice(4))));$("root-select").addEventListener("change",()=>mutation("/api/root",{path:$("root-select").value},"root-select"));$("add-root").addEventListener("click",()=>{const p=prompt("추가할 절대 폴더 경로를 입력하세요");if(p)mutation("/api/root",{path:p},"add-root")});$("refresh").addEventListener("click",()=>loadRoots().then(()=>Promise.all([loadStatus(),loadFiles(),loadIndexed()])).catch(showError));$("settings-open").addEventListener("click",()=>{$("settings-dialog").showModal();loadSettings();$("settings-algorithm").focus()});$("settings-cancel").addEventListener("click",()=>$('settings-dialog').close());$("settings-save").addEventListener("click",saveSettings);$("confirm-cancel").addEventListener("click",closeConfirm);$("confirm-ok").addEventListener("click",()=>{const action=state.confirmAction;closeConfirm();action?.()});$("ops-dialog-close").addEventListener("click",()=>$('ops-dialog').close());$("download").addEventListener("click",()=>state.selected&&location.assign(`/download?${params({path:state.selected,root:state.selectedRoot})}`));$("reveal").addEventListener("click",()=>state.selected&&mutation("/reveal",{path:state.selected,root:state.selectedRoot},"reveal"));$("open").addEventListener("click",()=>state.selected&&mutation("/open",{path:state.selected,root:state.selectedRoot},"open"));$("up-folder").addEventListener("click",()=>loadFiles(state.folder.split("/").slice(0,-1).join("/")));$("up-indexed").addEventListener("click",()=>loadIndexed(state.indexedFolder.split("/").slice(0,-1).join("/")));$("index-basic").addEventListener("click",()=>mutation("/api/index-selection",{mode:"basic",path:selectedPaths()},"index-basic"));$("index-content").addEventListener("click",()=>mutation("/api/index-selection",{mode:"content",path:selectedPaths()},"index-content"));$("reindex-toolbar").addEventListener("click",()=>mutation("/api/reindex",{async:true},"reindex-toolbar"));$("reindex-folder").addEventListener("click",()=>mutation("/api/reindex-folder",{path:state.folder||"."},"reindex-folder"));$("deep-index").addEventListener("click",()=>mutation("/api/deep-index",{async:true,media_ocr:$("media-ocr").checked,media_asr:$("media-asr").checked,media_max_mb:num("media-max-mb"),archive_max_entries:num("archive-max-entries"),archive_max_entry_bytes:num("archive-max-entry-bytes"),archive_max_total_bytes:num("archive-max-total-bytes")},"deep-index"));$("remove-folder").addEventListener("click",()=>state.folder?(folder=>confirmAction(`폴더 “${folder}”의 인덱스 아티팩트만 제거합니다(루트: ${state.root||"—"}). 원본 파일은 보존됩니다.`,()=>mutation("/api/remove-folder",{path:folder},"remove-folder")))(state.folder):toast("하위 폴더를 먼저 선택하세요."));$("remove-root").addEventListener("click",()=>(root=>root?confirmAction(`루트 “${root}”를 인덱스 목록에서 제거합니다. 원본 파일은 보존됩니다.`,()=>mutation("/api/remove-root",{path:root},"remove-root")):toast("제거할 루트가 없습니다."))(state.root));$("cancel-job").addEventListener("click",()=>state.currentJob&&api(`/api/jobs/${encodeURIComponent(state.currentJob)}/cancel`,{},{method:"POST",query:`token=${encodeURIComponent(token())}`}).then(()=>{$("cancel-job").disabled=true;setText("job-status","취소 요청됨 · 현재 서버 작업 종료 대기")}).catch(showError));$("scenario-run").addEventListener("click",()=>{const o=$("scenario-select").selectedOptions[0];if(o)runOps("/api/scenario-run",{id:o.value},"GET")});$("ops-doctor").addEventListener("click",()=>runOps("/api/doctor"));$("ops-map").addEventListener("click",()=>runOps("/api/map"));$("ops-graph").addEventListener("click",()=>runOps("/api/graph",{command:"status"}));$("ops-graph-query").addEventListener("click",()=>runOps("/api/graph",{command:"query",q:state.query,top_k:num("find-top-k")||20}));$("ops-graph-explain").addEventListener("click",()=>runOps("/api/graph",{command:"explain",path:state.selected}));$("ops-brief").addEventListener("click",()=>runOps("/api/brief",findOptions({q:state.query})));$("ops-clean-dry").addEventListener("click",()=>runOps("/api/clean",{dry_run:1},"POST"));$("ops-clean").addEventListener("click",()=>confirmAction(`루트 “${state.root||"—"}”의 인덱스 아티팩트를 정리합니다. 원본 파일은 보존됩니다. 계속할까요?`,()=>runOps("/api/clean",{force:1},"POST")));
$("exclude-selection").addEventListener("click",()=>excludeSelection(true));$("include-selection").addEventListener("click",()=>excludeSelection(false));
$("deep-target-enable").addEventListener("click",()=>state.folder?mutation("/api/deep-index-target",{path:state.folder,enabled:true},"deep-target-enable"):toast("하위 폴더를 먼저 선택하세요."));$("deep-target-disable").addEventListener("click",()=>state.folder?mutation("/api/deep-index-target",{path:state.folder,enabled:false},"deep-target-disable"):toast("하위 폴더를 먼저 선택하세요."));
$("confirm-dialog").addEventListener("cancel",()=>state.confirmAction=null);
document.querySelectorAll("dialog").forEach(d=>d.addEventListener("close",()=>{if(d.id==="settings-dialog")$("settings-open").focus()}));
document.querySelectorAll(".tab").forEach((t,i)=>t.addEventListener("keydown",e=>{const a=[...document.querySelectorAll(".tab")];let n;if(e.key==="ArrowRight")n=(i+1)%a.length;if(e.key==="ArrowLeft")n=(i+a.length-1)%a.length;if(e.key==="Home")n=0;if(e.key==="End")n=a.length-1;if(n!==undefined){e.preventDefault();activateTab(a[n].id.slice(4));a[n].focus()}}));activateTab("search");
document.addEventListener("DOMContentLoaded",()=>{loadRoots().then(()=>Promise.all([loadStatus(),loadFiles(),loadIndexed(),loadSettings(),loadScenarios()])).catch(showError)});})();</script></body></html>"###;
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
    // Library-root discovery walks HOME / Google Drive FUSE and must not
    // run before accept(). A TCP ready probe succeeds at bind; a synchronous
    // enqueue here delayed the first /api/find by ~14s on a cold Drive cache.
    thread::spawn(|| {
        crate::post_install_commands::enqueue_missing_library_root_prepares();
    });
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
    fn console_has_distinct_accessible_navigation_panels() {
        let markup = INDEX_HTML.split("<script>").next().unwrap();
        for name in ["search", "index", "ops"] {
            assert!(markup.contains(&format!("id=\"tab-{name}\" role=\"tab\"")));
            assert!(markup.contains(&format!("id=\"panel-{name}\" class=\"tab-panel\" role=\"tabpanel\"")));
        }
        assert!(markup.contains("id=\"panel-index\" class=\"tab-panel\" role=\"tabpanel\" aria-labelledby=\"tab-index\" hidden"));
        assert!(markup.contains("id=\"exclude-selection\""));
        assert!(markup.contains("id=\"up-indexed\""));
        assert!(!markup.contains("overflow-x:hidden"));
    }
    #[test]
    fn console_serializes_repeated_paths_and_uses_get_scenarios() {
        let script = INDEX_HTML.split("<script>").nth(1).unwrap();
        assert!(script.contains("v.forEach(item=>p.append(k,String(item)))"));
        assert!(script.contains("runOps(\"/api/scenario-run\",{id:o.value},\"GET\")"));
        assert!(!script.contains("innerHTML"));
        assert!(!script.contains("localStorage"));
        assert!(!script.contains("mutation(\"/api/scenario-run\""));
    }
}
