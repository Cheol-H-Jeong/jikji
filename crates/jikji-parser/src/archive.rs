use std::io::{Cursor, Read};
use std::path::{Component, Path};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::utils::cap_chars;
use crate::{
    ArchiveLimits, DocumentParser, ParseStatus, ParsedDocument, ParserInput, ParserRegistry,
};

#[derive(Debug, Default)]
pub struct ArchiveParser;

impl DocumentParser for ArchiveParser {
    fn name(&self) -> &'static str {
        "archive-metadata"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        matches!(
            extension,
            "zip"
                | "jar"
                | "war"
                | "tar"
                | "tar.gz"
                | "tgz"
                | "tar.bz2"
                | "tbz"
                | "tar.xz"
                | "txz"
                | "7z"
                | "rar"
        )
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let archive_name = input
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("archive");
        let lower_name = archive_name.to_ascii_lowercase();
        let format = extension_name(&lower_name);
        let mut document = ParsedDocument::new(
            format!("Archive: {archive_name}\nFormat: {format}"),
            ParseStatus::MetadataOnly,
            self.name(),
        );
        document
            .metadata
            .insert("archive_format".to_owned(), format.to_owned());
        document
            .metadata
            .insert("detail_indexed".to_owned(), "false".to_owned());
        document
    }
}

pub(crate) fn parse_deep(
    registry: &ParserRegistry,
    path: &Path,
    bytes: &[u8],
    max_chars: usize,
    limits: ArchiveLimits,
) -> ParsedDocument {
    let lower_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !ArchiveParser.supports_extension(extension_name(&lower_name)) {
        return registry.parse_bytes(path.to_string_lossy().as_ref(), bytes, max_chars);
    }
    let result = if lower_name.ends_with(".zip")
        || lower_name.ends_with(".jar")
        || lower_name.ends_with(".war")
    {
        deep_zip(registry, bytes, max_chars, limits)
    } else if lower_name.ends_with(".7z") || lower_name.ends_with(".rar") {
        return unsupported_archive_listing(path, &lower_name);
    } else {
        deep_tar(registry, bytes, &lower_name, max_chars, limits)
    };
    match result {
        Some((text, entries, skipped)) => {
            let mut document = ParsedDocument::new(text, ParseStatus::Success, "archive-deep");
            document
                .metadata
                .insert("entries_indexed".to_owned(), entries.to_string());
            document
                .metadata
                .insert("entries_skipped".to_owned(), skipped.to_string());
            document
        }
        None => ParsedDocument::failed("archive-deep"),
    }
}

fn extension_name(lower_name: &str) -> &str {
    for compound in ["tar.gz", "tar.bz2", "tar.xz"] {
        if lower_name.ends_with(compound) {
            return compound;
        }
    }
    lower_name.rsplit_once('.').map_or("", |(_, ext)| ext)
}

fn deep_zip(
    registry: &ParserRegistry,
    bytes: &[u8],
    max_chars: usize,
    limits: ArchiveLimits,
) -> Option<(String, usize, usize)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut output = String::new();
    let mut total = 0u64;
    let mut indexed = 0usize;
    let mut skipped = 0usize;
    for index in 0..archive.len() {
        if indexed >= limits.max_entries {
            skipped += archive.len() - index;
            break;
        }
        let mut member = archive.by_index(index).ok()?;
        let name = member.name().to_owned();
        if member.is_dir() || !safe_member_path(&name) {
            skipped += 1;
            continue;
        }
        let size = member.size();
        if size > limits.max_entry_bytes || total.saturating_add(size) > limits.max_total_bytes {
            skipped += 1;
            continue;
        }
        let Some(contents) = read_bounded(&mut member, limits.max_entry_bytes) else {
            skipped += 1;
            continue;
        };
        total = total.saturating_add(contents.len() as u64);
        append_member(registry, &mut output, &name, &contents, max_chars);
        indexed += 1;
        if output.chars().count() >= max_chars {
            break;
        }
    }
    Some((cap_chars(&output, max_chars), indexed, skipped))
}

