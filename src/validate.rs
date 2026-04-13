use std::collections::HashMap;

use serde_json::Value;

use crate::error::{PfpError, Result};

/// Compute Levenshtein edit distance between two strings.
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
fn get_definitions(schema: &Value) -> &Value {
    schema
        .get("definitions")
        .or_else(|| schema.get("$defs"))
        .unwrap_or(&Value::Null)
}

/// Strip the "#/definitions/" or "#/$defs/" prefix from a $ref string
/// and look up the target in the definitions map.
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

/// Recursively walk user parameters against the schema, collecting errors.
fn walk_params(
    params: &Value,
    schema_node: &Value,
    definitions: &Value,
    path: &str,
    errors: &mut Vec<(String, Option<String>, Vec<String>)>,
) {
    let obj = match params.as_object() {
        Some(o) => o,
        None => return,
    };

    let props = resolve_properties(schema_node, definitions, &mut Default::default());

    let valid_props = match props {
        Some(p) => p,
        None => return,
    };

    let mut valid_keys: Vec<String> = valid_props.keys().cloned().collect();
    valid_keys.sort();

    for (key, value) in obj {
        let full_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", path, key)
        };

        if let Some(child_schema) = valid_props.get(key) {
            if value.is_object() {
                walk_params(value, child_schema, definitions, &full_path, errors);
            }
        } else {
            let suggestion = suggest(key, &valid_keys);
            errors.push((full_path, suggestion, valid_keys.clone()));
        }
    }
}

/// Format the context label for a path.
fn format_context(path: &str) -> String {
    if path.is_empty() {
        "top-level parameters".to_string()
    } else {
        format!("parameters for {}", path)
    }
}

