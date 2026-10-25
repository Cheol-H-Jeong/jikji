//! Snippet mode and page-image mode for the console's preview pane.
//!
//! Snippets come only from what the index already holds (parser text cache, chunk rows) or, for
//! native text files, the file itself, so they work for every indexed file without converters.
//! Page images render PDFs (native, or Office/HWP converted to PDF) to PNG on the server with
//! `hayro` (Apache-2.0 OR MIT), so they do not depend on the browser having a PDF viewer.

use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use jikji_core::storage::{existing_root_storage_dir, load_artifact_by_path, load_artifacts_for_path};
use serde_json::{Value, json};

/// Snippet text sent to the browser at most.
pub(crate) const MAX_SNIPPET_TEXT_CHARS: usize = 200_000;
/// Chunk rows returned with an `index_chunks` snippet.
const MAX_SNIPPET_CHUNKS: usize = 400;
/// Pages listed for page-image preview.
pub(crate) const MAX_PREVIEW_PAGES: usize = 30;
/// Rendered page width cap in pixels.
const MAX_PAGE_WIDTH_PX: f32 = 1400.0;
/// Rendering resolution in dots per inch before the width cap applies.
const PAGE_DPI: f32 = 110.0;
/// Most pixels in one rendered page (16 MP ≈ 64 MB RGBA); far below vello_cpu's u16 limit.
const MAX_PAGE_PIXELS: f32 = 16_000_000.0;
/// Converted documents kept for paging, and their total size.
const PDF_CACHE_DOCS: usize = 16;
const PDF_CACHE_BYTES: usize = 256 * 1024 * 1024;
/// Largest PDF read for page images; bigger files are offered as a download instead.
pub(crate) const MAX_PAGE_PREVIEW_PDF_BYTES: u64 = 200 * 1024 * 1024;
/// Share of replacement / control characters above which indexed text counts as unreadable.
const UNREADABLE_RATIO: f64 = 0.2;

