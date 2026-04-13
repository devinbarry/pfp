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

/// Extract the definitions map from a top-level schema, checking both
/// "definitions" (Pydantic v1 / OpenAPI 3.0) and "$defs" (Pydantic v2 / JSON Schema 2020-12).
#[allow(dead_code)]
fn get_definitions(schema: &Value) -> &Value {
    schema
        .get("definitions")
        .or_else(|| schema.get("$defs"))
        .unwrap_or(&Value::Null)
}

/// Strip the "#/definitions/" or "#/$defs/" prefix from a $ref string
/// and look up the target in the definitions map.
#[allow(dead_code)]
fn resolve_ref<'a>(ref_str: &str, definitions: &'a Value) -> Option<&'a Value> {
    let name = ref_str
        .strip_prefix("#/definitions/")
        .or_else(|| ref_str.strip_prefix("#/$defs/"))?;
    definitions.get(name)
}

/// Extract valid property names and their schema nodes from a schema node.
/// Returns None if: the node allows arbitrary keys (additionalProperties),
/// is a scalar type, or cannot be resolved (broken $ref, cycle).
///
/// `visited` tracks resolved $ref targets to prevent infinite recursion.
#[allow(dead_code)]
fn resolve_properties(
    schema_node: &Value,
    definitions: &Value,
    visited: &mut std::collections::HashSet<String>,
) -> Option<HashMap<String, Value>> {
    // additionalProperties (not false) means any key is valid — skip validation
    if let Some(ap) = schema_node.get("additionalProperties") {
        if *ap != Value::Bool(false) {
            return None;
        }
    }

    let mut props: HashMap<String, Value> = HashMap::new();

    // Follow $ref
    if let Some(Value::String(ref_str)) = schema_node.get("$ref") {
        if visited.contains(ref_str) {
            // Cycle detected — return what we have (possibly empty → None)
            return if props.is_empty() { None } else { Some(props) };
        }
        visited.insert(ref_str.clone());
        if let Some(target) = resolve_ref(ref_str, definitions) {
            if let Some(ref_props) = resolve_properties(target, definitions, visited) {
                props.extend(ref_props);
            }
        }
        return if props.is_empty() { None } else { Some(props) };
    }

    // Collect inline properties
    if let Some(Value::Object(p)) = schema_node.get("properties") {
        for (key, val) in p {
            props.insert(key.clone(), val.clone());
        }
    }

    // Merge allOf entries
    if let Some(Value::Array(items)) = schema_node.get("allOf") {
        for item in items {
            if let Some(item_props) = resolve_properties(item, definitions, visited) {
                props.extend(item_props);
            }
        }
    }

    // Union anyOf / oneOf entries (skip null types)
    for keyword in &["anyOf", "oneOf"] {
        if let Some(Value::Array(items)) = schema_node.get(*keyword) {
            for item in items {
                // Skip {"type": "null"} entries
                if item.get("type").and_then(Value::as_str) == Some("null") {
                    continue;
                }
                if let Some(item_props) = resolve_properties(item, definitions, visited) {
                    props.extend(item_props);
                }
            }
        }
    }

    if props.is_empty() {
        None
    } else {
        Some(props)
    }
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

    // -- resolve_properties tests --

    #[test]
    fn resolve_flat_properties() {
        let schema = json!({
            "type": "object",
            "properties": {
                "dry_run": { "type": "boolean" },
                "action": { "type": "string" }
            }
        });
        let defs = json!({});
        let result = resolve_properties(&schema, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("dry_run"));
        assert!(props.contains_key("action"));
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn resolve_ref_into_definitions() {
        let node = json!({ "$ref": "#/definitions/FlowConfig" });
        let defs = json!({
            "FlowConfig": {
                "type": "object",
                "properties": {
                    "dry_run": { "type": "boolean" },
                    "action": { "type": "string" }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("dry_run"));
        assert!(props.contains_key("action"));
    }

    #[test]
    fn resolve_ref_into_dollar_defs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": { "$ref": "#/$defs/FlowConfig" }
            },
            "$defs": {
                "FlowConfig": {
                    "type": "object",
                    "properties": {
                        "dry_run": { "type": "boolean" }
                    }
                }
            }
        });
        let defs = schema.get("$defs").unwrap();
        let config_node = json!({ "$ref": "#/$defs/FlowConfig" });
        let result = resolve_properties(&config_node, defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("dry_run"));
    }

    #[test]
    fn resolve_allof_with_ref() {
        let node = json!({
            "allOf": [{ "$ref": "#/definitions/FlowConfig" }],
            "default": {}
        });
        let defs = json!({
            "FlowConfig": {
                "type": "object",
                "properties": {
                    "dry_run": { "type": "boolean" },
                    "action": { "type": "string" }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("dry_run"));
        assert!(props.contains_key("action"));
    }

    #[test]
    fn resolve_allof_merges_multiple_entries() {
        let node = json!({
            "allOf": [
                { "$ref": "#/definitions/Parent" },
                { "$ref": "#/definitions/Child" }
            ]
        });
        let defs = json!({
            "Parent": {
                "type": "object",
                "properties": {
                    "base_field": { "type": "string" }
                }
            },
            "Child": {
                "type": "object",
                "properties": {
                    "child_field": { "type": "integer" }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("base_field"));
        assert!(props.contains_key("child_field"));
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn resolve_allof_with_inline_properties() {
        let node = json!({
            "properties": {
                "inline_field": { "type": "string" }
            },
            "allOf": [
                { "$ref": "#/definitions/Extra" }
            ]
        });
        let defs = json!({
            "Extra": {
                "type": "object",
                "properties": {
                    "extra_field": { "type": "integer" }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("inline_field"));
        assert!(props.contains_key("extra_field"));
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn resolve_anyof_optional_model() {
        let node = json!({
            "anyOf": [
                { "$ref": "#/definitions/FlowConfig" },
                { "type": "null" }
            ]
        });
        let defs = json!({
            "FlowConfig": {
                "type": "object",
                "properties": {
                    "dry_run": { "type": "boolean" }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("dry_run"));
    }

    #[test]
    fn resolve_oneof_multiple_variants() {
        let node = json!({
            "oneOf": [
                { "$ref": "#/definitions/TypeA" },
                { "$ref": "#/definitions/TypeB" }
            ]
        });
        let defs = json!({
            "TypeA": {
                "type": "object",
                "properties": { "field_a": { "type": "string" } }
            },
            "TypeB": {
                "type": "object",
                "properties": { "field_b": { "type": "integer" } }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("field_a"));
        assert!(props.contains_key("field_b"));
    }

    #[test]
    fn resolve_anyof_null_only() {
        let node = json!({ "anyOf": [{ "type": "null" }] });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }

    #[test]
    fn resolve_additional_properties_allows_any_key() {
        let node = json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }

    #[test]
    fn resolve_additional_properties_false_validates_normally() {
        let node = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "additionalProperties": false
        });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("name"));
        assert_eq!(props.len(), 1);
    }

    #[test]
    fn resolve_self_referential_schema_no_crash() {
        let node = json!({ "$ref": "#/definitions/Node" });
        let defs = json!({
            "Node": {
                "type": "object",
                "properties": {
                    "value": { "type": "string" },
                    "children": {
                        "type": "array",
                        "items": { "$ref": "#/definitions/Node" }
                    }
                }
            }
        });
        let result = resolve_properties(&node, &defs, &mut Default::default());
        let props = result.unwrap();
        assert!(props.contains_key("value"));
        assert!(props.contains_key("children"));
    }

    #[test]
    fn resolve_unresolvable_ref_returns_none() {
        let node = json!({ "$ref": "#/definitions/DoesNotExist" });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }

    #[test]
    fn resolve_no_properties_scalar() {
        let node = json!({ "type": "string" });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }

    #[test]
    fn resolve_definitions_not_object() {
        let node = json!({ "$ref": "#/definitions/Bad" });
        let defs = json!("not an object");
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }

    #[test]
    fn resolve_properties_field_not_object() {
        let node = json!({
            "type": "object",
            "properties": "not an object"
        });
        let defs = json!({});
        let result = resolve_properties(&node, &defs, &mut Default::default());
        assert!(result.is_none());
    }
}
