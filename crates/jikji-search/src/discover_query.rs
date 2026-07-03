use std::collections::BTreeSet;
use std::path::Path;

use sha2::{Digest, Sha256};

pub(crate) fn strip_shell_noise(query: &str) -> String {
    let noise = shell_noise();
    query
        .replace(['$', '`'], " ")
        .split_whitespace()
        .filter_map(|raw| {
            let token = raw.trim_matches(|ch: char| ".,:;!?()[]{}\"'".contains(ch));
            let folded = token.trim_start_matches('-').to_lowercase();
            if token.is_empty()
                || noise.contains(folded.as_str())
                || (token.starts_with('-') && folded.len() <= 2)
                || token.chars().all(|ch| matches!(ch, '/' | '.'))
                || token.chars().any(|ch| "$`;&|<>\\\"".contains(ch))
            {
                None
            } else {
                Some(token.to_owned())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn query_variants(query: &str) -> Vec<String> {
    let mut out = vec![query.to_owned()];
    let folded = query.to_lowercase();
    if folded.contains("nda") || folded.contains("confidential") {
        out.push("NDA confidential information copying".to_owned());
    }
    let anchors = anchor_tokens(query).join(" ");
    if !anchors.is_empty() {
        out.push(anchors);
    }
    let mut seen = BTreeSet::new();
    out.into_iter()
        .filter(|variant| seen.insert(variant.to_lowercase()))
        .take(6)
        .collect()
}

pub(crate) fn classify_query(query: &str) -> String {
    let folded = query.to_lowercase();
    if [
        "habit",
        "usual",
        "summarize",
        "summary",
        "records",
        "versions",
    ]
    .iter()
    .any(|hint| folded.contains(hint))
    {
        "evidence_set".to_owned()
    } else if [
        "which",
        "what file",
        "find the",
        "locate",
        "contract",
        "agreement",
        "nda",
        "pdf",
        "document",
        "file",
    ]
    .iter()
    .any(|hint| folded.contains(hint))
    {
        "single_file".to_owned()
    } else {
        "adaptive".to_owned()
    }
}

pub(crate) fn retry_proof_for(root: &Path, query: &str, top_k: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.display().to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(query.as_bytes());
    hasher.update(b"\0");
    hasher.update(top_k.to_string().as_bytes());
    hasher.update(b"\0jikji-retry-v1");
    format!("{:x}", hasher.finalize())
        .chars()
        .take(24)
        .collect()
}

pub(crate) fn anchor_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .flat_map(|token| {
            let mut out = vec![token.to_lowercase()];
            if let Some(year) = token
                .strip_prefix("FY")
                .or_else(|| token.strip_prefix("fy"))
            {
                if year.len() == 2 && year.chars().all(|ch| ch.is_ascii_digit()) {
                    out.push(format!("20{year}"));
                    out.push(year.to_owned());
                }
            }
            out
        })
        .filter(|token| !generic_anchor(token))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn generic_anchor(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "CEO"
            | "CFO"
            | "COO"
            | "CTO"
            | "DOC"
            | "DOCX"
            | "INC"
            | "LLC"
            | "NDA"
            | "PDF"
            | "PPT"
            | "PPTX"
            | "TXT"
            | "XLS"
            | "XLSX"
    )
}

fn shell_noise() -> BTreeSet<&'static str> {
    [
        "bash", "cat", "chmod", "curl", "echo", "find", "grep", "ls", "rm", "rf", "rmdir", "sed",
        "sh", "sudo", "wget",
    ]
    .into_iter()
    .collect()
}
