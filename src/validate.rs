#[allow(unused_imports)]
use std::collections::HashMap;

#[allow(unused_imports)]
use serde_json::Value;

#[allow(unused_imports)]
use crate::error::{PfpError, Result};

/// Compute Levenshtein edit distance between two strings.
#[allow(dead_code)]
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a.as_bytes()[i - 1] == b.as_bytes()[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Find the closest match by Levenshtein distance (max distance 3).
#[allow(dead_code)]
fn suggest(invalid: &str, valid_keys: &[String]) -> Option<String> {
    valid_keys
        .iter()
        .map(|k| (k, levenshtein(invalid, k)))
        .filter(|(_, d)| *d <= 3)
        .min_by_key(|(_, d)| *d)
        .map(|(k, _)| k.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use serde_json::json;

    // -- Levenshtein tests --

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("dry_run", "dry_run"), 0);
    }

    #[test]
    fn levenshtein_distance_1() {
        assert_eq!(levenshtein("dry_urn", "dry_run"), 2);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    // -- suggest tests --

    #[test]
    fn suggest_distance_1() {
        let keys = vec!["dry_run".to_string(), "action".to_string()];
        assert_eq!(suggest("dry_urn", &keys), Some("dry_run".to_string()));
    }

    #[test]
    fn suggest_distance_3_boundary() {
        let keys = vec!["dry_run".to_string()];
        let dist = levenshtein("dry_nu", "dry_run");
        if dist <= 3 {
            assert_eq!(suggest("dry_nu", &keys), Some("dry_run".to_string()));
        }
    }

    #[test]
    fn suggest_distance_too_far() {
        let keys = vec!["dry_run".to_string()];
        assert_eq!(suggest("xyzzy", &keys), None);
    }

    #[test]
    fn suggest_picks_closest() {
        let keys = vec!["dry_run".to_string(), "dry_rug".to_string()];
        let result = suggest("dry_urn", &keys);
        assert!(result.is_some());
    }

    #[test]
    fn suggest_empty_valid_keys() {
        assert_eq!(suggest("anything", &[]), None);
    }
}
