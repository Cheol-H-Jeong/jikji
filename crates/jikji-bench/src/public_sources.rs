use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use jikji_core::{JikjiError, Result, io_error, json_error};
use serde::Serialize;
use serde_json::{Value, json};
use tar::Archive;
use url::Url;

use crate::public_datasets::{
    EdithAnswer, EdithBuildResult, EdithFixture, EdithMasterRow, EdithOptions, PublicDataDocument,
    PublicDataOptions, SplitDatasetBuildResult, build_publicdata_fixture,
    materialize_edith_fixture,
};

pub const EDITH_BASE_URL: &str =
    "https://huggingface.co/datasets/lightonai/veracier-industries/resolve/main";
pub const SEOUL_VIEW_URL: &str = "https://data.seoul.go.kr/bsp/wgs/dataView/data300View/{id}.do";
pub const SEOUL_XLSX_URL: &str = "https://data.seoul.go.kr/bsp/wgs/dataset/dataXlsxDown.do";

#[derive(Debug, Clone)]
pub struct DownloadLimits {
    pub timeout: Duration,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for DownloadLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(180),
            max_file_bytes: 2_000_000_000,
            max_total_bytes: 2_100_000_000,
        }
    }
}

pub trait WebClient {
    fn get(&self, url: &str, limits: &DownloadLimits) -> io::Result<Vec<u8>>;
    fn post_form(
        &self,
        url: &str,
        fields: &[(&str, String)],
        limits: &DownloadLimits,
    ) -> io::Result<Vec<u8>>;
}

#[derive(Debug, Clone, Default)]
pub struct RustHttpClient;

impl WebClient for RustHttpClient {
    fn get(&self, url: &str, limits: &DownloadLimits) -> io::Result<Vec<u8>> {
        request_bytes(
            ureq::get(url).timeout(limits.timeout).call(),
            url,
            limits.max_file_bytes,
        )
    }

    fn post_form(
        &self,
        url: &str,
        fields: &[(&str, String)],
        limits: &DownloadLimits,
    ) -> io::Result<Vec<u8>> {
        let encoded = fields
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        request_bytes(
            ureq::post(url)
                .timeout(limits.timeout)
                .set("Content-Type", "application/x-www-form-urlencoded")
                .send_string(&encoded),
            url,
            limits.max_file_bytes,
        )
    }
}

fn request_bytes(
    response: std::result::Result<ureq::Response, ureq::Error>,
    resource: &str,
    max_bytes: u64,
) -> io::Result<Vec<u8>> {
    let response =
        response.map_err(|error| io::Error::other(format!("HTTP {resource}: {error}")))?;
    if response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > max_bytes)
    {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("download exceeds {max_bytes} bytes: {resource}"),
        ));
    }
    read_bounded(response.into_reader(), resource, max_bytes)
}

