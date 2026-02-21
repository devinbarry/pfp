use serde_json::Value;

/// Parse a list of "key.path=value" strings into a nested JSON object.
#[allow(dead_code)]
pub fn build_params(sets: &[String]) -> Result<Value, String> {
    let mut root = serde_json::Map::new();

    for entry in sets {
        let (key, val) = entry
            .split_once('=')
            .ok_or_else(|| format!("Invalid --set format '{}', expected key=value", entry))?;

        let parts: Vec<&str> = key.split('.').collect();
        let typed_val = auto_type(val);

        insert_nested(&mut root, &parts, typed_val)?;
    }

    Ok(Value::Object(root))
}

/// Merge `overrides` into `base` (deep merge at each level).
#[allow(dead_code)]
pub fn merge_params(base: &Value, overrides: &Value) -> Value {
    match (base, overrides) {
        (Value::Object(b), Value::Object(o)) => {
            let mut merged = b.clone();
            for (k, v) in o {
                let existing = merged.get(k).cloned().unwrap_or(Value::Null);
                merged.insert(k.clone(), merge_params(&existing, v));
            }
            Value::Object(merged)
        }
        (_, override_val) => override_val.clone(),
    }
}

fn auto_type(val: &str) -> Value {
    if val == "true" {
        return Value::Bool(true);
    }
    if val == "false" {
        return Value::Bool(false);
    }
    if let Ok(n) = val.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = val.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }
    Value::String(val.to_string())
}

fn insert_nested(
    map: &mut serde_json::Map<String, Value>,
    parts: &[&str],
    value: Value,
) -> Result<(), String> {
    if parts.is_empty() {
        return Err("Empty key path".to_string());
    }

    if parts.len() == 1 {
        map.insert(parts[0].to_string(), value);
        return Ok(());
    }

    let entry = map
        .entry(parts[0].to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    match entry {
        Value::Object(ref mut inner) => insert_nested(inner, &parts[1..], value),
        _ => Err(format!(
            "Conflict: '{}' is not an object, cannot set '{}'",
            parts[0],
            parts[1..].join(".")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_key_value() {
        let result = build_params(&["name=hello".to_string()]).unwrap();
        assert_eq!(result, json!({"name": "hello"}));
    }

    #[test]
    fn dotted_path() {
        let result = build_params(&["config.action=destroy".to_string()]).unwrap();
        assert_eq!(result, json!({"config": {"action": "destroy"}}));
    }

    #[test]
    fn multiple_dotted_paths() {
        let result = build_params(&[
            "config.action=destroy".to_string(),
            "config.auto_approve=true".to_string(),
        ])
        .unwrap();
        assert_eq!(
            result,
            json!({"config": {"action": "destroy", "auto_approve": true}})
        );
    }

    #[test]
    fn auto_type_bool_true() {
        assert_eq!(auto_type("true"), Value::Bool(true));
    }

    #[test]
    fn auto_type_bool_false() {
        assert_eq!(auto_type("false"), Value::Bool(false));
    }

    #[test]
    fn auto_type_integer() {
        assert_eq!(auto_type("42"), json!(42));
    }

    #[test]
    fn auto_type_float() {
        assert_eq!(auto_type("3.14"), json!(3.14));
    }

    #[test]
    fn auto_type_string() {
        assert_eq!(auto_type("hello"), json!("hello"));
    }

    #[test]
    fn missing_equals_returns_error() {
        let result = build_params(&["no-equals".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn merge_override_wins() {
        let base = json!({"config": {"action": "plan", "dry_run": false}});
        let overrides = json!({"config": {"action": "destroy"}});
        let merged = merge_params(&base, &overrides);
        assert_eq!(merged, json!({"config": {"action": "destroy", "dry_run": false}}));
    }

    #[test]
    fn merge_adds_new_keys() {
        let base = json!({"environment": "prod"});
        let overrides = json!({"config": {"action": "plan"}});
        let merged = merge_params(&base, &overrides);
        assert_eq!(merged, json!({"environment": "prod", "config": {"action": "plan"}}));
    }

    #[test]
    fn deep_dotted_path() {
        let result = build_params(&["a.b.c=deep".to_string()]).unwrap();
        assert_eq!(result, json!({"a": {"b": {"c": "deep"}}}));
    }
}
