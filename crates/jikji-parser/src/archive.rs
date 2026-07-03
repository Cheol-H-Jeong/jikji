use std::io::{Cursor, Read};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::utils::cap_chars;
use crate::{DocumentParser, ParseStatus, ParsedDocument, ParserInput};

#[derive(Debug, Default)]
pub struct ArchiveParser;

impl DocumentParser for ArchiveParser {
    fn name(&self) -> &'static str {
        "archive-listing"
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
        let extension = input
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension.ends_with(".7z") || extension.ends_with(".rar") {
            return unsupported_archive_listing(input.path, extension.as_str());
        }
        let names = if extension.ends_with(".zip")
            || extension.ends_with(".jar")
            || extension.ends_with(".war")
        {
            zip_names(input.bytes)
        } else {
            tar_names(input.bytes, extension.as_str())
        };
        match names {
            Some(names) => ParsedDocument::new(
                format_listing(input.path, names.as_slice(), input.max_chars),
                ParseStatus::ArchiveListing,
                self.name(),
            ),
            None => ParsedDocument::failed(self.name()),
        }
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

fn zip_names(bytes: &[u8]) -> Option<Vec<String>> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        if let Ok(file) = archive.by_index(index) {
            names.push(file.name().to_owned());
        }
    }
    Some(names)
}

fn tar_names(bytes: &[u8], lower_name: &str) -> Option<Vec<String>> {
    if lower_name.ends_with(".tar.gz") || lower_name.ends_with(".tgz") {
        return tar_names_from_reader(GzDecoder::new(Cursor::new(bytes)));
    }
    if lower_name.ends_with(".tar.bz2") || lower_name.ends_with(".tbz") {
        return tar_names_from_reader(BzDecoder::new(Cursor::new(bytes)));
    }
    if lower_name.ends_with(".tar.xz") || lower_name.ends_with(".txz") {
        return tar_names_from_reader(XzDecoder::new(Cursor::new(bytes)));
    }
    tar_names_from_reader(Cursor::new(bytes))
}

fn tar_names_from_reader<R: Read>(reader: R) -> Option<Vec<String>> {
    let mut archive = Archive::new(reader);
    let entries = archive.entries().ok()?;
    let mut names = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return None;
        };
        let Ok(path) = entry.path() else {
            continue;
        };
        names.push(path.to_string_lossy().into_owned());
    }
    Some(names)
}

fn format_listing(path: &std::path::Path, names: &[String], max_chars: usize) -> String {
    let archive_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive");
    let files = names
        .iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty() && !name.ends_with('/'))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return format!("[archive: {archive_name}] (empty)");
    }
    let mut listing = format!("[archive: {archive_name} - {} files]\n", files.len());
    for name in files {
        let next = if listing.ends_with('\n') {
            name.to_owned()
        } else {
            format!(", {name}")
        };
        if listing.len().saturating_add(next.len()) > max_chars {
            listing.push_str(", ...");
            break;
        }
        listing.push_str(next.as_str());
    }
    cap_chars(listing.as_str(), max_chars)
}
