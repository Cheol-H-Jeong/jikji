use encoding_rs::{EUC_KR, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use regex::Regex;

pub fn decode_text(bytes: &[u8], max_chars: usize) -> String {
    let limit = max_chars.saturating_mul(8).min(bytes.len());
    let raw = &bytes[..limit];
    if raw.starts_with(&[0xff, 0xfe]) {
        return decode_with(UTF_16LE, &raw[2..], max_chars);
    }
    if raw.starts_with(&[0xfe, 0xff]) {
        return decode_with(UTF_16BE, &raw[2..], max_chars);
    }
    for encoding in [UTF_8, EUC_KR, WINDOWS_1252] {
        let (text, _, had_errors) = encoding.decode(raw);
        if !had_errors {
            return cap_chars(text.as_ref(), max_chars);
        }
    }
    let (text, _, _) = UTF_8.decode(raw);
    cap_chars(text.as_ref(), max_chars)
}

pub fn cap_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub fn normalize_lines(parts: &[String], max_chars: usize) -> String {
    let text = parts
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    cap_chars(text.as_str(), max_chars)
}

pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out.trim().to_owned()
}

pub fn strip_xml_tags(xml: &str, max_chars: usize) -> String {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity = String::new();
    for ch in xml.chars() {
        match ch {
            '<' => {
                if !text.trim().is_empty() {
                    parts.push(collapse_whitespace(text.as_str()));
                }
                text.clear();
                in_tag = true;
                in_entity = false;
                entity.clear();
            }
            '>' => in_tag = false,
            '&' if !in_tag => {
                in_entity = true;
                entity.clear();
            }
            ';' if in_entity && !in_tag => {
                text.push_str(decode_entity(entity.as_str()));
                in_entity = false;
                entity.clear();
            }
            _ if in_entity && !in_tag => entity.push(ch),
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    if !text.trim().is_empty() {
        parts.push(collapse_whitespace(text.as_str()));
    }
    normalize_lines(&parts, max_chars)
}

pub fn printable_runs(bytes: &[u8], max_chars: usize) -> String {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return cap_chars(
            decode_text(bytes, max_chars.saturating_mul(4)).as_str(),
            max_chars,
        );
    }
    let candidates = [
        decode_text(bytes, max_chars.saturating_mul(4)),
        decode_utf16_without_bom(bytes, true, max_chars.saturating_mul(4)),
        decode_utf16_without_bom(bytes, false, max_chars.saturating_mul(4)),
    ];
    let pattern = Regex::new(r"[\p{L}\p{N}\p{P}\p{Zs}\t\r\n]{4,}")
        .map_or_else(|_| String::new(), |regex| best_runs(&candidates, &regex));
    cap_chars(pattern.as_str(), max_chars)
}

fn decode_with(encoding: &'static encoding_rs::Encoding, bytes: &[u8], max_chars: usize) -> String {
    let (text, _, _) = encoding.decode(bytes);
    cap_chars(text.as_ref(), max_chars)
}

fn decode_utf16_without_bom(bytes: &[u8], little_endian: bool, max_chars: usize) -> String {
    let words = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    let text = String::from_utf16_lossy(words.as_slice());
    cap_chars(text.as_str(), max_chars)
}

fn best_runs(candidates: &[String], regex: &Regex) -> String {
    let mut best = String::new();
    for candidate in candidates {
        let text = regex
            .find_iter(candidate.as_str())
            .map(|found| found.as_str().trim())
            .filter(|part| part.chars().count() >= 4)
            .collect::<Vec<_>>()
            .join("\n");
        if text.len() > best.len() {
            best = text;
        }
    }
    best
}

fn decode_entity(entity: &str) -> &'static str {
    match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        _ => " ",
    }
}
