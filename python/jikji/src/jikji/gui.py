"""Local-only management GUI for Jikji roots.

The GUI is not Jikji's primary product; Jikji remains an agent skill/CLI.  This
loopback-only web app lets a human inspect one prepared root, refresh metadata,
switch/add a root, and optionally search/open/download files inside that root.
"""
from __future__ import annotations

import json
import mimetypes
import os
import secrets
import shutil
import subprocess
import sys
import threading
import urllib.parse
import webbrowser
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from .agent_index import build_agent_index
from .config import Config
from .discover import discover
from .eval import search
from .search_index import instant_index_path


class GuiSecurityError(ValueError):
    """Raised when a requested local path escapes the configured GUI root."""


def resolve_root_path(root: Path, rel_path: str) -> Path:
    """Resolve a file action path safely under root."""
    root = Path(root).expanduser().resolve()
    raw = urllib.parse.unquote(str(rel_path or "")).strip()
    if not raw:
        raise GuiSecurityError("missing path")
    candidate = Path(raw)
    if candidate.is_absolute():
        raise GuiSecurityError("absolute paths are not allowed")
    if any(part == ".." for part in candidate.parts):
        raise GuiSecurityError("path traversal is not allowed")
    resolved = (root / candidate).resolve()
    try:
        resolved.relative_to(root)
    except ValueError as exc:
        raise GuiSecurityError("path escapes root") from exc
    if not resolved.exists():
        raise FileNotFoundError(raw)
    return resolved


def resolve_gui_root(path_value: str) -> Path:
    """Resolve a user-supplied GUI root. Absolute paths are allowed here."""
    raw = str(path_value or "").strip()
    if not raw:
        raise GuiSecurityError("missing root")
    root = Path(raw).expanduser().resolve()
    if not root.is_dir():
        raise NotADirectoryError(root)
    return root


def open_local_path(path: Path) -> None:
    """Open a local file/folder with the platform default app without waiting."""
    if sys.platform == "darwin":
        cmd = ["open", str(path)]
    elif os.name == "nt":
        os.startfile(str(path))  # type: ignore[attr-defined]
        return
    else:
        opener = shutil.which("xdg-open") or shutil.which("gio")
        if opener is None:
            raise RuntimeError("No desktop opener found (expected xdg-open or gio)")
        cmd = [opener, "open", str(path)] if Path(opener).name == "gio" else [opener, str(path)]
    subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)  # noqa: S603


def _json_bytes(payload: Any) -> bytes:
    return json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def _read_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def root_status(root: Path) -> dict[str, Any]:
    root = Path(root).expanduser().resolve()
    index_dir = root / ".jikji"
    manifest = _read_json(index_dir / "manifest.json")
    graph = _read_json(index_dir / "knowledge_graph.json")
    graph_stats = graph.get("stats") if isinstance(graph.get("stats"), dict) else {}
    required = {
        "manifest": index_dir / "manifest.json",
        "search_index": instant_index_path(root),
        "wiki_index": index_dir / "wiki" / "index.md",
        "knowledge_graph": index_dir / "knowledge_graph.json",
        "graph_routes": index_dir / "graph_routes.jsonl",
        "file_cards": index_dir / "file_cards.jsonl",
        "doc_text": index_dir / "doc_text",
    }
    artifacts = {name: path.exists() for name, path in required.items()}
    return {
        "root": str(root),
        "prepared": bool(manifest) and artifacts["search_index"],
        "manifest": manifest,
        "graph_stats": graph_stats,
        "artifacts": artifacts,
        "default_agent_command": "jikji find ROOT \"query\" --json",
        "capabilities": {
            "find": "multi-query candidate slate + confidence + handoff action",
            "graph": "LLM Wiki knowledge graph status",
        },
        "paths": {name: str(path) for name, path in required.items()},
    }