fn read_bounded(mut reader: impl Read, resource: &str, max_bytes: u64) -> io::Result<Vec<u8>> {
    let capacity = usize::try_from(max_bytes.min(1024 * 1024)).unwrap_or(1024 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() as u64 + count as u64 > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("download exceeds {max_bytes} bytes: {resource}"),
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

#[derive(Debug, Clone)]
pub struct EdithDownloadOptions {
    pub base_url: String,
    pub max_cases: usize,
    pub max_docs: usize,
    pub download_docs: bool,
    pub limits: DownloadLimits,
}

impl Default for EdithDownloadOptions {
    fn default() -> Self {
        Self {
            base_url: EDITH_BASE_URL.to_owned(),
            max_cases: 8,
            max_docs: 60,
            download_docs: true,
            limits: DownloadLimits::default(),
        }
    }
}

pub fn download_edith(
    client: &impl WebClient,
    dest: &Path,
    options: &EdithDownloadOptions,
) -> Result<EdithBuildResult> {
    validate_http_base(&options.base_url, "EDiTh base URL")?;
    if options.max_cases == 0 || options.max_docs == 0 {
        return Err(invalid("EDiTh case and document limits must be positive"));
    }
    let base = options.base_url.trim_end_matches('/');
    let master_url = format!("{base}/MASTER_INDEX.csv");
    let answers_url = format!("{base}/ANSWER_KEY.json");
    let master_bytes = client
        .get(&master_url, &options.limits)
        .map_err(|e| io_error(&master_url, e))?;
    let answer_bytes = client
        .get(&answers_url, &options.limits)
        .map_err(|e| io_error(&answers_url, e))?;
    let mut total = checked_total(0, master_bytes.len(), &options.limits)?;
    total = checked_total(total, answer_bytes.len(), &options.limits)?;
    let master_index = parse_master_index(&master_bytes)?;
    let answers: BTreeMap<String, EdithAnswer> =
        serde_json::from_slice(&answer_bytes).map_err(|source| json_error(&answers_url, source))?;
    if answers.is_empty() {
        return Err(invalid("EDiTh ANSWER_KEY.json is empty"));
    }
    let wanted = selected_edith_paths(&master_index, &answers, options.max_cases, options.max_docs);
    let documents = if options.download_docs && !wanted.is_empty() {
        let archive_url = format!("{base}/by_entity.tar.gz");
        let archive = client
            .get(&archive_url, &options.limits)
            .map_err(|e| io_error(&archive_url, e))?;
        let _ = checked_total(total, archive.len(), &options.limits)?;
        extract_selected_edith(&archive, &wanted, options.limits.max_total_bytes)?
    } else {
        BTreeMap::new()
    };
    materialize_edith_fixture(
        dest,
        &EdithFixture {
            master_index,
            answers,
            documents,
        },
        EdithOptions {
            max_cases: options.max_cases,
            max_docs: options.max_docs,
        },
    )
}

fn parse_master_index(bytes: &[u8]) -> Result<Vec<EdithMasterRow>> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| invalid(error.to_string()))?
        .clone();
    let filename = headers
        .iter()
        .position(|name| name.trim() == "filename")
        .ok_or_else(|| invalid("EDiTh MASTER_INDEX.csv lacks filename column"))?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| invalid(error.to_string()))?;
        let value = record
            .get(filename)
            .unwrap_or_default()
            .trim()
            .replace('\\', "/");
        if !value.is_empty() {
            rows.push(EdithMasterRow { filename: value });
        }
    }
    if rows.is_empty() {
        return Err(invalid("EDiTh MASTER_INDEX.csv has no documents"));
    }
    Ok(rows)
}

fn selected_edith_paths(
    master: &[EdithMasterRow],
    answers: &BTreeMap<String, EdithAnswer>,
    max_cases: usize,
    max_docs: usize,
) -> BTreeSet<String> {
    let known = master
        .iter()
        .map(|row| row.filename.as_str())
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    for answer in answers.values().take(max_cases) {
        let mut paths = Vec::new();
        flatten_pdf_values(&answer.ground_truth, &mut paths);
        for path in paths {
            if selected.len() >= max_docs {
                return selected;
            }
            let normalized = path.replace('\\', "/").trim_matches('/').to_owned();
            if known.contains(normalized.as_str()) {
                selected.insert(normalized);
            }
        }
    }
    selected
}

fn flatten_pdf_values(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(path) if path.to_ascii_lowercase().ends_with(".pdf") => {
            out.push(path.clone())
        }
        Value::Array(values) => values
            .iter()
            .for_each(|value| flatten_pdf_values(value, out)),
        Value::Object(values) => values
            .values()
            .for_each(|value| flatten_pdf_values(value, out)),
        _ => {}
    }
}

fn extract_selected_edith(
    compressed: &[u8],
    wanted: &BTreeSet<String>,
    max_expanded_bytes: u64,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let decoder = GzDecoder::new(compressed);
    let mut archive = Archive::new(decoder);
    let mut found = BTreeMap::new();
    let mut expanded = 0_u64;
    let entries = archive
        .entries()
        .map_err(|e| io_error("EDiTh archive", e))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| io_error("EDiTh archive", e))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| io_error("EDiTh archive path", e))?;
        let normalized = path.to_string_lossy().replace('\\', "/");
        let suffix = normalized.strip_prefix("by_entity/").unwrap_or(&normalized);
        let Some(source) = wanted
            .iter()
            .find(|wanted_path| suffix.ends_with(wanted_path.as_str()))
        else {
            continue;
        };
        let bytes = read_bounded(
            &mut entry,
            suffix,
            max_expanded_bytes.saturating_sub(expanded),
        )
        .map_err(|e| io_error(suffix, e))?;
        expanded = expanded.saturating_add(bytes.len() as u64);
        found.insert(source.clone(), bytes);
        if found.len() == wanted.len() {
            break;
        }
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct PublicDataDownloadOptions {
    pub view_url: String,
    pub xlsx_url: String,
    pub target_docs: usize,
    pub max_id: usize,
    pub max_cases: usize,
    pub seed: u64,
    pub limits: DownloadLimits,
}

