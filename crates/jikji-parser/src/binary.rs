use regex::Regex;

use crate::utils::{cap_chars, printable_runs};
use crate::{DocumentParser, ParseStatus, ParsedDocument, ParserInput};

#[derive(Debug, Default)]
pub struct PdfParser;

impl DocumentParser for PdfParser {
    fn name(&self) -> &'static str {
        "pdf-best-effort"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        extension == "pdf"
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        let text = pdf_text(input.bytes, input.max_chars);
        ParsedDocument::new(text, ParseStatus::Success, self.name())
    }
}

#[derive(Debug, Default)]
pub struct LegacyOfficeParser;

impl DocumentParser for LegacyOfficeParser {
    fn name(&self) -> &'static str {
        "legacy-office-best-effort"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        matches!(extension, "doc" | "ppt" | "pps" | "xls")
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        ParsedDocument::new(
            printable_runs(input.bytes, input.max_chars),
            ParseStatus::Success,
            self.name(),
        )
    }
}

#[derive(Debug, Default)]
pub struct BinaryHwpParser;

impl DocumentParser for BinaryHwpParser {
    fn name(&self) -> &'static str {
        "hwp-best-effort"
    }

    fn supports_extension(&self, extension: &str) -> bool {
        extension == "hwp"
    }

    fn parse(&self, input: ParserInput<'_>) -> ParsedDocument {
        ParsedDocument::new(
            printable_runs(input.bytes, input.max_chars),
            ParseStatus::Success,
            self.name(),
        )
    }
}

fn pdf_text(bytes: &[u8], max_chars: usize) -> String {
    let raw = String::from_utf8_lossy(bytes);
    if let Ok(regex) = Regex::new(r"\(([^)]{2,})\)") {
        let captures = regex
            .captures_iter(raw.as_ref())
            .filter_map(|capture| capture.get(1))
            .map(|matched| matched.as_str())
            .collect::<Vec<_>>();
        if !captures.is_empty() {
            return cap_chars(captures.join("\n").as_str(), max_chars);
        }
    }
    let scraped = printable_runs(bytes, max_chars.saturating_mul(2));
    let without_pdf_noise = Regex::new(r"(?m)^(%PDF|%%EOF|xref|trailer|startxref).*$")
        .map_or(scraped.clone(), |regex| {
            regex.replace_all(scraped.as_str(), "").into_owned()
        });
    let literal = Regex::new(r"\(([^)]{2,})\)").map_or_else(
        |_| without_pdf_noise.clone(),
        |regex| {
            let captures = regex
                .captures_iter(without_pdf_noise.as_str())
                .filter_map(|capture| capture.get(1))
                .map(|matched| matched.as_str())
                .collect::<Vec<_>>();
            if captures.is_empty() {
                without_pdf_noise.clone()
            } else {
                captures.join("\n")
            }
        },
    );
    cap_chars(literal.trim(), max_chars)
}