HTML_PAGE = """<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Jikji Root Dashboard</title>
<style>
:root { color-scheme: dark; --bg:#101318; --panel:#181d25; --line:#2b3442; --txt:#e8edf4; --mut:#9aa8ba; --acc:#65f2ad; --warn:#ffd166; --bad:#ff6b6b; }
* { box-sizing:border-box; }
body { margin:0; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:var(--bg); color:var(--txt); }
main { max-width:1180px; margin:0 auto; padding:28px 18px 48px; }
h1 { margin:0 0 6px; font-size:28px; }
.sub { color:var(--mut); margin-bottom:22px; }
.panel, .card { background:var(--panel); border:1px solid var(--line); border-radius:16px; padding:15px; margin:12px 0; }
.row { display:flex; gap:10px; flex-wrap:wrap; align-items:center; }
.grid { display:grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap:10px; }
.metric { background:#0c0f14; border:1px solid var(--line); border-radius:14px; padding:12px; }
.metric .v { font-size:24px; font-weight:800; color:#dfffea; }
.metric .k { color:var(--mut); font-size:13px; }
input { flex:1; min-width:260px; border:1px solid var(--line); background:#0c0f14; color:var(--txt); border-radius:12px; padding:13px 14px; font-size:15px; }
button, a.btn { border:1px solid var(--line); background:#202838; color:var(--txt); padding:11px 13px; border-radius:12px; cursor:pointer; text-decoration:none; display:inline-flex; align-items:center; gap:6px; font-weight:650; }
button.primary { background:linear-gradient(135deg,#1e6f52,#2453a6); border-color:#3c8d70; }
button:hover, a.btn:hover { border-color:var(--acc); }
#statusLine { margin:12px 2px; color:var(--mut); min-height:22px; }
.path { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size:14px; overflow-wrap:anywhere; color:#dfffea; }
.meta { color:var(--mut); font-size:13px; margin-top:5px; }
.evidence { margin-top:8px; color:#b7c3d2; font-size:14px; line-height:1.45; }
.actions { display:flex; flex-wrap:wrap; gap:8px; justify-content:flex-end; }
.top { display:flex; justify-content:space-between; gap:12px; align-items:flex-start; }
.score { color:var(--warn); }
.err { color:var(--bad); }
.ok { color:var(--acc); }
.empty { border:1px dashed var(--line); padding:24px; text-align:center; border-radius:16px; color:var(--mut); }
small { color:var(--mut); }
</style>
</head>
<body>
<main>
<h1>Jikji Root Dashboard</h1>
<div class="sub">Jikji는 에이전트 스킬/CLI입니다. 이 GUI는 사람이 root의 메타화·LLM Wiki·지식그래프 상태를 보고 refresh/root 전환을 관리하는 로컬 패널입니다.</div>

<section class="panel">
  <div class="row">
    <input id="rootInput" placeholder="관리할 폴더 경로 예: /home/me/Documents" />
    <button class="primary" id="switchBtn">root 추가/전환</button>
    <button id="refreshBtn">prepare/refresh</button>
    <label class="meta"><input type="checkbox" id="mediaOpt" style="min-width:auto; width:auto" /> 멀티모달 OCR/ASR opt-in</label>
  </div>
  <div id="statusLine"></div>
  <div id="metrics" class="grid"></div>
  <div id="artifacts"></div>
</section>

<section class="panel">
  <h2>Find 검증</h2>
  <div class="sub">현재 agent 기본 진입점은 <code>jikji find ROOT "query" --json</code>입니다. query type, confidence, recommended action, 후보 evidence를 사람이 확인합니다.</div>
  <form class="row" id="form">
    <input id="q" autocomplete="off" placeholder="예: 작년 봄 ACME 계약서 PDF / invoice payment clause / 회의록" />
    <button class="primary" type="submit">find</button>
  </form>
</section>
<div id="results"></div>
</main>
<script>
const form = document.getElementById('form');
const q = document.getElementById('q');
const TOKEN = "__JIKJI_TOKEN__";
const rootInput = document.getElementById('rootInput');
const statusLine = document.getElementById('statusLine');
const metrics = document.getElementById('metrics');
const artifacts = document.getElementById('artifacts');
const results = document.getElementById('results');
const mediaOpt = document.getElementById('mediaOpt');
function esc(s){ return String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function enc(s){ return encodeURIComponent(String(s ?? '')); }
async function api(path, opts={}){
  const method = String(opts.method || 'GET').toUpperCase();
  let url = path;
  if(method !== 'GET') url += (url.includes('?') ? '&' : '?') + 'token=' + enc(TOKEN);
  const r = await fetch(url, opts);
  const d = await r.json().catch(() => ({}));
  if(!r.ok) throw new Error(d.error || r.statusText);
  return d;
}
function metric(k,v){ return `<div class="metric"><div class="v">${esc(v ?? '—')}</div><div class="k">${esc(k)}</div></div>`; }
function renderStatus(s){
  rootInput.value = s.root || '';
  const m = s.manifest || {}; const g = s.graph_stats || {}; const media = m.media_index || {};
  statusLine.innerHTML = `${s.prepared ? '<span class="ok">prepared</span>' : '<span class="err">not prepared</span>'} · <small>${esc(s.root)}</small> · <code>${esc(s.default_agent_command || '')}</code>`;
  metrics.innerHTML = [
    metric('files', m.files), metric('folders', m.folders), metric('documents', m.documents), metric('parse errors', m.parse_errors),
    metric('wiki sources', m.llm_wiki_sources), metric('graph nodes', g.nodes || m.knowledge_graph_nodes), metric('graph edges', g.edges || m.knowledge_graph_edges), metric('media index', media.status || '—'), metric('media files', media.media_files), metric('search index bytes', m.search_index_bytes)
  ].join('');
  artifacts.innerHTML = '<h3>Artifacts</h3>' + Object.entries(s.artifacts || {}).map(([k,v]) => `<div class="path">${v ? '✓' : '✗'} ${esc(k)} <small>${esc((s.paths||{})[k]||'')}</small></div>`).join('');
}
async function loadStatus(){ try { renderStatus(await api('/api/status')); } catch(e){ statusLine.innerHTML='<span class="err">'+esc(e.message)+'</span>'; } }
async function switchRoot(){
  if(mediaOpt.checked && !confirm('이미지/음성/영상 OCR·ASR은 CPU/RAM을 사용할 수 있습니다. 큰 미디어는 건너뛰고 bounded mode로 진행합니다. 계속할까요?')) return;
  statusLine.textContent='root 전환 중…';
  const suffix = mediaOpt.checked ? '&enable_media=1' : '';
  renderStatus(await api('/api/root?path=' + enc(rootInput.value) + suffix, {method:'POST'}));
}
async function refreshRoot(){
  if(mediaOpt.checked && !confirm('이미지/음성/영상 OCR·ASR은 CPU/RAM을 사용할 수 있습니다. 큰 미디어는 건너뛰고 bounded mode로 진행합니다. 계속할까요?')) return;
  statusLine.textContent='prepare/refresh 중…';
  const suffix = mediaOpt.checked ? '?enable_media=1' : '';
  renderStatus(await api('/api/refresh' + suffix, {method:'POST'}));
}
async function openPath(path){ await api('/open?path=' + enc(path), {method:'POST'}); statusLine.textContent = '열기 요청: ' + path; }
function renderFind(data){
  const items = data.candidates || [];
  if(!items.length){ results.innerHTML = '<div class="empty">find 결과가 없습니다.</div>'; return; }
  const factors = data.confidence_factors || {};
  const header = `<section class="card"><div class="top"><div><div class="path">Find: ${esc(data.query_type)} · ${esc(data.confidence)} · score ${esc(data.confidence_score)}</div><div class="meta">action=${esc(data.recommended_action)} · variants=${esc((data.query_variants||[]).join(' / '))}</div><div class="meta">factors: ${Object.entries(factors).map(([k,v]) => esc(k)+'='+esc(v)).join(' · ')}</div></div></div></section>`;
  const cards = items.map((it, idx) => {
    const path = it.p || it.path || '';
    const ev = it.ev ? `<div class="evidence">${esc(it.ev)}</div>` : (it.evidence || []).slice(0,2).map(x => `<div class="evidence">${esc(x)}</div>`).join('');
    const why = (it.why || it.reasons || []).join(', ');
    const terms = (it.terms || it.matched_terms || []).slice(0,8).join(', ');
    const score = it.s ?? it.score ?? '';
    return `<section class="card"><div class="top"><div><div class="path">${idx+1}. ${esc(path)}</div><div class="meta"><span class="score">score ${esc(score)}</span> · rank ${esc(it.rank ?? '')} · ${esc(why)} · ${esc(terms)}</div><div class="meta">queries: ${esc((it.queries||[]).join(' / '))}</div>${ev}</div><div class="actions"><button type="button" onclick="openPath(decodeURIComponent('${enc(path)}')).catch(e => statusLine.innerHTML='<span class=err>'+esc(e.message)+'</span>')">열기</button><a class="btn" href="/download?path=${enc(path)}">다운로드</a><button type="button" onclick="api('/reveal?path=${enc(path)}',{method:'POST'}).catch(e => statusLine.innerHTML='<span class=err>'+esc(e.message)+'</span>')">폴더 열기</button></div></div></section>`;
  }).join('');
  results.innerHTML = header + cards;
}
async function doSearch(){
  const query = q.value.trim(); if(!query){ q.focus(); return; }
  statusLine.textContent = 'find 중…'; results.innerHTML = '';
  const data = await api('/api/find?q=' + enc(query) + '&top_k=20');
  statusLine.innerHTML = `${esc((data.candidates||[]).length)}개 후보 · ${esc(data.query_type)} · ${esc(data.confidence)} · action ${esc(data.recommended_action)} · <small>${esc(data.root)}</small>`;
  renderFind(data);
}
document.getElementById('switchBtn').addEventListener('click', () => switchRoot().catch(e => statusLine.innerHTML='<span class="err">'+esc(e.message)+'</span>'));
document.getElementById('refreshBtn').addEventListener('click', () => refreshRoot().catch(e => statusLine.innerHTML='<span class="err">'+esc(e.message)+'</span>'));
form.addEventListener('submit', e => { e.preventDefault(); doSearch().catch(err => statusLine.innerHTML='<span class="err">'+esc(err.message)+'</span>'); });
loadStatus();
</script>
</body>
</html>
"""