impl Default for PublicDataDownloadOptions {
    fn default() -> Self {
        Self {
            view_url: SEOUL_VIEW_URL.to_owned(),
            xlsx_url: SEOUL_XLSX_URL.to_owned(),
            target_docs: 90,
            max_id: 700,
            max_cases: 40,
            seed: 20_260_529,
            limits: DownloadLimits {
                timeout: Duration::from_secs(90),
                max_file_bytes: 64 * 1024 * 1024,
                max_total_bytes: 1024 * 1024 * 1024,
            },
        }
    }
}

pub fn download_publicdata(
    client: &impl WebClient,
    dest: &Path,
    options: &PublicDataDownloadOptions,
) -> Result<SplitDatasetBuildResult> {
    validate_http_template(&options.view_url)?;
    validate_http_base(&options.xlsx_url, "public-data XLSX URL")?;
    if options.target_docs < 3 || options.max_id == 0 || options.max_cases == 0 {
        return Err(invalid(
            "public-data target_docs must be at least 3 and other limits positive",
        ));
    }
    let mut documents = Vec::new();
    let mut failures = Vec::new();
    let mut total = 0_u64;
    for id in 1..=options.max_id {
        if documents.len() >= options.target_docs {
            break;
        }
        let xlsx = match client.post_form(
            &options.xlsx_url,
            &[
                ("id", id.to_string()),
                ("tdColNmArr", String::new()),
                ("rowFilterList", "[]".to_owned()),
            ],
            &options.limits,
        ) {
            Ok(bytes) if looks_like_xlsx(&bytes) => bytes,
            Ok(_) => {
                failures.push(json!({"id": id, "reason": "not_xlsx"}));
                continue;
            }
            Err(error) => {
                failures.push(json!({"id": id, "reason": error.to_string()}));
                continue;
            }
        };
        total = checked_total(total, xlsx.len(), &options.limits)?;
        let view_url = options.view_url.replace("{id}", &id.to_string());
        let html = match client.get(&view_url, &options.limits) {
            Ok(bytes) => bytes,
            Err(error) => {
                failures.push(json!({"id": id, "reason": error.to_string()}));
                continue;
            }
        };
        total = checked_total(total, html.len(), &options.limits)?;
        let html = String::from_utf8_lossy(&html);
        let title =
            html_field(&html, "trgtTableIndctNm").unwrap_or_else(|| format!("dataset-{id}"));
        let description = html_field(&html, "dsDesc").unwrap_or_default();
        let filename = format!("seoul_{id}.xlsx");
        documents.push(PublicDataDocument {
            id: id.to_string(),
            filename,
            title,
            description,
            xlsx_text: xlsx_text(&xlsx, 240).unwrap_or_default(),
            source_url: view_url,
            license_note: "Seoul Data Hub public open-data download".to_owned(),
            bytes: xlsx,
        });
    }
    if documents.len() < 3 {
        return Err(invalid(format!(
            "too few public-data documents downloaded: {}",
            documents.len()
        )));
    }
    fs::create_dir_all(dest).map_err(|e| io_error(dest, e))?;
    let failure_path = dest.join("download_failures.json");
    let failure_value = json!({"count": failures.len(), "failures": failures});
    fs::write(
        &failure_path,
        serde_json::to_vec_pretty(&failure_value).map_err(|e| json_error(&failure_path, e))?,
    )
    .map_err(|e| io_error(&failure_path, e))?;
    build_publicdata_fixture(
        dest,
        &documents,
        PublicDataOptions {
            max_cases_per_split: options.max_cases,
            seed: options.seed,
        },
    )
}

