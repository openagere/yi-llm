use serde_json::{json, Map, Value};

/// Keys that OpenAI-compatible tool validators reject or ignore. `$defs` and
/// `definitions` are dropped only after their references have been inlined.
const DROPPED_KEYS: &[&str] = &["$schema", "$id", "$comment", "$defs", "definitions"];

/// Rewrites a tool's JSON Schema into the subset OpenAI-compatible Responses
/// upstreams accept: local `$ref`s inlined, no nullable type unions, nullable
/// `anyOf` branches collapsed, and a top-level object with `properties`.
pub fn sanitize_tool_schema(schema: &Value) -> Value {
    let definitions = collect_definitions(schema);
    let mut normalized = normalize(schema, &definitions, &mut Vec::new());
    if let Some(object) = normalized.as_object_mut() {
        object.entry("type").or_insert_with(|| json!("object"));
        object.entry("properties").or_insert_with(|| json!({}));
    } else {
        normalized = json!({"type":"object","properties":{}});
    }
    normalized
}

fn collect_definitions(schema: &Value) -> Map<String, Value> {
    let mut definitions = Map::new();
    for key in ["$defs", "definitions"] {
        if let Some(entries) = schema.get(key).and_then(Value::as_object) {
            definitions.extend(
                entries
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone())),
            );
        }
    }
    definitions
}

fn normalize(schema: &Value, definitions: &Map<String, Value>, stack: &mut Vec<String>) -> Value {
    let Some(object) = schema.as_object() else {
        return schema.clone();
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or_default().to_owned();
        return match definitions.get(&name) {
            // Recursive or unknown references cannot be inlined, so accept any object.
            Some(target) if !stack.contains(&name) => {
                stack.push(name);
                let resolved = normalize(target, definitions, stack);
                stack.pop();
                resolved
            }
            _ => json!({"type":"object"}),
        };
    }

    let mut out = Map::new();
    for (key, value) in object {
        if DROPPED_KEYS.contains(&key.as_str()) {
            continue;
        }
        match key.as_str() {
            "description" if value.is_null() => {}
            "type" => {
                if let Some(kind) = normalize_type(value) {
                    out.insert(key.clone(), kind);
                }
            }
            "properties" => {
                if let Some(properties) = value.as_object() {
                    let mut normalized = Map::new();
                    for (name, property) in properties {
                        normalized.insert(name.clone(), normalize(property, definitions, stack));
                    }
                    out.insert(key.clone(), Value::Object(normalized));
                }
            }
            "items" | "additionalProperties" | "not" => {
                out.insert(key.clone(), normalize(value, definitions, stack));
            }
            "anyOf" | "oneOf" | "allOf" => {
                if let Some(branches) = value.as_array() {
                    let normalized = branches
                        .iter()
                        .map(|branch| normalize(branch, definitions, stack))
                        .collect();
                    out.insert(key.clone(), Value::Array(normalized));
                }
            }
            _ => {
                out.insert(key.clone(), value.clone());
            }
        }
    }

    collapse_nullable_branches(&mut out);
    if out.contains_key("properties") {
        out.entry("type").or_insert_with(|| json!("object"));
    }
    prune_required(&mut out);
    Value::Object(out)
}

/// `["string", "null"]` becomes `"string"`. A null-only union keeps `"null"`.
fn normalize_type(value: &Value) -> Option<Value> {
    let Some(types) = value.as_array() else {
        return Some(value.clone());
    };
    types
        .iter()
        .find(|kind| kind.as_str() != Some("null"))
        .or_else(|| types.first())
        .cloned()
}

fn is_null_schema(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("null")
}