class JikjiGuiServer(ThreadingHTTPServer):
    def __init__(self, server_address: tuple[str, int], root: Path, *, auto_prepare: bool = False):
        self._root_lock = threading.RLock()
        self._root = resolve_gui_root(str(root))
        self.manage_token = secrets.token_urlsafe(24)
        if auto_prepare and not instant_index_path(self._root).exists():
            build_agent_index(self._root, Config())
        super().__init__(server_address, JikjiGuiHandler)

    @property
    def root(self) -> Path:
        with self._root_lock:
            return self._root

    def prepare_current_root(self, *, enable_media_index: bool = False) -> None:
        root = self.root
        build_agent_index(root, Config(enable_media_index=enable_media_index))

    def switch_root(self, path_value: str, *, prepare: bool = False, enable_media_index: bool = False) -> None:
        new_root = resolve_gui_root(path_value)
        if prepare:
            build_agent_index(new_root, Config(enable_media_index=enable_media_index))
        with self._root_lock:
            self._root = new_root


class JikjiGuiHandler(BaseHTTPRequestHandler):
    server: JikjiGuiServer

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def _send(self, status: HTTPStatus, body: bytes, content_type: str) -> None:
        self.send_response(int(status))
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _send_json(self, status: HTTPStatus, payload: Any) -> None:
        self._send(status, _json_bytes(payload), "application/json; charset=utf-8")

    def _query(self) -> dict[str, list[str]]:
        parsed = urllib.parse.urlparse(self.path)
        return urllib.parse.parse_qs(parsed.query, keep_blank_values=True)

    def _query_value(self, name: str) -> str:
        return (self._query().get(name) or [""])[0]

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler name
        parsed = urllib.parse.urlparse(self.path)
        try:
            if parsed.path == "/" or parsed.path == "/index.html":
                html = HTML_PAGE.replace("__JIKJI_TOKEN__", self.server.manage_token)
                self._send(HTTPStatus.OK, html.encode("utf-8"), "text/html; charset=utf-8")
            elif parsed.path == "/api/status":
                self._send_json(HTTPStatus.OK, root_status(self.server.root))
            elif parsed.path == "/api/search":
                self._handle_search()
            elif parsed.path in {"/api/find", "/api/discover"}:
                self._handle_find()
            elif parsed.path == "/download":
                self._handle_download()
            else:
                self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
        except GuiSecurityError as exc:
            self._send_json(HTTPStatus.FORBIDDEN, {"error": str(exc)})
        except FileNotFoundError as exc:
            self._send_json(HTTPStatus.NOT_FOUND, {"error": str(exc)})
        except Exception as exc:  # keep GUI server alive on per-request failures
            self._send_json(HTTPStatus.INTERNAL_SERVER_ERROR, {"error": str(exc)})

    def _require_token(self) -> None:
        if self._query_value("token") != self.server.manage_token:
            raise GuiSecurityError("invalid management token")

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler name
        parsed = urllib.parse.urlparse(self.path)
        try:
            self._require_token()
            if parsed.path == "/open":
                self._handle_open()
            elif parsed.path == "/reveal":
                self._handle_reveal()
            elif parsed.path == "/api/refresh":
                enable_media = self._query_value("enable_media") in {"1", "true", "yes", "on"}
                self.server.prepare_current_root(enable_media_index=enable_media)
                self._send_json(HTTPStatus.OK, root_status(self.server.root))
            elif parsed.path == "/api/root":
                enable_media = self._query_value("enable_media") in {"1", "true", "yes", "on"}
                prepare = self._query_value("prepare") in {"1", "true", "yes", "on"}
                self.server.switch_root(self._query_value("path"), prepare=prepare, enable_media_index=enable_media)
                self._send_json(HTTPStatus.OK, root_status(self.server.root))
            else:
                self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
        except GuiSecurityError as exc:
            self._send_json(HTTPStatus.FORBIDDEN, {"error": str(exc)})
        except FileNotFoundError as exc:
            self._send_json(HTTPStatus.NOT_FOUND, {"error": str(exc)})
        except Exception as exc:
            self._send_json(HTTPStatus.INTERNAL_SERVER_ERROR, {"error": str(exc)})

    def _path_param(self) -> str:
        return self._query_value("path")

    def _handle_search(self) -> None:
        query = self._query_value("q").strip()
        top_k_raw = self._query_value("top_k") or "20"
        top_k = max(1, min(100, int(top_k_raw) if top_k_raw.isdigit() else 20))
        if not query:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": "missing q"})
            return
        ranked = search(self.server.root, query, top_k=top_k)
        self._send_json(HTTPStatus.OK, {"root": str(self.server.root), "query": query, "candidates": ranked})

    def _handle_find(self) -> None:
        query = self._query_value("q").strip()
        top_k_raw = self._query_value("top_k") or "20"
        top_k = max(1, min(100, int(top_k_raw) if top_k_raw.isdigit() else 20))
        if not query:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": "missing q"})
            return
        payload = discover(self.server.root, query, top_k=top_k)
        payload["mode"] = "find"
        payload["command"] = "jikji find"
        self._send_json(HTTPStatus.OK, payload)

    def _handle_download(self) -> None:
        path = resolve_root_path(self.server.root, self._path_param())
        if not path.is_file():
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": "download target is not a file"})
            return
        ctype = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        size = path.stat().st_size
        self.send_response(int(HTTPStatus.OK))
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(size))
        self.send_header("Content-Disposition", f"attachment; filename*=UTF-8''{urllib.parse.quote(path.name)}")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        with path.open("rb") as handle:
            shutil.copyfileobj(handle, self.wfile, length=1024 * 1024)

    def _handle_open(self) -> None:
        path = resolve_root_path(self.server.root, self._path_param())
        open_local_path(path)
        self._send_json(HTTPStatus.OK, {"ok": True, "path": str(path)})

    def _handle_reveal(self) -> None:
        path = resolve_root_path(self.server.root, self._path_param())
        open_local_path(path if path.is_dir() else path.parent)
        self._send_json(HTTPStatus.OK, {"ok": True, "path": str(path.parent if path.is_file() else path)})


def serve_gui(
    root: Path,
    *,
    host: str = "127.0.0.1",
    port: int = 8765,
    auto_prepare: bool = False,
    open_browser: bool = True,
    quiet: bool = False,
) -> str:
    server = JikjiGuiServer((host, port), Path(root), auto_prepare=auto_prepare)
    actual_host, actual_port = server.server_address[:2]
    url = f"http://{actual_host}:{actual_port}/"
    if open_browser:
        webbrowser.open(url)
    if not quiet:
        print(f"Jikji GUI: {url}", flush=True)
        print(f"Root: {server.root}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return url