fn looks_like_xlsx(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"PK") {
        return false;
    }
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .ok()
        .is_some_and(|mut archive| archive.by_name("[Content_Types].xml").is_ok())
}

fn xlsx_text(bytes: &[u8], max_cells: usize) -> io::Result<Vec<String>> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(io::Error::other)?;
    let mut strings = Vec::new();
    for name in ["xl/sharedStrings.xml", "xl/workbook.xml"] {
        let Ok(mut file) = archive.by_name(name) else {
            continue;
        };
        let mut xml = String::new();
        file.read_to_string(&mut xml)?;
        for text in xml_tag_text(&xml, "t")
            .into_iter()
            .chain(xml_tag_text(&xml, "sheet"))
        {
            let value = decode_xml(&text);
            if !value.trim().is_empty() && !strings.contains(&value) {
                strings.push(value);
            }
            if strings.len() >= max_cells {
                return Ok(strings);
            }
        }
    }
    Ok(strings)
}

fn html_field(html: &str, class_name: &str) -> Option<String> {
    let marker = format!("class=\"{class_name}");
    let start = html.find(&marker)?;
    let body = &html[start..];
    let body = &body[body.find('>')? + 1..];
    let end = body.find('<')?;
    let value = strip_tags(&body[..end]).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn xml_tag_text(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut cursor = xml;
    let mut values = Vec::new();
    while let Some(start) = cursor.find(&open) {
        cursor = &cursor[start + open.len()..];
        let Some(body_start) = cursor.find('>') else {
            break;
        };
        cursor = &cursor[body_start + 1..];
        let Some(end) = cursor.find(&close) else {
            break;
        };
        values.push(strip_tags(&cursor[..end]));
        cursor = &cursor[end + close.len()..];
    }
    values
}

fn strip_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    decode_xml(&output)
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn checked_total(total: u64, added: usize, limits: &DownloadLimits) -> Result<u64> {
    let total = total.saturating_add(added as u64);
    if total > limits.max_total_bytes {
        return Err(invalid(format!(
            "total download exceeds {} bytes",
            limits.max_total_bytes
        )));
    }
    Ok(total)
}

fn validate_http_base(value: &str, name: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|e| invalid(format!("invalid {name}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid(format!("{name} must be an HTTP(S) URL")));
    }
    Ok(())
}

fn validate_http_template(value: &str) -> Result<()> {
    if !value.contains("{id}") {
        return Err(invalid("public-data view URL must contain {id}"));
    }
    validate_http_base(&value.replace("{id}", "1"), "public-data view URL")
}

fn invalid(message: impl Into<String>) -> JikjiError {
    io_error(
        "public dataset",
        io::Error::new(io::ErrorKind::InvalidInput, message.into()),
    )
}

#[derive(Debug, Serialize)]
pub struct PublicSuiteResult {
    pub build: Value,
    pub benchmark_report: Option<PathBuf>,
    pub metrics: Value,
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io::{self, Write};

    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;

    #[derive(Default)]
    struct FixtureWeb {
        get: BTreeMap<String, io::Result<Vec<u8>>>,
        posts: RefCell<Vec<Vec<u8>>>,
    }

    impl WebClient for FixtureWeb {
        fn get(&self, url: &str, _limits: &DownloadLimits) -> io::Result<Vec<u8>> {
            match self.get.get(url) {
                Some(Ok(bytes)) => Ok(bytes.clone()),
                Some(Err(error)) => Err(io::Error::new(error.kind(), error.to_string())),
                None => Err(io::Error::new(io::ErrorKind::NotFound, url.to_owned())),
            }
        }

        fn post_form(
            &self,
            _url: &str,
            _fields: &[(&str, String)],
            _limits: &DownloadLimits,
        ) -> io::Result<Vec<u8>> {
            let mut posts = self.posts.borrow_mut();
            if posts.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    "fixture exhausted",
                ));
            }
            Ok(posts.remove(0))
        }
    }

    #[test]
    fn edith_url_adapter_downloads_real_metadata_and_tar_materialization() {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = "https://fixture.invalid/edith";
        let client = FixtureWeb {
            get: BTreeMap::from([
                (format!("{base}/MASTER_INDEX.csv"), Ok(b"filename\ncompany/report.pdf\n".to_vec())),
                (format!("{base}/ANSWER_KEY.json"), Ok(br#"{"q1":{"question":"Find signed revenue","ground_truth":["company/report.pdf"],"role":"analyst","entity":"company"}}"#.to_vec())),
                (format!("{base}/by_entity.tar.gz"), Ok(edith_archive("by_entity/company/report.pdf", b"%PDF-1.4 real fixture document"))),
            ]),
            posts: RefCell::new(Vec::new()),
        };
        let result = download_edith(
            &client,
            temp.path(),
            &EdithDownloadOptions {
                base_url: base.to_owned(),
                max_cases: 1,
                max_docs: 1,
                download_docs: true,
                limits: DownloadLimits {
                    timeout: Duration::from_secs(1),
                    max_file_bytes: 1024 * 1024,
                    max_total_bytes: 2 * 1024 * 1024,
                },
            },
        )
        .expect("download EDiTh");
        assert_eq!((result.selected_questions, result.extracted_docs), (1, 1));
        assert!(result.corpus_root.join("company/report.pdf").is_file());
        assert!(
            fs::read_to_string(result.eval_set_path)
                .expect("eval")
                .contains("Find signed revenue")
        );
    }

    #[test]
    fn publicdata_url_adapter_uses_posted_xlsx_and_html_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = "https://fixture.invalid/public";
        let client = FixtureWeb {
            get: (1..=3).map(|id| (format!("{base}/view/{id}"), Ok(format!("<h1 class=\"trgtTableIndctNm\">서울 데이터 {id}</h1><div class=\"dsDesc\">공공 설명 {id}</div>").into_bytes()))).collect(),
            posts: RefCell::new((1..=3).map(|id| xlsx_fixture(&format!("고유 지표 {id}"))).collect()),
        };
        let result = download_publicdata(
            &client,
            temp.path(),
            &PublicDataDownloadOptions {
                view_url: format!("{base}/view/{{id}}"),
                xlsx_url: format!("{base}/xlsx"),
                target_docs: 3,
                max_id: 3,
                max_cases: 3,
                seed: 7,
                limits: DownloadLimits {
                    timeout: Duration::from_secs(1),
                    max_file_bytes: 1024 * 1024,
                    max_total_bytes: 4 * 1024 * 1024,
                },
            },
        )
        .expect("download publicdata");
        assert_eq!(result.docs, 3);
        assert!(result.eval_set_path.is_file());
        assert!(
            fs::read_to_string(temp.path().join("metadata/test_docs.jsonl"))
                .expect("metadata")
                .contains("서울 데이터")
        );
    }

    #[test]
    fn adapters_reject_bad_input_network_errors_and_byte_limits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let invalid = download_edith(
            &FixtureWeb::default(),
            temp.path(),
            &EdithDownloadOptions {
                base_url: "file:///tmp/x".to_owned(),
                ..EdithDownloadOptions::default()
            },
        )
        .expect_err("bad URL");
        assert!(invalid.to_string().contains("HTTP(S)"));

        let network = download_edith(
            &FixtureWeb::default(),
            temp.path(),
            &EdithDownloadOptions {
                base_url: "https://fixture.invalid".to_owned(),
                ..EdithDownloadOptions::default()
            },
        )
        .expect_err("network");
        assert!(network.to_string().contains("MASTER_INDEX.csv"));

        let limited = read_bounded(&b"12345"[..], "fixture", 4).expect_err("byte limit");
        assert_eq!(limited.kind(), io::ErrorKind::FileTooLarge);
    }

    fn edith_archive(path: &str, bytes: &[u8]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, bytes)
            .expect("append");
        archive
            .into_inner()
            .expect("encoder")
            .finish()
            .expect("gzip")
    }

    fn xlsx_fixture(text: &str) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            archive
                .start_file("[Content_Types].xml", options)
                .expect("content types");
            archive.write_all(b"<Types/>").expect("write");
            archive
                .start_file("xl/sharedStrings.xml", options)
                .expect("strings");
            archive
                .write_all(format!("<sst><si><t>{text}</t></si></sst>").as_bytes())
                .expect("write");
            archive.finish().expect("finish");
        }
        cursor.into_inner()
    }
}
