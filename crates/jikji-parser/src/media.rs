use std::collections::BTreeMap;

use crate::{DocumentParser, ParseStatus, ParsedDocument, ParserInput};

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp", "gif"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "m4a", "flac", "ogg", "aac", "opus", "wma"];
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg",
];

#[derive(Debug, Default)]
pub struct MediaMetadataParser;

impl DocumentParser for MediaMetadataParser {
    fn name(&self) -> &'static str {
        "media-metadata"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        IMAGE_EXTENSIONS.contains(&extension)
            || AUDIO_EXTENSIONS.contains(&extension)
            || VIDEO_EXTENSIONS.contains(&extension)
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let extension = input
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let kind = media_kind(extension.as_str());
        let mut metadata = BTreeMap::new();
        metadata.insert("kind".to_owned(), kind.to_owned());
        metadata.insert("bytes".to_owned(), input.bytes.len().to_string());
        let text = media_text(input, extension.as_str(), kind);
        let status = if text.is_empty() {
            ParseStatus::MetadataOnly
        } else {
            ParseStatus::Success
        };
        let mut document = ParsedDocument::new(text, status, self.name());
        document.metadata = metadata;
        document
    }
}

fn media_kind(extension: &str) -> &'static str {
    if IMAGE_EXTENSIONS.contains(&extension) {
        "image"
    } else if AUDIO_EXTENSIONS.contains(&extension) {
        "audio"
    } else {
        "video"
    }
}

fn media_text(input: ParserInput<'_>, extension: &str, kind: &str) -> String {
    if kind == "image" && extension == "png" {
        return png_text(input);
    }
    String::new()
}

fn png_text(input: ParserInput<'_>) -> String {
    let Some(dimensions) = png_dimensions(input.bytes) else {
        return String::new();
    };
    let name = input
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    format!(
        "# Image: {name}\nFormat: PNG\nDimensions: {}x{} pixels",
        dimensions.width, dimensions.height
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageDimensions {
    width: u32,
    height: u32,
}

fn png_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    let header = bytes.get(..24)?;
    if header.get(..8)? != b"\x89PNG\r\n\x1a\n" || header.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(header.get(20..24)?.try_into().ok()?);
    Some(ImageDimensions { width, height })
}
