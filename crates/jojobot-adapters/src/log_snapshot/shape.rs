use schemars::JsonSchema;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Definitions and Rust names are schema notation, not serialized fields.
/// Expand references before hashing and leave prose out of compatibility.
pub fn fingerprint<T: JsonSchema>() -> u32 {
    let schema =
        serde_json::to_value(schemars::schema_for!(T)).expect("a generated schema serializes");
    let shape = shape(&schema, &schema);
    let bytes = serde_json::to_vec(&shape).expect("a schema value serializes");
    let digest = Sha256::digest(bytes);
    u32::from_be_bytes(digest[..4].try_into().expect("four fingerprint bytes"))
}

fn shape(root: &Value, node: &Value) -> Value {
    match node {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let target = root
                    .pointer(
                        reference
                            .strip_prefix('#')
                            .expect("a local schema reference"),
                    )
                    .expect("a generated reference resolves");
                return shape(root, target);
            }
            let mut out: serde_json::Map<String, Value> = object
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "$defs"
                            | "$schema"
                            | "title"
                            | "description"
                            | "examples"
                            | "default"
                            | "deprecated"
                    )
                })
                .map(|(key, value)| {
                    let value = if key == "properties" {
                        Value::Object(
                            value
                                .as_object()
                                .expect("schema properties")
                                .iter()
                                .map(|(name, schema)| (name.clone(), shape(root, schema)))
                                .collect(),
                        )
                    } else {
                        shape(root, value)
                    };
                    (key.clone(), value)
                })
                .collect();
            out.sort_keys();
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| shape(root, item)).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
#[allow(dead_code)] // These types are schemas, never runtime fixture values.
mod tests {
    use super::fingerprint;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    struct Before {
        count: u64,
    }

    /// Different Rust names and prose do not change the serialized shape.
    #[derive(JsonSchema)]
    struct SameShape {
        /// A count with a new description.
        count: u64,
    }

    #[derive(JsonSchema)]
    struct Added {
        count: u64,
        ready: bool,
    }

    #[derive(JsonSchema)]
    struct Removed {}

    #[derive(JsonSchema)]
    struct Retyped {
        count: String,
    }

    #[derive(JsonSchema)]
    struct AddedAnnotationName {
        count: u64,
        description: Option<String>,
    }

    #[test]
    fn snapshot_shape_changes_with_fields_and_not_with_rust_names_or_prose() {
        assert_eq!(fingerprint::<Before>(), fingerprint::<SameShape>());
        assert_ne!(fingerprint::<Before>(), fingerprint::<Added>());
        assert_ne!(fingerprint::<Before>(), fingerprint::<Removed>());
        assert_ne!(fingerprint::<Before>(), fingerprint::<Retyped>());
        assert_ne!(
            fingerprint::<Before>(),
            fingerprint::<AddedAnnotationName>()
        );
    }
}
