#![forbid(unsafe_code)]

#[cfg(unix)]
use nix::sys::signal::{Signal, killpg};
#[cfg(unix)]
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_NATIVE_MEDIA_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENGINE_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_ENGINE_TIMEOUT: Duration = Duration::from_secs(30);

pub const OCR_ENGINE_ENV: &str = "JIKJI_OCR_ENGINE";
pub const OCR_ENGINE_ARGS_ENV: &str = "JIKJI_OCR_ENGINE_ARGS_JSON";
pub const OCR_ENGINE_TIMEOUT_ENV: &str = "JIKJI_OCR_ENGINE_TIMEOUT_SECONDS";
pub const ASR_ENGINE_ENV: &str = "JIKJI_ASR_ENGINE";
pub const ASR_ENGINE_ARGS_ENV: &str = "JIKJI_ASR_ENGINE_ARGS_JSON";
pub const ASR_ENGINE_TIMEOUT_ENV: &str = "JIKJI_ASR_ENGINE_TIMEOUT_SECONDS";
pub const ENGINE_INPUT_ENV: &str = "JIKJI_MEDIA_ENGINE_INPUT";
pub const ENGINE_KIND_ENV: &str = "JIKJI_MEDIA_ENGINE_KIND";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeAvailability {
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaBridgeStatus {
    MetadataOnly,
    Success,
    Unavailable,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaBridgeRequest {
    pub path: PathBuf,
    pub kind: MediaKind,
}

impl MediaBridgeRequest {
    pub fn new(path: PathBuf, kind: MediaKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBridgeOutcome {
    pub status: MediaBridgeStatus,
    pub text: String,
    pub metadata: BTreeMap<String, String>,
    pub error: String,
}

impl MediaBridgeOutcome {
    fn status(
        status: MediaBridgeStatus,
        metadata: BTreeMap<String, String>,
        error: String,
    ) -> Self {
        Self {
            status,
            text: String::new(),
            metadata,
            error,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEngineConfig {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub timeout: Duration,
    configuration_error: Option<String>,
}

impl NativeEngineConfig {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            args: Vec::new(),
            timeout: DEFAULT_ENGINE_TIMEOUT,
            configuration_error: None,
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn is_available(&self) -> bool {
        resolve_executable(&self.executable).is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgeConfig {
    pub ocr: Option<NativeEngineConfig>,
    pub asr: Option<NativeEngineConfig>,
}

impl BridgeConfig {
    pub fn from_env() -> Self {
        Self {
            ocr: engine_from_env(OCR_ENGINE_ENV, OCR_ENGINE_ARGS_ENV, OCR_ENGINE_TIMEOUT_ENV),
            asr: engine_from_env(ASR_ENGINE_ENV, ASR_ENGINE_ARGS_ENV, ASR_ENGINE_TIMEOUT_ENV),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BridgeRuntime {
    config: BridgeConfig,
}

impl BridgeRuntime {
    pub fn new() -> Self {
        Self::with_config(BridgeConfig::from_env())
    }

    pub fn with_config(config: BridgeConfig) -> Self {
        Self { config }
    }

    pub fn availability(&self) -> BridgeAvailability {
        BridgeAvailability::Native
    }

    pub fn config(&self) -> &BridgeConfig {
        &self.config
    }

    pub fn extract(&self, request: &MediaBridgeRequest) -> MediaBridgeOutcome {
        let file = match File::open(&request.path) {
            Ok(file) => file,
            Err(error) => {
                let status = if error.kind() == std::io::ErrorKind::NotFound {
                    MediaBridgeStatus::Unavailable
                } else {
                    MediaBridgeStatus::Failed
                };
                return MediaBridgeOutcome::status(status, BTreeMap::new(), error.to_string());
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = file
            .take(MAX_NATIVE_MEDIA_BYTES + 1)
            .read_to_end(&mut bytes)
        {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::Failed,
                BTreeMap::new(),
                error.to_string(),
            );
        }
        if bytes.len() as u64 > MAX_NATIVE_MEDIA_BYTES {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::Failed,
                BTreeMap::new(),
                format!(
                    "media file exceeds native metadata limit of {MAX_NATIVE_MEDIA_BYTES} bytes"
                ),
            );
        }
        let mut metadata = extract_metadata(&request.path, &bytes, request.kind);
        let Some((engine_kind, engine)) = self.engine(request.kind) else {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::MetadataOnly,
                metadata,
                String::new(),
            );
        };
        metadata.insert("text_engine".to_owned(), engine_kind.to_owned());
        run_engine(engine, request, metadata)
    }

    fn engine(&self, kind: MediaKind) -> Option<(&'static str, &NativeEngineConfig)> {
        match kind {
            MediaKind::Image => self.config.ocr.as_ref().map(|engine| ("ocr", engine)),
            MediaKind::Audio | MediaKind::Video => {
                self.config.asr.as_ref().map(|engine| ("asr", engine))
            }
        }
    }
}

fn engine_from_env(
    executable_name: &str,
    args_name: &str,
    timeout_name: &str,
) -> Option<NativeEngineConfig> {
    let executable = env::var_os(executable_name).filter(|value| !value.is_empty())?;
    let mut engine = NativeEngineConfig::new(PathBuf::from(executable));
    if let Ok(value) = env::var(args_name)
        && !value.trim().is_empty()
    {
        match serde_json::from_str::<Vec<String>>(&value) {
            Ok(args) => engine.args = args,
            Err(error) => {
                engine.configuration_error = Some(format!("invalid {args_name}: {error}"));
            }
        }
    }
    if let Ok(value) = env::var(timeout_name)
        && !value.trim().is_empty()
    {
        match value.parse::<f64>() {
            Ok(seconds) if seconds.is_finite() && seconds > 0.0 && seconds <= 86_400.0 => {
                engine.timeout = Duration::from_secs_f64(seconds);
            }
            _ => {
                engine.configuration_error = Some(format!(
                    "invalid {timeout_name}: expected seconds in (0, 86400]"
                ));
            }
        }
    }
    Some(engine)
}

fn resolve_executable(executable: &Path) -> Option<PathBuf> {
    if executable.components().count() > 1 {
        return executable.is_file().then(|| executable.to_path_buf());
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(executable))
        .find(|candidate| candidate.is_file())
}

#[derive(Debug)]
struct EngineStream {
    bytes: Vec<u8>,
    overflow: bool,
}

fn run_engine(
    engine: &NativeEngineConfig,
    request: &MediaBridgeRequest,
    metadata: BTreeMap<String, String>,
) -> MediaBridgeOutcome {
    if let Some(error) = &engine.configuration_error {
        return MediaBridgeOutcome::status(MediaBridgeStatus::Failed, metadata, error.clone());
    }
    let mut command = Command::new(&engine.executable);
    command
        .args(&engine.args)
        .env(ENGINE_INPUT_ENV, &request.path)
        .env(ENGINE_KIND_ENV, media_kind_name(request.kind))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::Unavailable,
                metadata,
                error.to_string(),
            );
        }
    };
    let stdout = child.stdout.take().map(read_engine_stream);
    let stderr = child.stderr.take().map(read_engine_stream);
    match wait_for_engine(&mut child, engine.timeout) {
        Ok(Some(status)) => finish_engine(status, stdout, stderr, metadata),
        Ok(None) => {
            kill_engine(&mut child);
            let error = collect_stream(stderr).0;
            let _ = collect_stream(stdout);
            MediaBridgeOutcome::status(
                MediaBridgeStatus::Timeout,
                metadata,
                if error.is_empty() {
                    format!("native media engine timed out after {:?}", engine.timeout)
                } else {
                    error
                },
            )
        }
        Err(error) => {
            kill_engine(&mut child);
            let _ = collect_stream(stdout);
            let _ = collect_stream(stderr);
            MediaBridgeOutcome::status(MediaBridgeStatus::Failed, metadata, error.to_string())
        }
    }
}

fn wait_for_engine(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn read_engine_stream<R>(mut stream: R) -> thread::JoinHandle<std::io::Result<EngineStream>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        stream
            .by_ref()
            .take((MAX_ENGINE_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let overflow = bytes.len() > MAX_ENGINE_OUTPUT_BYTES;
        if overflow {
            std::io::copy(&mut stream, &mut std::io::sink())?;
            bytes.truncate(MAX_ENGINE_OUTPUT_BYTES);
        }
        Ok(EngineStream { bytes, overflow })
    })
}

fn collect_stream(
    stream: Option<thread::JoinHandle<std::io::Result<EngineStream>>>,
) -> (String, bool) {
    let Some(stream) = stream else {
        return (String::new(), false);
    };
    let Ok(Ok(stream)) = stream.join() else {
        return (String::new(), false);
    };
    (
        String::from_utf8_lossy(&stream.bytes).trim().to_owned(),
        stream.overflow,
    )
}

fn kill_engine(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn finish_engine(
    status: ExitStatus,
    stdout: Option<thread::JoinHandle<std::io::Result<EngineStream>>>,
    stderr: Option<thread::JoinHandle<std::io::Result<EngineStream>>>,
    metadata: BTreeMap<String, String>,
) -> MediaBridgeOutcome {
    let (text, stdout_overflow) = collect_stream(stdout);
    let (error, stderr_overflow) = collect_stream(stderr);
    if stdout_overflow || stderr_overflow {
        return MediaBridgeOutcome::status(
            MediaBridgeStatus::Failed,
            metadata,
            format!("native media engine output exceeds {MAX_ENGINE_OUTPUT_BYTES} bytes"),
        );
    }
    if !status.success() {
        return MediaBridgeOutcome::status(
            MediaBridgeStatus::Failed,
            metadata,
            if error.is_empty() {
                format!("native media engine exited with {status}")
            } else {
                error
            },
        );
    }
    if text.is_empty() {
        return MediaBridgeOutcome::status(MediaBridgeStatus::MetadataOnly, metadata, error);
    }
    MediaBridgeOutcome {
        status: MediaBridgeStatus::Success,
        text,
        metadata,
        error,
    }
}

pub fn media_bridge_status() -> MediaBridgeOutcome {
    MediaBridgeOutcome::status(
        MediaBridgeStatus::MetadataOnly,
        BTreeMap::new(),
        String::new(),
    )
}

pub fn extract_metadata(path: &Path, bytes: &[u8], kind: MediaKind) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::from([
        ("engine".to_owned(), "rust-native".to_owned()),
        ("kind".to_owned(), media_kind_name(kind).to_owned()),
        ("bytes".to_owned(), bytes.len().to_string()),
    ]);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    metadata.insert("format".to_owned(), extension.clone());

    if kind == MediaKind::Image {
        if let Some((width, height)) = image_dimensions(extension.as_str(), bytes) {
            metadata.insert("width".to_owned(), width.to_string());
            metadata.insert("height".to_owned(), height.to_string());
        }
    } else if kind == MediaKind::Audio && extension == "wav" {
        add_wav_metadata(bytes, &mut metadata);
    }
    metadata
}

fn media_kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Audio => "audio",
        MediaKind::Video => "video",
    }
}

fn image_dimensions(extension: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    match extension {
        "png" => png_dimensions(bytes),
        "jpg" | "jpeg" => jpeg_dimensions(bytes),
        "gif" => gif_dimensions(bytes),
        "bmp" => bmp_dimensions(bytes),
        "webp" => webp_dimensions(bytes),
        _ => None,
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = bytes.get(..24)?;
    if header.get(..8)? != b"\x89PNG\r\n\x1a\n" || header.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be_u32(header, 16)?, be_u32(header, 20)?))
}

fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = bytes.get(..10)?;
    if header.get(..6)? != b"GIF87a" && header.get(..6)? != b"GIF89a" {
        return None;
    }
    Some((u32::from(le_u16(header, 6)?), u32::from(le_u16(header, 8)?)))
}

fn bmp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != b"BM" || le_u32(bytes, 14)? < 12 {
        return None;
    }
    let width = le_i32(bytes, 18)?.unsigned_abs();
    let height = le_i32(bytes, 22)?.unsigned_abs();
    (width > 0 && height > 0).then_some((width, height))
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    match bytes.get(12..16)? {
        b"VP8X" => {
            let width = le_u24(bytes, 24)?.checked_add(1)?;
            let height = le_u24(bytes, 27)?.checked_add(1)?;
            Some((width, height))
        }
        b"VP8L" if *bytes.get(20)? == 0x2f => {
            let bits = le_u32(bytes, 21)?;
            let width = (bits & 0x3fff) + 1;
            let height = ((bits >> 14) & 0x3fff) + 1;
            Some((width, height))
        }
        _ => None,
    }
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != b"\xff\xd8" {
        return None;
    }
    let mut offset = 2;
    while offset + 4 <= bytes.len() {
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if marker == 0xd8 || marker == 0xd9 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let segment_len = usize::from(be_u16(bytes, offset)?);
        if segment_len < 2 || offset.checked_add(segment_len)? > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u32::from(be_u16(bytes, offset + 3)?);
            let width = u32::from(be_u16(bytes, offset + 5)?);
            return (width > 0 && height > 0).then_some((width, height));
        }
        offset += segment_len;
    }
    None
}

fn add_wav_metadata(bytes: &[u8], metadata: &mut BTreeMap<String, String>) {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return;
    }
    let mut offset = 12;
    let mut byte_rate = None;
    let mut data_bytes = None;
    while offset + 8 <= bytes.len() {
        let chunk = &bytes[offset..offset + 4];
        let size = match le_u32(bytes, offset + 4) {
            Some(size) => size as usize,
            None => return,
        };
        let data_offset = offset + 8;
        if data_offset
            .checked_add(size)
            .is_none_or(|end| end > bytes.len())
        {
            return;
        }
        if chunk == b"fmt " && size >= 16 {
            if let (Some(channels), Some(sample_rate), Some(rate), Some(bits)) = (
                le_u16(bytes, data_offset + 2),
                le_u32(bytes, data_offset + 4),
                le_u32(bytes, data_offset + 8),
                le_u16(bytes, data_offset + 14),
            ) {
                metadata.insert("channels".to_owned(), channels.to_string());
                metadata.insert("sample_rate_hz".to_owned(), sample_rate.to_string());
                metadata.insert("bits_per_sample".to_owned(), bits.to_string());
                byte_rate = Some(rate);
            }
        } else if chunk == b"data" {
            data_bytes = Some(size as u64);
        }
        offset = data_offset + size + (size % 2);
    }
    if let (Some(rate), Some(data_len)) = (byte_rate, data_bytes)
        && rate > 0
    {
        metadata.insert(
            "duration_ms".to_owned(),
            data_len
                .saturating_mul(1000)
                .checked_div(u64::from(rate))
                .unwrap_or(0)
                .to_string(),
        );
    }
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn le_u24(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = bytes.get(offset..offset + 3)?;
    Some(u32::from(value[0]) | (u32::from(value[1]) << 8) | (u32::from(value[2]) << 16))
}

fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn le_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