/// The snippet payload for `rel_path` under `root` (`full` is the resolved file on disk, if any).
pub(crate) fn snippet_payload(root: &Path, rel_path: &str, full: Option<&Path>, query: &str) -> Value {
    let file_row = load_artifact_by_path(root, "files", rel_path).ok().flatten();
    let metadata = metadata_json(rel_path, full, file_row.as_ref());

    if let Some(row) = &file_row {
        let cache_path = row.get("text_cache_path").and_then(Value::as_str).unwrap_or_default();
        if let Some(text) = read_index_cache(root, cache_path) {
            return text_payload(rel_path, "index_cache", text, query, metadata, Vec::new());
        }
    }
    let chunks = load_artifacts_for_path(root, "chunks", rel_path, MAX_SNIPPET_CHUNKS).unwrap_or_default();
    if !chunks.is_empty() {
        let mut text = String::new();
        let mut parts = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let body = chunk
                .get("body_text")
                .or_else(|| chunk.get("preview"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if body.trim().is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(body);
            parts.push(json!({
                "text": body,
                "page_hint": chunk.get("page_hint").cloned().unwrap_or(Value::Null),
                "slide_hint": chunk.get("slide_hint").cloned().unwrap_or(Value::Null),
                "sheet_hint": chunk.get("sheet_hint").cloned().unwrap_or(Value::Null),
                "heading_hint": chunk.get("heading_hint").cloned().unwrap_or(Value::Null),
            }));
        }
        if !text.is_empty() && readable(&text) {
            return text_payload(rel_path, "index_chunks", text, query, metadata, parts);
        }
    }
    if let Some(path) = full.filter(|path| path.is_file()) {
        // Indexed text is missing or was produced by an older parser (raw OLE bytes for legacy
        // .doc/.ppt/.hwp). Extract it from the original now so the snippet is never empty.
        if let Some(text) = parse_original(path) {
            return text_payload(rel_path, "parsed_now", text, query, metadata, Vec::new());
        }
        if let Ok(Some((text, truncated))) = super::routing::read_text_preview(path) {
            let mut payload = text_payload(rel_path, "file_text", text, query, metadata, Vec::new());
            if truncated {
                payload["truncated"] = json!(true);
            }
            return payload;
        }
    }
    let reason = if file_row.is_some() { "unreadable_text" } else { "not_indexed" };
    metadata_payload(rel_path, reason, metadata)
}

/// Largest original file parsed on demand for a snippet.
const MAX_PARSE_NOW_BYTES: u64 = 64 * 1024 * 1024;

/// Extensions parsed on demand: the legacy formats older releases cached as raw container bytes,
/// handled by Jikji's own bounds-checked parsers. Third-party extractors (PDF in particular) can
/// panic on hostile files, and with `panic = "abort"` that would take the whole console down.
const PARSE_NOW_EXTENSIONS: &[&str] = &["doc", "dot", "ppt", "pps", "pot", "hwp", "rtf"];

/// Text extracted from the original file with the indexer's own parsers, when they know the
/// format and produce readable text. Plain text files are left to `read_text_preview`.
fn parse_original(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !PARSE_NOW_EXTENSIONS.contains(&ext.as_str()) || path.metadata().ok()?.len() > MAX_PARSE_NOW_BYTES {
        return None;
    }
    let registry = jikji_parser::ParserRegistry::with_defaults();
    let parsed = registry.parse_path(path, MAX_SNIPPET_TEXT_CHARS);
    let parsed_ok = parsed.status == jikji_parser::ParseStatus::Success
        && parsed.parser_name != "plain-text"
        && !parsed.text.trim().is_empty()
        && readable(&parsed.text);
    parsed_ok.then_some(parsed.text)
}

/// Parser text cache for this file, or `None` when missing, empty or unreadable.
fn read_index_cache(root: &Path, cache_path: &str) -> Option<String> {
    let rel = cache_path.strip_prefix(".jikji/").unwrap_or(cache_path);
    let rel = Path::new(rel);
    if rel.as_os_str().is_empty()
        || rel.is_absolute()
        || rel.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let storage = existing_root_storage_dir(root).ok().flatten()?;
    let limit = MAX_SNIPPET_TEXT_CHARS.saturating_mul(4);
    let mut bytes = Vec::new();
    fs::File::open(storage.join(rel))
        .ok()?
        .take(u64::try_from(limit).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let body = strip_cache_header(&text);
    (!body.trim().is_empty() && readable(body)).then(|| body.to_owned())
}

/// The parser cache starts with `# Source:`, `# File ID:` and `# Parsed by:` lines and a blank
/// line (`doc_text_cache.rs`); they describe the cache, not the document.
fn strip_cache_header(text: &str) -> &str {
    let mut rest = text;
    for prefix in ["# Source:", "# File ID:", "# Parsed by:"] {
        if !rest.starts_with(prefix) {
            break;
        }
        rest = rest.split_once('\n').map_or("", |(_, tail)| tail);
    }
    rest.strip_prefix('\n').unwrap_or(rest)
}

/// Old releases cached container bytes of legacy .doc/.ppt/.xls/.hwp (and renamed .docx) as
/// text. That shows as the OLE header (`þÿÿÿ`), zip headers, the HWP stream directory, or binary
/// decoded as UTF-16, which looks like scattered CJK and private-use characters. None of it may
/// be shown as a snippet.
fn readable(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.starts_with("þÿÿÿ")
        || trimmed.starts_with("PK")
        || trimmed.starts_with("Root Entry")
        || trimmed.contains("[Content_Types].xml")
    {
        return false;
    }
    let (mut total, mut bad, mut cjk, mut hangul, mut ascii_words) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut in_word = false;
    for ch in text.chars().take(4000) {
        total += 1;
        let code = ch as u32;
        let suspicious = ch == '\u{fffd}'
            || ch == 'ÿ'
            || ch == 'þ'
            || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
            || (0xE000..=0xF8FF).contains(&code)
            || (0x2000..=0x206F).contains(&code) && !matches!(ch, '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}')
            || (0x1100..=0x11FF).contains(&code)
            || (0x0600..=0x0FFF).contains(&code)
            || (0x1200..=0x18FF).contains(&code);
        if suspicious {
            bad += 1;
        }
        if (0x4E00..=0x9FFF).contains(&code) || (0x3400..=0x4DBF).contains(&code) {
            cjk += 1;
        }
        if (0xAC00..=0xD7A3).contains(&code) {
            hangul += 1;
        }
        let letter = ch.is_ascii_alphabetic();
        if letter && !in_word {
            ascii_words += 1;
        }
        in_word = letter;
    }
    if total == 0 {
        return true;
    }
    // Binary read as UTF-16 lands mostly in the CJK ideograph block with almost no Hangul or
    // Latin words around it; real Chinese/Japanese text is rare in this corpus but still passes
    // when it is not mixed with the other noise signals.
    let ideograph_noise = cjk * 4 > total && hangul * 10 < cjk && bad > 0;
    (bad as f64) / (total as f64) < UNREADABLE_RATIO
        && !ideograph_noise
        && !mixed_script_noise(text)
        && (hangul > 0 || ascii_words > 0 || cjk > 0 || bad == 0)
}

/// Script group of a character, for spotting binary decoded as UTF-16: such text jumps between
/// unrelated writing systems (Han, Hangul, kana, Cyrillic, Armenian, Ethiopic, …) every few
/// characters, which real documents never do.
fn script_group(ch: char) -> Option<u8> {
    let code = ch as u32;
    Some(match code {
        0x0041..=0x007A | 0x00C0..=0x024F => 0,
        0x0370..=0x03FF => 1,
        0x0400..=0x052F => 2,
        0x0530..=0x058F => 3,
        0x0590..=0x05FF => 4,
        0x0600..=0x0FFF => 5,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 => 6,
        0x1200..=0x18FF => 7,
        0x3040..=0x30FF => 8,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF => 9,
        0xA000..=0xABFF => 10,
        0xE000..=0xF8FF => 11,
        _ => return None,
    })
}

/// True when, across 40-character windows, letters switch between four or more script groups.
fn mixed_script_noise(text: &str) -> bool {
    let letters: Vec<u8> = text.chars().take(4000).filter_map(script_group).collect();
    if letters.len() < 12 {
        return false;
    }
    let noisy_windows = letters
        .chunks(40)
        .filter(|window| {
            let mut seen = [false; 12];
            for &group in *window {
                seen[group as usize] = true;
            }
            seen.iter().filter(|&&present| present).count() >= 4
        })
        .count();
    noisy_windows * 3 >= letters.len().div_ceil(40)
}

fn text_payload(
    rel_path: &str,
    source: &str,
    text: String,
    query: &str,
    metadata: Value,
    chunks: Vec<Value>,
) -> Value {
    let truncated = text.chars().count() > MAX_SNIPPET_TEXT_CHARS;
    let text = if truncated { text.chars().take(MAX_SNIPPET_TEXT_CHARS).collect() } else { text };
    let mut payload = json!({
        "path": rel_path,
        "source": source,
        "matches": super::routing::match_ranges(&text, query),
        "text": text,
        "truncated": truncated,
        "metadata": metadata,
    });
    if !chunks.is_empty() {
        payload["chunks"] = Value::Array(chunks);
    }
    payload
}

fn metadata_payload(rel_path: &str, reason: &str, metadata: Value) -> Value {
    json!({
        "path": rel_path,
        "source": "metadata",
        "reason": reason,
        "text": "",
        "truncated": false,
        "matches": [],
        "metadata": metadata,
    })
}

fn metadata_json(rel_path: &str, full: Option<&Path>, row: Option<&Value>) -> Value {
    let field = |name: &str| row.and_then(|row| row.get(name)).cloned().unwrap_or(Value::Null);
    let size = full
        .and_then(|path| path.metadata().ok())
        .map(|metadata| json!(metadata.len()))
        .unwrap_or_else(|| field("size"));
    let summary = row
        .and_then(|row| row.get("summary"))
        .and_then(Value::as_str)
        .filter(|summary| readable(summary))
        .map_or(Value::Null, |summary| json!(summary));
    json!({
        "name": Path::new(rel_path).file_name().map(|name| name.to_string_lossy()),
        "size": size,
        "ext": field("ext"),
        "parse_status": field("parse_status"),
        "parser": field("parser"),
        "summary": summary,
    })
}

/// A converted or native PDF held in memory so paging does not re-run the converter.
struct CachedPdf {
    key: String,
    bytes: Arc<Vec<u8>>,
}

static PDF_CACHE: LazyLock<Mutex<VecDeque<CachedPdf>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
/// Held while a cache miss is produced, so concurrent requests for a cold document wait for
/// one conversion instead of each starting their own.
static PDF_PRODUCE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Cache key: path, size and modification time, so an edited file is converted again.
fn cache_key(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some(format!("{}\u{0}{}\u{0}{modified}", path.display(), metadata.len()))
}

fn cache_lookup(key: &str) -> Option<Arc<Vec<u8>>> {
    let mut cache = PDF_CACHE.lock().ok()?;
    let index = cache.iter().position(|entry| entry.key == key)?;
    let entry = cache.remove(index)?;
    let bytes = Arc::clone(&entry.bytes);
    cache.push_front(entry);
    Some(bytes)
}

/// PDF bytes for `path`, produced by `produce` on a cache miss.
pub(crate) fn cached_pdf(
    path: &Path,
    produce: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Arc<Vec<u8>>, String> {
    let key = cache_key(path).ok_or_else(|| "file is not readable".to_owned())?;
    if let Some(bytes) = cache_lookup(&key) {
        return Ok(bytes);
    }
    let _producing = PDF_PRODUCE.lock().map_err(|_| "preview cache is unavailable".to_owned())?;
    // Another request may have produced it while this one waited.
    if let Some(bytes) = cache_lookup(&key) {
        return Ok(bytes);
    }
    let bytes = Arc::new(produce()?);
    // A document larger than the whole budget is served but not kept.
    if bytes.len() > PDF_CACHE_BYTES {
        return Ok(bytes);
    }
    if let Ok(mut cache) = PDF_CACHE.lock() {
        cache.push_front(CachedPdf { key, bytes: Arc::clone(&bytes) });
        let mut total: usize = cache.iter().map(|entry| entry.bytes.len()).sum();
        while cache.len() > PDF_CACHE_DOCS || total > PDF_CACHE_BYTES {
            let Some(evicted) = cache.pop_back() else { break };
            total = total.saturating_sub(evicted.bytes.len());
        }
    }
    Ok(bytes)
}

/// Renders running at once. Each holds a full RGBA canvas, so this bounds peak memory no matter
/// how many page requests arrive together.
const MAX_CONCURRENT_RENDERS: usize = 2;
static ACTIVE_RENDERS: LazyLock<(Mutex<usize>, Condvar)> = LazyLock::new(|| (Mutex::new(0), Condvar::new()));

/// Holds one render slot until dropped.
struct RenderSlot;

impl RenderSlot {
    fn acquire() -> Option<Self> {
        let (count, ready) = &*ACTIVE_RENDERS;
        let mut active = count.lock().ok()?;
        while *active >= MAX_CONCURRENT_RENDERS {
            active = ready.wait(active).ok()?;
        }
        *active += 1;
        Some(Self)
    }
}

impl Drop for RenderSlot {
    fn drop(&mut self) {
        let (count, ready) = &*ACTIVE_RENDERS;
        if let Ok(mut active) = count.lock() {
            *active = active.saturating_sub(1);
        }
        ready.notify_one();
    }
}

/// Number of pages in a PDF.
pub(crate) fn page_count(bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
    let pdf = Pdf::new(Arc::clone(bytes)).map_err(|error| format!("PDF could not be read: {error:?}"))?;
    Ok(pdf.pages().len())
}

/// Page `page` (1-based) rendered as PNG.
pub(crate) fn render_page_png(bytes: &Arc<Vec<u8>>, page: usize) -> Result<Option<Vec<u8>>, String> {
    let pdf = Pdf::new(Arc::clone(bytes)).map_err(|error| format!("PDF could not be read: {error:?}"))?;
    let Some(index) = page.checked_sub(1) else {
        return Ok(None);
    };
    let Some(page) = pdf.pages().get(index) else {
        return Ok(None);
    };
    let (width, height) = page.render_dimensions();
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return Err("page has no size".to_owned());
    }
    let mut scale = (PAGE_DPI / 72.0).min(MAX_PAGE_WIDTH_PX / width);
    // Long pages (scrolls, banners) are scaled down so the canvas stays within the pixel budget.
    let pixels = width * scale * height * scale;
    if pixels > MAX_PAGE_PIXELS {
        scale *= (MAX_PAGE_PIXELS / pixels).sqrt();
    }
    // vello_cpu canvases are u16-sized on each edge; a very thin, very long page could still
    // overflow one edge after the pixel budget, so refuse it instead of panicking.
    let max_edge = f32::from(u16::MAX);
    if width * scale < 1.0 || height * scale < 1.0 || width * scale > max_edge || height * scale > max_edge {
        return Err("page is too large to render".to_owned());
    }
    let Some(_slot) = RenderSlot::acquire() else {
        return Err("renderer is unavailable".to_owned());
    };
    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    pixmap.into_png().map(Some).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_original, readable, snippet_payload, strip_cache_header};

    #[test]
    fn unindexed_legacy_doc_snippet_is_extracted_from_the_original() {
        let dir = std::env::temp_dir().join(format!("jikji-snippet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("saved-by-word.doc");
        std::fs::write(&doc, br"{\rtf1\ansi\ansicpg949{\fonttbl{\f0 Gulim;}} \'c3\'aa\'ba\'bf \'ba\'bb\'b9\'ae refund\par}").unwrap();
        let payload = snippet_payload(&dir, "saved-by-word.doc", Some(&doc), "");
        assert_eq!(payload["source"], "parsed_now", "{payload}");
        let text = payload["text"].as_str().unwrap();
        assert!(text.contains("챗봇 본문 refund"), "{text}");
        assert!(!text.contains("Gulim"), "{text}");
        // PDF extraction can panic on hostile input; it must never run on the request path.
        let pdf = dir.join("x.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\n%garbage").unwrap();
        assert!(parse_original(&pdf).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cp949_and_latin1_text_files_show_as_snippets() {
        let dir = std::env::temp_dir().join(format!("jikji-snippet-enc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let korean = dir.join("design.txt");
        let (bytes, _, _) = encoding_rs::EUC_KR.encode("속성명\t데이터 타입\r\nCLCT_DB_NM\tVARCHAR\t원시데이터명\r\n");
        std::fs::write(&korean, &bytes).unwrap();
        let payload = snippet_payload(&dir, "design.txt", Some(&korean), "");
        assert_eq!(payload["source"], "file_text", "{payload}");
        assert!(payload["text"].as_str().unwrap().contains("원시데이터명"));
        let western = dir.join("tags.txt");
        std::fs::write(&western, b"3IV1 \tcaf\xe9 codec used by 3ivx\r\n").unwrap();
        let payload = snippet_payload(&dir, "tags.txt", Some(&western), "");
        assert!(payload["text"].as_str().unwrap().contains("café codec"), "{payload}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legacy_ole_garbage_is_not_readable_but_korean_text_is() {
        assert!(!readable(&"þÿÿÿ ÿÿÿÿÿÿÿÿÿÿÿÿ\u{1}\u{2}".repeat(40)));
        assert!(readable("용역 계약서 본문 refund policy\n두 번째 줄"));
        assert!(readable(""));
    }

    #[test]
    fn container_bytes_cached_by_old_releases_are_not_readable() {
        // Samples taken from the production index (old releases cached these as "text").
        assert!(!readable("2‘oWf [Content_Types].xml ”ËjÃ0 …þƒÑ¶ØJº( Š4NDõB"));
        assert!(!readable("Root Entry FileHeader DocInfo HwpSummaryInformation Root Ent"));
        assert!(!readable("扪扪村村 ሄᘀ膄ȀԻ蹥Ի蹥楆 색쒳났ﲬ ᇉ뜀ꗇギ색탆 ꌥ\u{2000}情岸ギ 薺\u{2000}㨀"));
        assert!(!readable("Ԁ吸촇준Ȁ Ȁ뀄섀Ȁ 尀瀀Ȁ ††††††††††††††††††††††††††††††††††"));
        // Real text with typographic punctuation, numbers and mixed scripts stays readable.
        assert!(readable("일 일 업 무 보 고 (2025.7.7.) 보고일자 : 2025. 7. 7. — “AI 사업” 관리…"));
        assert!(readable("Machine Learning for entrepreneurship 코드 SIT31009"));
    }

    #[test]
    fn cache_header_lines_are_not_shown_as_document_text() {
        let cached = "# Source: docs/a.docx\n# File ID: sha256:ab\n# Parsed by: Jikji\n\n본문 첫 줄\n# 제목처럼 보이는 줄";
        assert_eq!(strip_cache_header(cached), "본문 첫 줄\n# 제목처럼 보이는 줄");
        assert_eq!(strip_cache_header("# Source: x\n# File ID: y\n# Parsed by: Jikji\n\n"), "");
        assert_eq!(strip_cache_header("# 일반 마크다운 제목\n본문"), "# 일반 마크다운 제목\n본문");
    }
}
