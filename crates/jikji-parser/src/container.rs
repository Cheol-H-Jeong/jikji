use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::structured::html_or_xml_to_text;
use crate::utils::{cap_chars, strip_xml_tags};
use crate::{DocumentParser, ParseStatus, ParsedDocument, ParserInput};

#[derive(Debug, Default)]
pub struct EpubParser;

impl DocumentParser for EpubParser {
    fn name(&self) -> &'static str {
        "epub"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        extension == "epub"
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let Some(text) = zip_text(
            input.bytes,
            &|name| matches!(suffix(name).as_str(), "xhtml" | "html" | "htm" | "xml"),
            input.max_chars,
        ) else {
            return ParsedDocument::failed(self.name());
        };
        ParsedDocument::new(text, ParseStatus::Success, self.name())
    }
}

#[derive(Debug, Default)]
pub struct OfficeOpenXmlParser;

impl DocumentParser for OfficeOpenXmlParser {
    fn name(&self) -> &'static str {
        "office-openxml"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        matches!(extension, "docx" | "pptx" | "ppsx" | "xlsx")
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let matcher = |name: &str| {
            let lower = name.to_ascii_lowercase();
            (lower.starts_with("word/") && lower.ends_with(".xml"))
                || (lower.starts_with("ppt/slides/") && lower.ends_with(".xml"))
                || lower.ends_with("sharedstrings.xml")
                || (lower.starts_with("xl/worksheets/") && lower.ends_with(".xml"))
        };
        let Some(text) = zip_text(input.bytes, &matcher, input.max_chars) else {
            return ParsedDocument::failed(self.name());
        };
        ParsedDocument::new(text, ParseStatus::Success, self.name())
    }
}

#[derive(Debug, Default)]
pub struct OpenDocumentParser;

impl DocumentParser for OpenDocumentParser {
    fn name(&self) -> &'static str {
        "opendocument"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        matches!(extension, "odt" | "ods" | "odp")
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let Some(text) = zip_text(input.bytes, &|name| name == "content.xml", input.max_chars)
        else {
            return ParsedDocument::failed(self.name());
        };
        ParsedDocument::new(text, ParseStatus::Success, self.name())
    }
}

#[derive(Debug, Default)]
pub struct HwpxParser;

impl DocumentParser for HwpxParser {
    fn name(&self) -> &'static str {
        "hwpx"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        extension == "hwpx"
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let matcher = |name: &str| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("contents/section") && lower.ends_with(".xml")
                || lower.contains("section") && lower.ends_with(".xml")
        };
        let Some(text) = zip_text(input.bytes, &matcher, input.max_chars) else {
            return ParsedDocument::failed(self.name());
        };
        ParsedDocument::new(text, ParseStatus::Success, self.name())
    }
}

fn zip_text(bytes: &[u8], predicate: &dyn Fn(&str) -> bool, max_chars: usize) -> Option<String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut parts = Vec::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).ok()?;
        let name = file.name().to_owned();
        if !predicate(name.as_str()) {
            continue;
        }
        let mut xml = String::new();
        if file.read_to_string(&mut xml).is_err() {
            let mut bytes = Vec::new();
            if file.read_to_end(&mut bytes).is_err() {
                continue;
            }
            xml = html_or_xml_to_text(bytes.as_slice(), max_chars);
        } else {
            xml = strip_xml_tags(xml.as_str(), max_chars);
        }
        if !xml.trim().is_empty() {
            total = total.saturating_add(xml.len());
            parts.push(xml);
            if total >= max_chars {
                break;
            }
        }
    }
    Some(cap_chars(parts.join("\n").as_str(), max_chars))
}

fn suffix(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, suffix)| suffix.to_ascii_lowercase())
        .unwrap_or_default()
}