/// `anyOf: [X, {"type": "null"}]` is folded into `X` when only one branch remains.
fn collapse_nullable_branches(out: &mut Map<String, Value>) {
    for key in ["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = out.get(key).cloned() else {
            continue;
        };
        let total = branches.len();
        let mut remaining: Vec<Value> = branches
            .into_iter()
            .filter(|branch| !is_null_schema(branch))
            .collect();
        if remaining.len() == total {
            continue;
        }
        if remaining.len() == 1 {
            out.remove(key);
            if let Some(branch) = remaining
                .pop()
                .and_then(|branch| branch.as_object().cloned())
            {
                for (name, value) in branch {
                    out.entry(name).or_insert(value);
                }
            }
        } else if remaining.is_empty() {
            out.remove(key);
        } else {
            out.insert(key.to_owned(), Value::Array(remaining));
        }
    }
}

/// Drops `required` names that have no matching property.
fn prune_required(out: &mut Map<String, Value>) {
    let Some(properties) = out.get("properties").and_then(Value::as_object) else {
        return;
    };
    let Some(required) = out.get("required").and_then(Value::as_array) else {
        return;
    };
    let kept: Vec<Value> = required
        .iter()
        .filter(|name| {
            name.as_str()
                .is_some_and(|name| properties.contains_key(name))
        })
        .cloned()
        .collect();
    if kept.is_empty() {
        out.remove("required");
    } else {
        out.insert("required".to_owned(), Value::Array(kept));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_schema_marker_and_inlines_local_refs() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"mode": {"$ref": "#/$defs/Mode"}},
            "$defs": {"Mode": {"type": "string", "enum": ["fast", "slow"]}}
        });
        let out = sanitize_tool_schema(&schema);
        assert!(out.get("$schema").is_none());
        assert!(out.get("$defs").is_none());
        assert_eq!(
            out["properties"]["mode"],
            json!({"type": "string", "enum": ["fast", "slow"]})
        );
    }

    #[test]
    fn recursive_refs_do_not_loop() {
        let schema = json!({
            "type": "object",
            "properties": {"node": {"$ref": "#/$defs/Node"}},
            "$defs": {"Node": {"type": "object", "properties": {"next": {"$ref": "#/$defs/Node"}}}}
        });
        let out = sanitize_tool_schema(&schema);
        assert_eq!(
            out["properties"]["node"]["properties"]["next"],
            json!({"type": "object"})
        );
    }

    #[test]
    fn nullable_type_union_becomes_single_type() {
        let out = sanitize_tool_schema(&json!({
            "type": "object",
            "properties": {"path": {"type": ["string", "null"]}}
        }));
        assert_eq!(out["properties"]["path"], json!({"type": "string"}));
    }

    #[test]
    fn nullable_any_of_collapses_into_parent() {
        let out = sanitize_tool_schema(&json!({
            "type": "object",
            "properties": {"limit": {
                "anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}],
                "description": "max rows"
            }}
        }));
        assert_eq!(
            out["properties"]["limit"],
            json!({"type": "integer", "minimum": 1, "description": "max rows"})
        );
    }

    #[test]
    fn top_level_any_of_with_single_object_branch_is_flattened() {
        let out = sanitize_tool_schema(&json!({
            "anyOf": [{"type": "object", "properties": {"cmd": {"type": "string"}}}, {"type": "null"}]
        }));
        assert_eq!(out["type"], "object");
        assert_eq!(out["properties"]["cmd"], json!({"type": "string"}));
        assert!(out.get("anyOf").is_none());
    }

    #[test]
    fn missing_root_type_is_filled_in() {
        let out = sanitize_tool_schema(&json!({"properties": {"a": {"type": "string"}}}));
        assert_eq!(out["type"], "object");
    }

    #[test]
    fn empty_schema_becomes_empty_object() {
        assert_eq!(
            sanitize_tool_schema(&json!({})),
            json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn required_names_without_properties_are_removed() {
        let out = sanitize_tool_schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a", "ghost"]
        }));
        assert_eq!(out["required"], json!(["a"]));
        let none = sanitize_tool_schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["ghost"]
        }));
        assert!(none.get("required").is_none());
    }

    #[test]
    fn null_descriptions_are_dropped() {
        let out = sanitize_tool_schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string", "description": null}}
        }));
        assert!(out["properties"]["a"].get("description").is_none());
    }
}
