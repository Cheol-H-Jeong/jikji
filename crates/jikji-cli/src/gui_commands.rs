use std::net::{TcpListener, TcpStream};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use jikji_core::PrepareOptions;
use jikji_index::prepare;
use serde_json::json;

use crate::args::GuiArgs;
use crate::output::print_json;

mod access;
mod http;
mod jobs;
mod preview;
mod routing;
mod scenarios;
mod token;

use access::{AccessGate, Gate, security_headers};
use routing::{GuiState, route_request};
use token::ManagementToken;

const INDEX_HTML: &str = include_str!("gui_commands/assets/index.html");
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
    let access_token = access_token(args.access_token.clone())?;
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
            "manage_token": token.as_str(),
            "access_token": access_token.as_str()
        }))?;
    } else if !args.serve_child {
        println!("Jikji GUI: {url}");
        println!("Access token: {}", access_token.as_str());
    }
    let gate = AccessGate::new(access_token, args.allow_host);
    serve_loop(listener, GuiState::new(root, token), gate)
}

/// Console login token: `--access-token`, else `$JIKJI_GUI_ACCESS_TOKEN`, else a random one.
fn access_token(flag: Option<String>) -> jikji_core::Result<ManagementToken> {
    let configured = flag
        .or_else(|| std::env::var("JIKJI_GUI_ACCESS_TOKEN").ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    match configured {
        Some(value) if value.len() < 16 => {
            Err(invalid_input("GUI access token must be at least 16 characters"))
        }
        Some(value) => Ok(ManagementToken::new(value)),
        None => ManagementToken::generate(),
    }
}

fn spawn_background(args: GuiArgs) -> jikji_core::Result<ExitCode> {
    let port = if args.port == 0 {
        reserve_loopback_port(&args.host)?
    } else {
        args.port
    };
    let token = ManagementToken::generate()?;
    let access_token = access_token(args.access_token.clone())?;
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
        // Passed through the environment so the token never shows up in `ps` output.
        .env("JIKJI_GUI_ACCESS_TOKEN", access_token.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for host in &args.allow_host {
        command.arg("--allow-host").arg(host);
    }
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
        "access_token": access_token.as_str(),
        "cleanup": cleanup_command(child.id()),
    });
    if args.json {
        print_json(&payload)?;
    } else {
        println!("{url}");
        println!("Access token: {}", access_token.as_str());
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

fn serve_loop(listener: TcpListener, state: GuiState, gate: AccessGate) -> jikji_core::Result<ExitCode> {
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
                let request_gate = gate.clone();
                thread::spawn(move || {
                    let _ = handle_stream(stream, &request_state, &request_gate);
                });
            }
            Err(source) => return Err(jikji_core::io_error("<gui-accept>", source)),
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn handle_stream(stream: TcpStream, state: &GuiState, gate: &AccessGate) -> jikji_core::Result<()> {
    let request = http::HttpRequest::read(&stream)?;
    let response = match gate.check(&request) {
        Gate::Allow => route_request(state, &request, INDEX_HTML),
        Gate::Respond(response) => response,
    };
    http::write_response(stream, response, &security_headers())
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
    fn console_has_accessible_shell_before_network_requests() {
        let markup = INDEX_HTML.split("<script>").next().unwrap();
        for id in ["main-content", "panel-overview", "panel-search", "panel-index", "panel-tools",
            "nav-overview", "nav-search", "nav-index", "nav-tools", "manage-token",
            "search-query", "roots-panel", "files-panel", "indexed-panel", "settings-dialog",
            "search-split", "index-split", "search-preview-pane", "index-preview-pane",
            "search-preview-modes", "index-preview-modes", "search-snippet-tab", "search-file-tab",
            "index-snippet-tab", "index-file-tab",
            "search-preview-back", "index-preview-back", "logout", "confirm-dialog", "jobs-strip", "cancel-job"] {
            assert!(markup.contains(&format!("id=\"{id}\"")), "missing {id}");
        }
        assert!(markup.contains("<main"));
        assert!(markup.contains("<nav"));
        assert!(markup.contains("<header"));
        assert!(markup.contains("aria-current=\"page\""));
        assert!(markup.contains("aria-busy=\"true\""));
        assert!(markup.contains("class=\"skeleton\""));
        assert!(!INDEX_HTML.contains("preview-dialog"));
        assert!(markup.contains("파일을 선택하면 여기에 미리보기가 표시됩니다"));
        assert!(markup.contains("grid-template-columns:minmax(0,3fr) minmax(0,2fr)"));
        for scope in ["search", "index"] {
            assert!(markup.contains(&format!("id=\"{scope}-preview-modes\" class=\"preview-modes\" role=\"tablist\"")));
        }
        assert!(INDEX_HTML.find("</main>").unwrap() < INDEX_HTML.find("fetch(").unwrap());
    }
    #[test]
    fn console_uses_the_api_contract_without_unsafe_dom_or_persistence() {
        let script = INDEX_HTML.split("<script>").nth(1).unwrap();
        for route in ["/api/status", "/api/roots", "/api/files", "/api/indexed-files",
            "/api/find", "/api/preview", "/api/preview/file", "/api/settings",
            "/api/snippet", "/api/preview/pages", "/api/preview/page?",
            "/api/scenarios", "/api/scenario-run", "/api/doctor", "/api/map", "/api/graph",
            "/api/brief", "/api/clean", "/api/root", "/api/remove-root", "/api/refresh",
            "/api/reindex", "/api/reindex-folder", "/api/remove-folder", "/api/deep-index",
            "/api/deep-index-target", "/api/index-selection", "/api/exclude-selection",
            "/api/jobs/", "/cancel", "/download", "/reveal", "/open", "/api/logout"] {
            assert!(script.contains(route), "missing route {route}");
        }
        for unsafe_api in ["innerHTML", "outerHTML", "insertAdjacentHTML", "document.write", "eval(", "localStorage"] {
            assert!(!INDEX_HTML.contains(unsafe_api), "forbidden API {unsafe_api}");
        }
        for kind in ["text", "image", "pdf", "audio", "video", "none"] {
            assert!(script.contains(&format!("case \"{kind}\":")), "missing preview kind {kind}");
        }
        assert!(script.contains("response.status===401"));
        assert!(script.contains("location.reload()"));
        assert!(script.contains("d.binary_url.startsWith(\"/api/preview/file\")"));
        assert!(script.contains("new URL(url,location.origin).origin===location.origin"));
        assert!(script.contains("modeSeq===p[modeKey]"));
        assert!(script.contains("snippetSeq"));
        assert!(script.contains("fileSeq"));
        assert!(script.contains("img.loading=index===0?\"eager\":\"lazy\""), "first page loads at once, the rest lazily");
        assert!(script.contains("ArrowLeft"));
        assert!(script.contains("ArrowRight"));
        assert!(script.contains("document.createTextNode"));
        assert!(script.contains("el(\"mark\""));
        assert!(script.contains("aria-selected"));
        assert!(script.contains("120000"));
        assert!(script.contains("90000:20000"));
        assert!(!INDEX_HTML.contains("onclick="));
        assert!(script.contains("value.forEach(item=>p.append(key,String(item)))"));
        assert!(script.contains("output(\"시나리오 결과\",\"/api/scenario-run\",{id:select.value})"));
        assert!(script.contains("Promise.allSettled([loadStatus(),loadRoots()"));
    }
}
