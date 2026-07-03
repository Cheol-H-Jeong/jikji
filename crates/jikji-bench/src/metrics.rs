use serde_json::{Value, json};

pub(crate) fn metrics(cases: usize, ranks: &[Option<usize>]) -> Value {
    let hit_at = |limit: usize| -> f64 {
        if cases == 0 {
            return 0.0;
        }
        let hits = ranks
            .iter()
            .filter(|rank| rank.is_some_and(|value| value <= limit))
            .count();
        round4(hits as f64 / cases as f64)
    };
    let reciprocal_sum = ranks
        .iter()
        .filter_map(|rank| rank.map(|value| 1.0 / value as f64))
        .sum::<f64>();
    json!({
        "cases": cases,
        "hit_at_1": hit_at(1),
        "hit_at_5": hit_at(5),
        "hit_at_10": hit_at(10),
        "duplicate_or_exact_hit_at_10": hit_at(10),
        "mrr": if cases == 0 { 0.0 } else { round4(reciprocal_sum / cases as f64) },
        "seconds": 0.0,
    })
}

pub(crate) fn rank_of_any(paths: &[String], expected: &[String]) -> Option<usize> {
    paths
        .iter()
        .position(|path| expected.iter().any(|expected_path| path == expected_path))
        .map(|idx| idx + 1)
}

pub(crate) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

pub(crate) fn round4(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}
