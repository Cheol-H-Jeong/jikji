use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use jikji_core::PrepareOptions;
use jikji_index::{doctor, prepare};
use jikji_search::{DiscoverOptions, SearchOptions, discover, search};
use serde_json::{Value, json};

use super::http::{HttpRequest, HttpResponse, malformed_request, query_bool, query_value};
use super::token::ManagementToken;

#[derive(Clone)]
pub(crate) struct GuiState {
    root: Arc<RwLock<PathBuf>>,
    manage_token: ManagementToken,
}

impl GuiState {
    pub(crate) fn new(root: PathBuf, manage_token: ManagementToken) -> Self {
        Self {
            root: Arc::new(RwLock::new(root)),
            manage_token,
        }
    }

    fn root(&self) -> std::result::Result<PathBuf, HttpResponse> {
        self.root
            .read()
            .map(|root| root.clone())
            .map_err(|_| HttpResponse::json(500, json!({"error": "root state lock poisoned"})))
    }

    fn switch_root(&self, root: PathBuf) -> std::result::Result<(), HttpResponse> {
        let mut guard = self
            .root
            .write()
            .map_err(|_| HttpResponse::json(500, json!({"error": "root state lock poisoned"})))?;
        *guard = root;
        Ok(())
    }

    fn token_matches(&self, query: &str) -> bool {
        query_value(query, "token").is_some_and(|token| self.manage_token.matches(&token))
    }
}

pub(crate) fn route_request(
    state: &GuiState,
    request: &HttpRequest,
    index_html: &'static str,
) -> HttpResponse {
    if request.method.is_empty() {
        return malformed_request();
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => HttpResponse::html(200, index_html),
        ("GET", "/api/status") => with_root(state, root_status),
        ("GET", "/api/search") => with_root(state, |root| search_response(root, &request.query)),
        ("GET", "/api/find") | ("GET", "/api/discover") => {
            with_root(state, |root| discover_response(root, &request.query))
        }
        ("GET", "/download") => with_root(state, |root| download_response(root, &request.query)),
        ("POST", "/api/refresh") => management_response(state, &request.query, refresh_response),
        ("POST", "/api/root") => management_response(state, &request.query, root_switch_response),
        _ => HttpResponse::json(404, json!({"error": "not found"})),
    }
}

fn management_response(
    state: &GuiState,
    query: &str,
    action: fn(&GuiState, &str) -> HttpResponse,
) -> HttpResponse {
    if !state.token_matches(query) {
        return HttpResponse::json(403, json!({"error": "invalid management token"}));
    }
    action(state, query)
}

fn with_root(state: &GuiState, action: impl FnOnce(&Path) -> HttpResponse) -> HttpResponse {
    match state.root() {
        Ok(root) => action(&root),
        Err(response) => response,
    }
}

fn root_status(root: &Path) -> HttpResponse {
    let manifest = read_json(root.join(".jikji/manifest.json"));
    let doctor_ok = doctor(root).map(|report| report.ok).unwrap_or(false);
    HttpResponse::json(
        200,
        json!({
            "root": root,
            "prepared": doctor_ok,
            "manifest": manifest,
            "artifacts": {
                "manifest": root.join(".jikji/manifest.json").exists(),
                "search_index": root.join(".jikji/search_index.sqlite").exists(),
                "knowledge_graph": root.join(".jikji/knowledge_graph.json").exists(),
                "file_cards": root.join(".jikji/file_cards.jsonl").exists(),
                "doc_text": root.join(".jikji/doc_text").exists()
            },
            "default_agent_command": "jikji find ROOT \"query\" --json",
        }),
    )
}

fn search_response(root: &Path, query: &str) -> HttpResponse {
    let q = query_value(query, "q").unwrap_or_default();
    if q.trim().is_empty() {
        return HttpResponse::json(400, json!({"error": "missing q"}));
    }
    let top_k = query_value(query, "top_k")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    match search(root, &q, SearchOptions { top_k }) {
        Ok(candidates) => HttpResponse::json(
            200,
            json!({"root": root, "query": q, "candidates": candidates}),
        ),
        Err(error) => HttpResponse::json(500, json!({"error": error.to_string()})),
    }
}

fn discover_response(root: &Path, query: &str) -> HttpResponse {
    let q = query_value(query, "q").unwrap_or_default();
    if q.trim().is_empty() {
        return HttpResponse::json(400, json!({"error": "missing q"}));
    }
    let top_k = query_value(query, "top_k")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    match discover(
        root,
        &q,
        DiscoverOptions {
            top_k,
            ..DiscoverOptions::default()
        },
    ) {
        Ok(mut payload) => {
            payload["mode"] = json!("find");
            payload["command"] = json!("jikji find");
            HttpResponse::json(200, payload)
        }
        Err(error) => HttpResponse::json(500, json!({"error": error.to_string()})),
    }
}

fn download_response(root: &Path, query: &str) -> HttpResponse {
    let Some(path_value) = query_value(query, "path") else {
        return HttpResponse::json(400, json!({"error": "missing path"}));
    };
    let path = match resolve_root_path(root, &path_value) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if !path.is_file() {
        return HttpResponse::json(400, json!({"error": "download target is not a file"}));
    }
    match fs::read(&path) {
        Ok(body) => HttpResponse::binary(200, body, "application/octet-stream"),
        Err(source) => HttpResponse::json(500, json!({"error": source.to_string()})),
    }
}

fn refresh_response(state: &GuiState, _query: &str) -> HttpResponse {
    with_root(state, |root| {
        match prepare(root, &PrepareOptions::default()) {
            Ok(_) => root_status(root),
            Err(error) => HttpResponse::json(500, json!({"error": error.to_string()})),
        }
    })
}

fn root_switch_response(state: &GuiState, query: &str) -> HttpResponse {
    let Some(path) = query_value(query, "path") else {
        return HttpResponse::json(400, json!({"error": "missing path"}));
    };
    let candidate = PathBuf::from(path);
    let root = match candidate.canonicalize() {
        Ok(root) if root.is_dir() => root,
        Ok(root) => {
            return HttpResponse::json(
                400,
                json!({"error": format!("path is not a directory: {}", root.display())}),
            );
        }
        Err(source) => return HttpResponse::json(400, json!({"error": source.to_string()})),
    };
    if query_bool(query, "prepare")
        && let Err(error) = prepare(&root, &PrepareOptions::default())
    {
        return HttpResponse::json(500, json!({"error": error.to_string()}));
    }
    match state.switch_root(root.clone()) {
        Ok(()) => root_status(&root),
        Err(response) => response,
    }
}

fn resolve_root_path(root: &Path, rel_path: &str) -> std::result::Result<PathBuf, HttpResponse> {
    let candidate = Path::new(rel_path);
    if rel_path.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(HttpResponse::json(
            403,
            json!({"error": "path traversal is not allowed"}),
        ));
    }
    let joined = root.join(candidate);
    let resolved = joined
        .canonicalize()
        .map_err(|source| HttpResponse::json(404, json!({"error": source.to_string()})))?;
    if !resolved.starts_with(root) {
        return Err(HttpResponse::json(
            403,
            json!({"error": "path escapes root"}),
        ));
    }
    Ok(resolved)
}

fn read_json(path: PathBuf) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}))
}
