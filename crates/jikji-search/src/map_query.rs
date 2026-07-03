use crate::tokenizer::{compact_lookup_text, filename_lookup_keys, tokens};

pub(crate) fn compact_query_terms(query: &str) -> Vec<String> {
    tokens(query, 64)
        .into_iter()
        .map(|token| compact_lookup_text(&token))
        .filter(|token| {
            (has_cjk(token) && token.chars().count() >= 6)
                || token.chars().filter(|ch| ch.is_ascii_digit()).count() >= 4
        })
        .collect()
}

pub(crate) fn filename_anchors(query: &str) -> Vec<String> {
    if !is_filename_query(query) && !is_duplicate_query(query) {
        return Vec::new();
    }
    tokens(query, 24)
        .into_iter()
        .filter(|token| is_strong_filename_anchor(token))
        .flat_map(|token| filename_lookup_keys(&token))
        .collect()
}

pub(crate) fn quoted_terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in query.chars() {
        if matches!(ch, '"' | '\'' | '‘' | '’' | '“' | '”') {
            if in_quote {
                let term = current.trim().to_lowercase();
                if term.chars().count() >= 2 {
                    out.push(term);
                }
                current.clear();
            }
            in_quote = !in_quote;
        } else if in_quote {
            current.push(ch);
        }
    }
    out
}

pub(crate) fn date_anchors(query: &str) -> Vec<String> {
    const MONTHS: &[(&str, &str)] = &[
        ("january", "jan"),
        ("february", "feb"),
        ("march", "mar"),
        ("april", "apr"),
        ("may", "may"),
        ("june", "jun"),
        ("july", "jul"),
        ("august", "aug"),
        ("september", "sep"),
        ("october", "oct"),
        ("november", "nov"),
        ("december", "dec"),
    ];
    let lower = query.to_lowercase();
    let words = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for window in words.windows(2) {
        let month = MONTHS.iter().find_map(|(full, short)| {
            (window[0] == *full || window[0] == *short).then_some(*short)
        });
        if let Some(short) = month {
            if let Ok(day) = window[1].parse::<u8>() {
                out.push(format!("{short}{day}"));
                out.push(format!("{short}{day:02}"));
            }
        }
    }
    out
}

pub(crate) fn is_strong_entity_token(token: &str) -> bool {
    if token.chars().count() < 4 || token.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    !matches!(
        token,
        "week"
            | "year"
            | "years"
            | "quarter"
            | "quarters"
            | "growth"
            | "margin"
            | "margins"
            | "gross"
            | "percentage"
            | "changes"
            | "expense"
            | "expenses"
            | "records"
            | "health"
            | "school"
            | "break"
            | "things"
            | "everything"
            | "related"
            | "since"
            | "came"
            | "home"
            | "something"
            | "creative"
            | "technical"
            | "again"
            | "look"
            | "sort"
            | "take"
            | "wondering"
    )
}

pub(crate) fn contains_word(text: &str, token: &str) -> bool {
    text.split(|ch: char| !ch.is_alphanumeric())
        .any(|part| part == token)
}

pub(crate) fn query_bigrams(ordered_tokens: &[String]) -> Vec<(String, String)> {
    ordered_tokens
        .windows(2)
        .map(|items| (items[0].clone(), items[1].clone()))
        .collect()
}

pub(crate) fn has_cjk(text: &str) -> bool {
    text.chars().any(|ch| ('가'..='힣').contains(&ch))
}

fn is_filename_query(query: &str) -> bool {
    let compact = compact_lookup_text(query);
    ["파일명", "제목", "이름", "filename", "name"]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn is_duplicate_query(query: &str) -> bool {
    let compact = compact_lookup_text(query);
    ["사본", "중복", "동일", "duplicate", "copy"]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn is_strong_filename_anchor(raw: &str) -> bool {
    let compact = compact_lookup_text(raw);
    if compact.len() < 3 {
        return false;
    }
    compact.chars().any(|ch| ch.is_ascii_digit())
        || raw.chars().filter(|ch| ch.is_alphabetic()).count() >= 5
}
