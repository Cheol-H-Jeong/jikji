#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_NATIVE_MEDIA_BYTES: u64 = 512 * 1024 * 1024;

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
    pub python_required_by_default: bool,
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
            python_required_by_default: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BridgeRuntime;

impl BridgeRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn availability(&self) -> BridgeAvailability {
        BridgeAvailability::Native
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
        let metadata = extract_metadata(&request.path, &bytes, request.kind);
        MediaBridgeOutcome::status(MediaBridgeStatus::MetadataOnly, metadata, String::new())
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
