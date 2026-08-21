use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use tempfile::tempdir;

#[test]
fn edith_import_and_suite_use_http_metadata_and_archive() {
    let fixture = FixtureServer::start();
    let temp = tempdir().expect("tempdir");
    let import = run([
        "edith-import",
        temp.path().join("import").to_str().unwrap(),
        "--base-url",
        &format!("{}/edith", fixture.url),
        "--cases",
        "1",
        "--json",
    ]);
    assert_success(&import);
    let payload: serde_json::Value = serde_json::from_slice(&import.stdout).expect("import JSON");
    assert_eq!(payload["extracted_docs"], 1);
    assert!(
        temp.path()
            .join("import/corpus/company/report.pdf")
            .is_file()
    );

    let suite = run([
        "edith-suite",
        temp.path().join("suite").to_str().unwrap(),
        "--base-url",
        &format!("{}/edith", fixture.url),
        "--cases",
        "1",
        "--json",
    ]);
    assert_success(&suite);
    let payload: serde_json::Value = serde_json::from_slice(&suite.stdout).expect("suite JSON");
    assert_eq!(payload["build"]["extracted_docs"], 1);
    assert!(payload["benchmark_report"].as_str().is_some());
}

#[test]
fn publicdata_build_and_suite_use_http_post_xlsx_adapter() {
    let fixture = FixtureServer::start();
    let temp = tempdir().expect("tempdir");
    let build = run([
        "publicdata-build",
        temp.path().join("build").to_str().unwrap(),
        "--base-url",
        &format!("{}/public", fixture.url),
        "--cases",
        "3",
        "--json",
    ]);
    assert_success(&build);
    let payload: serde_json::Value = serde_json::from_slice(&build.stdout).expect("build JSON");
    assert_eq!(payload["docs"], 3);
    assert!(temp.path().join("build/corpus/test").is_dir());

    let suite = run([
        "publicdata-suite",
        temp.path().join("suite").to_str().unwrap(),
        "--base-url",
        &format!("{}/public", fixture.url),
        "--cases",
        "3",
        "--json",
    ]);
    assert_success(&suite);
    let payload: serde_json::Value = serde_json::from_slice(&suite.stdout).expect("suite JSON");
    assert_eq!(payload["build"]["docs"], 3);

    let unprepared = run([
        "edith-suite",
        temp.path().join("unprepared").to_str().unwrap(),
        "--base-url",
        &format!("{}/edith", fixture.url),
        "--cases",
        "1",
        "--no-prepare",
    ]);
    assert!(!unprepared.status.success());
    assert!(String::from_utf8_lossy(&unprepared.stderr).contains("already prepared corpus index"));
    assert!(payload["benchmark_report"].as_str().is_some());
}

#[test]
fn public_cli_surfaces_network_and_invalid_limit_errors() {
    let temp = tempdir().expect("tempdir");
    let invalid = run([
        "edith-import",
        temp.path().join("invalid").to_str().unwrap(),
        "--base-url",
        "file:///tmp/not-http",
        "--cases",
        "1",
    ]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("HTTP(S)"));

    let network = run([
        "publicdata-build",
        temp.path().join("network").to_str().unwrap(),
        "--base-url",
        "http://127.0.0.1:9",
        "--cases",
        "3",
        "--timeout-seconds",
        "1",
    ]);
    assert!(!network.status.success());
    assert!(String::from_utf8_lossy(&network.stderr).contains("too few public-data documents"));

    let limits = run([
        "edith-suite",
        temp.path().join("limits").to_str().unwrap(),
        "--base-url",
        "http://127.0.0.1:9",
        "--max-file-bytes",
        "10",
        "--max-total-bytes",
        "9",
    ]);
    assert!(!limits.status.success());
    assert!(String::from_utf8_lossy(&limits.stderr).contains("max-file-bytes"));
}

fn run<const N: usize>(args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jikji"))
        .args(args)
        .output()
        .expect("run jikji")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct FixtureServer {
    url: String,
}

impl FixtureServer {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fixture");
        let address = listener.local_addr().expect("address");
        thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    serve(&mut stream);
                }
            }
        });
        Self {
            url: format!("http://{address}"),
        }
    }
}

fn serve(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let Ok(count) = stream.read(&mut buffer) else {
            return;
        };
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let line = String::from_utf8_lossy(&request);
    let path = line
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, body) = match path {
        "/edith/MASTER_INDEX.csv" => ("200 OK", "text/csv", b"filename\ncompany/report.pdf\n".to_vec()),
        "/edith/ANSWER_KEY.json" => ("200 OK", "application/json", br#"{"q1":{"question":"Find signed revenue","ground_truth":["company/report.pdf"],"role":"analyst","entity":"company"}}"#.to_vec()),
        "/edith/by_entity.tar.gz" => ("200 OK", "application/gzip", edith_archive()),
        "/public/xlsx" => ("200 OK", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", xlsx_fixture()),
        path if path.starts_with("/public/view/") => {
            let id = path.trim_start_matches("/public/view/");
            ("200 OK", "text/html; charset=utf-8", format!("<h1 class=\"trgtTableIndctNm\">서울 교통 데이터 {id}</h1><div class=\"dsDesc\">노선별 고유 지표 {id}</div>").into_bytes())
        }
        _ => ("404 Not Found", "text/plain", b"missing".to_vec()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("response headers");
    stream.write_all(&body).expect("response body");
}

fn edith_archive() -> Vec<u8> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let bytes = b"%PDF-1.4 fixture with signed revenue evidence";
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "by_entity/company/report.pdf", &bytes[..])
        .expect("append PDF");
    archive
        .into_inner()
        .expect("encoder")
        .finish()
        .expect("gzip")
}

fn xlsx_fixture() -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default();
        archive
            .start_file("[Content_Types].xml", options)
            .expect("types");
        archive.write_all(b"<Types/>").expect("types body");
        archive
            .start_file("xl/sharedStrings.xml", options)
            .expect("strings");
        archive
            .write_all("<sst><si><t>노선별 고유 운행 지표</t></si></sst>".as_bytes())
            .expect("strings body");
        archive.finish().expect("finish XLSX");
    }
    cursor.into_inner()
}

#[allow(dead_code)]
fn assert_path(path: &Path) {
    assert!(fs::metadata(path).is_ok());
}