/// Validate user parameters against a deployment's OpenAPI schema.
pub fn validate_params(params: &Value, schema: &Value) -> Result<()> {
    if !params.is_object() {
        return Ok(());
    }

    let definitions = get_definitions(schema);

    let mut errors: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    walk_params(params, schema, definitions, "", &mut errors);

    if errors.is_empty() {
        return Ok(());
    }

    let mut msg = String::new();

    if errors.len() == 1 {
        let (ref path, ref suggestion, _) = errors[0];
        msg.push_str(&format!("unknown parameter '{}'", path));

        let parent = path.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
        let context = format_context(parent);
        msg.push_str(&format!(
            "\n\nValid {}:\n  {}",
            context,
            errors[0].2.join(", ")
        ));

        if let Some(ref s) = suggestion {
            let suggested_full = if let Some((parent, _)) = path.rsplit_once('.') {
                format!("{}.{}", parent, s)
            } else {
                s.clone()
            };
            msg.push_str(&format!("\n\nDid you mean '{}'?", suggested_full));
        }
    } else {
        msg.push_str("unknown parameters found\n");
        for (ref path, ref suggestion, _) in &errors {
            match suggestion {
                Some(s) => {
                    let suggested_full = if let Some((parent, _)) = path.rsplit_once('.') {
                        format!("{}.{}", parent, s)
                    } else {
                        s.clone()
                    };
                    msg.push_str(&format!(
                        "\n  '{}' — did you mean '{}'?",
                        path, suggested_full
                    ));
                }
                None => {
                    msg.push_str(&format!("\n  '{}' — no close match", path));
                }
            }
        }

        let mut seen_parents: Vec<String> = Vec::new();
        for (ref path, _, ref valid_keys) in &errors {
            let parent = path
                .rsplit_once('.')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            if !seen_parents.contains(&parent) {
                seen_parents.push(parent.clone());
                let context = format_context(&parent);
                msg.push_str(&format!(
                    "\n\nValid {}:\n  {}",
                    context,
                    valid_keys.join(", ")
                ));
            }
        }
    }

    Err(PfpError::Validation(msg))
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
    fn levenshtein_transposition_distance_2() {
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

    // -- validate_params tests --

    fn make_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "config": { "$ref": "#/definitions/FlowConfig" },
                "environment": { "type": "string", "default": "production" }
            },
            "definitions": {
                "FlowConfig": {
                    "type": "object",
                    "properties": {
                        "dry_run": { "type": "boolean", "default": false },
                        "action": { "type": "string" },
                        "git_ref": { "type": "string" },
                        "deployment_name": { "type": "string" },
                        "inventory_name": { "type": "string" },
                        "playbook_name": { "type": "string" },
                        "ansible_debug": { "type": "boolean" },
                        "ansible_limit": { "type": "string" },
                        "ansible_tags": { "type": "string" },
                        "vault_secrets": { "type": "boolean" }
                    }
                }
            }
        })
    }

    #[test]
    fn validate_valid_params_pass() {
        let schema = make_schema();
        let params = json!({"config": {"dry_run": true, "action": "plan"}});
        assert!(validate_params(&params, &schema).is_ok());
    }

    #[test]
    fn validate_valid_top_level_param() {
        let schema = make_schema();
        let params = json!({"environment": "staging"});
        assert!(validate_params(&params, &schema).is_ok());
    }

    #[test]
    fn validate_empty_overrides_pass() {
        let schema = make_schema();
        let params = json!({});
        assert!(validate_params(&params, &schema).is_ok());
    }

    #[test]
    fn validate_unknown_top_level_rejected() {
        let schema = make_schema();
        let params = json!({"conifg": {"dry_run": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("conifg"),
            "should mention invalid key: {}",
            msg
        );
        assert!(msg.contains("config"), "should suggest 'config': {}", msg);
    }

    #[test]
    fn validate_unknown_nested_rejected() {
        let schema = make_schema();
        let params = json!({"config": {"dry_urn": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("dry_urn"),
            "should mention invalid key: {}",
            msg
        );
        assert!(msg.contains("dry_run"), "should suggest 'dry_run': {}", msg);
    }

    #[test]
    fn validate_multiple_errors_reported() {
        let schema = make_schema();
        let params = json!({"config": {"dry_urn": true, "foobar": "x"}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("dry_urn"), "should mention dry_urn: {}", msg);
        assert!(msg.contains("foobar"), "should mention foobar: {}", msg);
    }

    #[test]
    fn validate_no_suggestion_for_distant_key() {
        let schema = make_schema();
        let params = json!({"config": {"xyzzy_foo_bar": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("xyzzy_foo_bar"),
            "should mention invalid key: {}",
            msg
        );
        assert!(
            !msg.contains("did you mean"),
            "should not suggest for distant key: {}",
            msg
        );
    }

    #[test]
    fn validate_error_shows_valid_keys_sorted() {
        let schema = make_schema();
        let params = json!({"config": {"bogus": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("action"), "should list valid keys: {}", msg);
        assert!(msg.contains("dry_run"), "should list valid keys: {}", msg);
        let action_pos = msg.find("action").unwrap();
        let dry_run_pos = msg.find("dry_run").unwrap();
        assert!(action_pos < dry_run_pos, "keys should be sorted: {}", msg);
    }

    #[test]
    fn validate_shows_full_dotted_path_in_context() {
        let schema = make_schema();
        let params = json!({"config": {"bogus": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("config.bogus"),
            "should show full dotted path: {}",
            msg
        );
    }

    #[test]
    fn validate_errors_from_different_parents_grouped() {
        let schema = make_schema();
        let params = json!({"conifg": {}, "config": {"bogus": true}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("conifg"),
            "should mention top-level error: {}",
            msg
        );
        assert!(
            msg.contains("config.bogus"),
            "should mention nested error: {}",
            msg
        );
    }

    #[test]
    fn validate_none_schema_properties_passes() {
        let schema = json!({"type": "object"});
        let params = json!({"anything": "goes"});
        assert!(validate_params(&params, &schema).is_ok());
    }

    #[test]
    fn validate_non_object_params_pass() {
        let schema = make_schema();
        let params = json!("not an object");
        assert!(validate_params(&params, &schema).is_ok());
    }

    // -- Multi-level $ref chain tests --

    fn make_nested_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "config": { "$ref": "#/definitions/FlowConfig" }
            },
            "definitions": {
                "FlowConfig": {
                    "type": "object",
                    "properties": {
                        "db": { "$ref": "#/definitions/DatabaseConfig" },
                        "dry_run": { "type": "boolean" }
                    }
                },
                "DatabaseConfig": {
                    "type": "object",
                    "properties": {
                        "host": { "type": "string" },
                        "port": { "type": "integer" }
                    }
                }
            }
        })
    }

    #[test]
    fn validate_two_level_nesting_valid() {
        let schema = make_nested_schema();
        let params = json!({"config": {"db": {"host": "localhost"}}});
        assert!(validate_params(&params, &schema).is_ok());
    }

    #[test]
    fn validate_two_level_nesting_invalid() {
        let schema = make_nested_schema();
        let params = json!({"config": {"db": {"hsot": "localhost"}}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("config.db.hsot"),
            "should show full path: {}",
            msg
        );
        assert!(msg.contains("host"), "should suggest 'host': {}", msg);
        assert!(
            msg.contains("config.db.host"),
            "should show full suggested path: {}",
            msg
        );
    }

    #[test]
    fn validate_two_level_error_shows_correct_context() {
        let schema = make_nested_schema();
        let params = json!({"config": {"db": {"bogus": true}}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("parameters for config.db"),
            "should show correct parent: {}",
            msg
        );
    }

    // -- $defs (Pydantic v2) full integration --

    #[test]
    fn validate_pydantic_v2_dollar_defs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "allOf": [{ "$ref": "#/$defs/FlowConfig" }],
                    "default": {}
                }
            },
            "$defs": {
                "FlowConfig": {
                    "type": "object",
                    "properties": {
                        "dry_run": { "type": "boolean" },
                        "action": { "type": "string" }
                    }
                }
            }
        });
        let params = json!({"config": {"dry_run": true}});
        assert!(validate_params(&params, &schema).is_ok());

        let params_bad = json!({"config": {"bogus": true}});
        let err = validate_params(&params_bad, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("config.bogus"),
            "should catch invalid key: {}",
            msg
        );
    }

    // -- anyOf Optional model integration --

    #[test]
    fn validate_optional_model_anyof() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "anyOf": [
                        { "$ref": "#/definitions/FlowConfig" },
                        { "type": "null" }
                    ]
                }
            },
            "definitions": {
                "FlowConfig": {
                    "type": "object",
                    "properties": {
                        "dry_run": { "type": "boolean" }
                    }
                }
            }
        });
        let params = json!({"config": {"dry_run": true}});
        assert!(validate_params(&params, &schema).is_ok());

        let params_bad = json!({"config": {"bogus": true}});
        assert!(validate_params(&params_bad, &schema).is_err());
    }

    // -- additionalProperties integration --

    #[test]
    fn validate_additional_properties_skips_validation() {
        let schema = json!({
            "type": "object",
            "properties": {
                "tags": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            },
            "definitions": {}
        });
        let params = json!({"tags": {"anything": "goes", "foo": "bar"}});
        assert!(validate_params(&params, &schema).is_ok());
    }

    // -- Inline JSON override tests --

    #[test]
    fn validate_inline_json_object_override() {
        let schema = make_schema();
        let params = json!({"config": {"dry_run": true}});
        assert!(validate_params(&params, &schema).is_ok());

        let params_bad = json!({"config": {"bogus": true}});
        assert!(validate_params(&params_bad, &schema).is_err());
    }

    #[test]
    fn validate_inline_json_nested_invalid() {
        let schema = make_nested_schema();
        let params = json!({"config": {"db": {"hsot": "x"}}});
        let err = validate_params(&params, &schema).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("config.db.hsot"),
            "should catch nested inline JSON key: {}",
            msg
        );
    }
}