fn deep_tar(
    registry: &ParserRegistry,
    bytes: &[u8],
    lower_name: &str,
    max_chars: usize,
    limits: ArchiveLimits,
) -> Option<(String, usize, usize)> {
    if lower_name.ends_with(".tar.gz") || lower_name.ends_with(".tgz") {
        return deep_tar_reader(
            registry,
            GzDecoder::new(Cursor::new(bytes)),
            max_chars,
            limits,
        );
    }
    if lower_name.ends_with(".tar.bz2") || lower_name.ends_with(".tbz") {
        return deep_tar_reader(
            registry,
            BzDecoder::new(Cursor::new(bytes)),
            max_chars,
            limits,
        );
    }
    if lower_name.ends_with(".tar.xz") || lower_name.ends_with(".txz") {
        return deep_tar_reader(
            registry,
            XzDecoder::new(Cursor::new(bytes)),
            max_chars,
            limits,
        );
    }
    deep_tar_reader(registry, Cursor::new(bytes), max_chars, limits)
}

fn deep_tar_reader<R: Read>(
    registry: &ParserRegistry,
    reader: R,
    max_chars: usize,
    limits: ArchiveLimits,
) -> Option<(String, usize, usize)> {
    let mut archive = Archive::new(reader);
    let mut output = String::new();
    let mut total = 0u64;
    let mut indexed = 0usize;
    let mut skipped = 0usize;
    for entry in archive.entries().ok()? {
        if indexed >= limits.max_entries {
            skipped += 1;
            continue;
        }
        let mut entry = entry.ok()?;
        let name = entry.path().ok()?.to_string_lossy().into_owned();
        if !entry.header().entry_type().is_file() || !safe_member_path(&name) {
            skipped += 1;
            continue;
        }
        let size = entry.size();
        if size > limits.max_entry_bytes || total.saturating_add(size) > limits.max_total_bytes {
            skipped += 1;
            continue;
        }
        let Some(contents) = read_bounded(&mut entry, limits.max_entry_bytes) else {
            skipped += 1;
            continue;
        };
        total = total.saturating_add(contents.len() as u64);
        append_member(registry, &mut output, &name, &contents, max_chars);
        indexed += 1;
        if output.chars().count() >= max_chars {
            break;
        }
    }
    Some((cap_chars(&output, max_chars), indexed, skipped))
}

fn read_bounded(reader: &mut impl Read, max_bytes: u64) -> Option<Vec<u8>> {
    let mut contents = Vec::new();
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut contents)
        .ok()?;
    (contents.len() as u64 <= max_bytes).then_some(contents)
}

fn safe_member_path(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn append_member(
    registry: &ParserRegistry,
    output: &mut String,
    name: &str,
    contents: &[u8],
    max_chars: usize,
) {
    output.push_str("[member: ");
    output.push_str(name);
    output.push_str("]\n");
    let remaining = max_chars.saturating_sub(output.chars().count());
    if remaining == 0 {
        return;
    }
    let parsed = registry.parse_bytes(name, contents, remaining);
    if matches!(
        parsed.status,
        ParseStatus::Success | ParseStatus::MetadataOnly
    ) {
        output.push_str(&parsed.text);
        output.push('\n');
    }
}

fn unsupported_archive_listing(path: &std::path::Path, lower_name: &str) -> ParsedDocument {
    let archive_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive");
    let extension = lower_name
        .rsplit_once('.')
        .map_or(lower_name, |(_, extension)| extension);
    let mut document = ParsedDocument::new(
        format!("[archive: {archive_name}] member listing unavailable for .{extension}"),
        ParseStatus::MetadataOnly,
        "archive-listing",
    );
    document
        .metadata
        .insert("listing".to_owned(), "unsupported".to_owned());
    document
        .metadata
        .insert("archive_format".to_owned(), extension.to_owned());
    document
}
