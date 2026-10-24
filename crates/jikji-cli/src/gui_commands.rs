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
    fn console_has_accessible_shell_before_network_requests() {
        let markup = INDEX_HTML.split("<script>").next().unwrap();
        for id in ["main-content", "panel-overview", "panel-search", "panel-index", "panel-tools",
            "nav-overview", "nav-search", "nav-index", "nav-tools", "manage-token",
            "search-query", "roots-panel", "files-panel", "indexed-panel", "settings-dialog",
            "preview-dialog", "confirm-dialog", "jobs-strip", "cancel-job"] {
            assert!(markup.contains(&format!("id=\"{id}\"")), "missing {id}");
        }
        assert!(markup.contains("<main"));
        assert!(markup.contains("<nav"));
        assert!(markup.contains("<header"));
        assert!(markup.contains("aria-current=\"page\""));
        assert!(markup.contains("aria-busy=\"true\""));
        assert!(markup.contains("class=\"skeleton\""));
        assert!(INDEX_HTML.find("</main>").unwrap() < INDEX_HTML.find("fetch(").unwrap());
    }
    #[test]
    fn console_uses_the_api_contract_without_unsafe_dom_or_persistence() {
        let script = INDEX_HTML.split("<script>").nth(1).unwrap();
        for route in ["/api/status", "/api/roots", "/api/files", "/api/indexed-files",
            "/api/find", "/api/preview", "/api/preview/file", "/api/settings",
            "/api/scenarios", "/api/scenario-run", "/api/doctor", "/api/map", "/api/graph",
            "/api/brief", "/api/clean", "/api/root", "/api/remove-root", "/api/refresh",
            "/api/reindex", "/api/reindex-folder", "/api/remove-folder", "/api/deep-index",
            "/api/deep-index-target", "/api/index-selection", "/api/exclude-selection",
            "/api/jobs/", "/cancel", "/download", "/reveal", "/open"] {
            assert!(script.contains(route), "missing route {route}");
        }
        for unsafe_api in ["innerHTML", "outerHTML", "insertAdjacentHTML", "document.write", "eval(", "localStorage"] {
            assert!(!INDEX_HTML.contains(unsafe_api), "forbidden API {unsafe_api}");
        }
        assert!(script.contains("value.forEach(item=>p.append(key,String(item)))"));
        assert!(script.contains("output(\"시나리오 결과\",\"/api/scenario-run\",{id:select.value})"));
        assert!(script.contains("Promise.allSettled([loadStatus(),loadRoots()"));
    }
}
