use jikji_media_bridge::{MediaKind, extract_metadata};

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
        let metadata = extract_metadata(input.path, input.bytes, kind);
        let text = media_text(input, extension.as_str(), kind, &metadata);
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

fn media_kind(extension: &str) -> MediaKind {
    if IMAGE_EXTENSIONS.contains(&extension) {
        MediaKind::Image
    } else if AUDIO_EXTENSIONS.contains(&extension) {
        MediaKind::Audio
    } else {
        MediaKind::Video
    }
}

fn media_text(
    input: ParserInput<'_>,
    extension: &str,
    kind: MediaKind,
    metadata: &std::collections::BTreeMap<String, String>,
) -> String {
    if kind != MediaKind::Image {
        return String::new();
    }
    let (Some(width), Some(height)) = (metadata.get("width"), metadata.get("height")) else {
        return String::new();
    };
    let name = input
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    format!(
        "# Image: {name}\nFormat: {}\nDimensions: {width}x{height} pixels",
        extension.to_ascii_uppercase()
    )
}
