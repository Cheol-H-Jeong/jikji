use serde_json::{Value, json};

const MAP_CHUNK_CHARS: usize = 6_000;
const MAX_MAP_CHUNKS: usize = 24;

pub(crate) fn chunk_rows(row: &Value, digest: &str, body_text: &str) -> Vec<Value> {
    let chars = body_text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }
    chars
        .chunks(MAP_CHUNK_CHARS)
        .take(MAX_MAP_CHUNKS)
        .enumerate()
        .map(|(idx, chunk)| {
            let preview = chunk.iter().take(240).collect::<String>();
            let terms = terms_for(&preview, 32);
            let content_terms = terms.iter().take(24).cloned().collect::<Vec<_>>();
            let rare_terms = terms.iter().take(16).cloned().collect::<Vec<_>>();
            json!({
                "schema_version": 1,
                "path": row["path"],
                "chunk_id": format!("{digest}:{:04}", idx + 1),
                "text_cache_path": row["text_cache_path"],
                "char_start": idx * MAP_CHUNK_CHARS,
                "char_end": idx * MAP_CHUNK_CHARS + chunk.len(),
                "token_estimate": (chunk.len() / 4).max(1),
                "heading_hint": "",
                "page_hint": null,
                "sheet_hint": null,
                "slide_hint": null,
                "content_terms": content_terms,
                "rare_terms": rare_terms,
                "phrase_signatures": phrase_signatures(&terms),
                "intent_tags": [],
                "preview": preview
            })
        })
        .collect()
}

fn terms_for(text: &str, limit: usize) -> Vec<String> {
    let mut terms = Vec::<String>::new();
    for token in text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-')) {
        let token = token.trim_matches(['.', '_', '-']).to_ascii_lowercase();
        if token.len() >= 2 && !terms.contains(&token) {
            terms.push(token);
            if terms.len() >= limit {
                break;
            }
        }
    }
    terms
}

fn phrase_signatures(terms: &[String]) -> Vec<String> {
    terms
        .windows(2)
        .take(8)
        .map(|pair| pair.join(" "))
        .collect()
}
