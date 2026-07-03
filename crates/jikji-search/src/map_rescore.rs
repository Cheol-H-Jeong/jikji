use serde_json::Value;

use crate::map_query::{
    compact_query_terms, contains_word, date_anchors, filename_anchors, has_cjk,
    is_strong_entity_token, query_bigrams, quoted_terms,
};
use crate::tokenizer::{compact_lookup_text, filename_lookup_keys, tokens};

pub(crate) fn rescore(
    query: &str,
    row: &Value,
    fielded_score: f64,
) -> (f64, Vec<String>, Vec<String>) {
    let ordered_tokens = tokens(query, 40);
    if ordered_tokens.is_empty() {
        return (0.0, Vec::new(), Vec::new());
    }

    let path = value_str(row, "path").to_lowercase();
    let name = value_str(row, "name").to_lowercase();
    let summary = value_str(row, "summary").to_lowercase();
    let body = value_str(row, "body_text").to_lowercase();
    let content = array_text(row, "content_terms").to_lowercase();
    let rare = array_text(row, "rare_terms").to_lowercase();
    let phrases = array_text(row, "phrase_signatures").to_lowercase();
    let evidence = format!("{} {}", array_text(row, "evidence_previews"), summary).to_lowercase();
    let folder = format!(
        "{} {} {} {}",
        array_text(row, "folder_terms"),
        array_text(row, "folder_roles"),
        array_text(row, "path_terms"),
        array_text(row, "name_terms")
    )
    .to_lowercase();
    let map_text = format!("{content} {rare} {phrases} {evidence} {folder} {body}");
    let compact_map = compact_lookup_text(&map_text);
    let compact_path = compact_lookup_text(&format!("{path} {name} {folder}"));
    let folder_context = is_folder_context_query(query);
    let quoted_terms = quoted_terms(query);

    let mut score = 0.0;
    let mut reasons = Vec::new();
    let mut matched = Vec::new();

    let _ = fielded_score;

    let mut original_hits = 0usize;
    for token in &ordered_tokens {
        let rare_weight = rarity_weight(token, &rare, &content, &body);
        let mut token_score = 0.0;
        if content.contains(token) {
            token_score += 24.0 * rare_weight;
        }
        if rare.contains(token) {
            token_score += 35.0 * rare_weight.max(1.0);
        }
        if phrases.contains(token) {
            token_score += 42.0 * rare_weight.max(1.0);
        }
        if evidence.contains(token) {
            token_score += 18.0 * rare_weight;
        }
        if body.contains(token) {
            token_score += body_score(&body, token, rare_weight);
        }
        if folder.contains(token) || path.contains(token) || name.contains(token) {
            token_score += if folder_context { 34.0 } else { 11.0 } * rare_weight;
        }
        if is_strong_entity_token(token) {
            if contains_word(&name, token) {
                token_score += 6_400.0;
                push_unique(&mut reasons, "rare-token-in-name");
            } else if contains_word(&path, token) || contains_word(&folder, token) {
                token_score += 2_800.0;
                push_unique(&mut reasons, "rare-token-in-path");
            }
        }
        if token_score > 0.0 {
            score += token_score * 1.25;
            original_hits += 1;
            push_unique(&mut matched, token);
        }
    }

    if original_hits >= 2 {
        score += 18.0 + 8.0 * original_hits as f64;
        reasons.push("multi-map-term".to_owned());
    }
    if original_hits >= 3 {
        score += 18.0;
        reasons.push("strong-map-term".to_owned());
    }

    let mut quoted_hits = 0usize;
    for term in &quoted_terms {
        if body.contains(term) || evidence.contains(term) || phrases.contains(term) {
            score += 3_200.0;
            quoted_hits += 1;
            push_unique(&mut reasons, "quoted-term");
            push_unique(&mut matched, term);
        } else if map_text.contains(term) {
            score += 1_200.0;
            quoted_hits += 1;
            push_unique(&mut reasons, "quoted-term");
            push_unique(&mut matched, term);
        }
    }
    if !quoted_terms.is_empty() && quoted_hits >= quoted_terms.len() {
        score += 1_000.0 + 250.0 * quoted_hits as f64;
        reasons.push("all-quoted-terms".to_owned());
    }

    let compact_hits = compact_query_terms(query)
        .into_iter()
        .map(|term| {
            if compact_map.contains(&term) {
                score += if has_cjk(&term) { 950.0 } else { 520.0 };
                push_unique(&mut reasons, "compact-exact-term");
                true
            } else if compact_path.contains(&term) {
                score += 420.0;
                push_unique(&mut reasons, "compact-path-term");
                true
            } else {
                false
            }
        })
        .filter(|hit| *hit)
        .count();
    if compact_hits >= 2 {
        score += 420.0 + 120.0 * compact_hits as f64;
        reasons.push("multi-compact-term".to_owned());
    }

    for (left, right) in query_bigrams(&ordered_tokens) {
        let phrase = format!("{left} {right}");
        if phrases.contains(&phrase) {
            score += 80.0;
            push_unique(&mut reasons, "map-phrase");
        } else if evidence.contains(&phrase) {
            score += 35.0;
            push_unique(&mut reasons, "evidence-phrase");
        }
        if folder_context && (folder.contains(&phrase) || path.contains(&phrase)) {
            score += 150.0;
            push_unique(&mut reasons, "path-phrase");
        }
    }

    for anchor in filename_anchors(query) {
        let name_keys = filename_lookup_keys(&name);
        let path_keys = filename_lookup_keys(&path);
        if name_keys.iter().any(|key| key == &anchor) {
            score += 1200.0;
            push_unique(&mut reasons, "filename-anchor");
            push_unique(&mut matched, &anchor);
        } else if name_keys
            .iter()
            .any(|key| key.len() >= 3 && (key.contains(&anchor) || anchor.contains(key)))
        {
            score += 980.0;
            push_unique(&mut reasons, "filename-anchor");
            push_unique(&mut matched, &anchor);
        } else if path_keys.iter().any(|key| key == &anchor) {
            score += 640.0;
            push_unique(&mut reasons, "filename-anchor");
            push_unique(&mut matched, &anchor);
        }
    }

    for date_anchor in date_anchors(query) {
        let compact_name = compact_lookup_text(&name);
        let compact_path = compact_lookup_text(&path);
        if compact_name.contains(&date_anchor) {
            score += 4_800.0;
            push_unique(&mut reasons, "date-anchor-in-name");
        } else if compact_path.contains(&date_anchor) {
            score += 2_400.0;
            push_unique(&mut reasons, "date-anchor-in-path");
        }
    }

    if compact_lookup_text(query).contains("schoolbreak")
        || compact_lookup_text(query).contains("break")
    {
        if contains_word(&name, "break") {
            score += 9_000.0;
            push_unique(&mut reasons, "break-plan-name");
        } else if contains_word(&path, "break") {
            score += 3_000.0;
            push_unique(&mut reasons, "break-plan-path");
        }
    }

    if score > 0.0 && reasons.is_empty() {
        reasons.push("map-overlap".to_owned());
    }
    (score, reasons, matched)
}

fn value_str(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn array_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

fn body_score(body: &str, token: &str, rare_weight: f64) -> f64 {
    let tf = body.matches(token).count() as f64;
    if tf <= 0.0 {
        return 0.0;
    }
    let length_norm = 0.5 + 0.5 * (body.len() as f64 / 1400.0).min(2.6);
    let k1 = 1.4;
    let saturation = tf * (k1 + 1.0) / (tf + k1 * length_norm);
    15.0 * rare_weight.max(1.0) * saturation
}

fn rarity_weight(token: &str, rare: &str, content: &str, body: &str) -> f64 {
    let mut weight: f64 = 1.0;
    if rare.contains(token) {
        weight += 2.0;
    }
    if content.contains(token) {
        weight += 0.8;
    }
    let tf = body.matches(token).count();
    if tf == 1 {
        weight += 1.0;
    }
    weight.min(4.0)
}

fn is_folder_context_query(query: &str) -> bool {
    let compact = compact_lookup_text(query);
    query.contains('/')
        || [
            "폴더",
            "경로",
            "아래",
            "안에",
            "folder",
            "path",
            "directory",
        ]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_owned());
    }
}
