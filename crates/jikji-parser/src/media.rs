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
        let text = metadata_text(&metadata);
        let mut document = ParsedDocument::new(text, ParseStatus::MetadataOnly, self.name());
        document.metadata = metadata;
        document
    }
}

fn metadata_text(metadata: &std::collections::BTreeMap<String, String>) -> String {
    let mut lines = Vec::new();
    if let (Some(width), Some(height)) = (metadata.get("width"), metadata.get("height")) {
        lines.push(format!("Dimensions: {width}x{height} pixels"));
    }
    if let Some(duration) = metadata.get("duration_ms") {
        lines.push(format!("Duration: {duration} ms"));
    }
    for (key, value) in metadata {
        if !matches!(key.as_str(), "width" | "height" | "duration_ms") {
            lines.push(format!("{}: {value}", key.replace('_', " ")));
        }
    }
    lines.join("\n")
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
