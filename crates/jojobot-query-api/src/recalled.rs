//! The typed answer of an app's read: the objects it may read, each as its
//! handle, name, kind and folded fields, and never the handler's text.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::Answered;

/// **One object of an app's read.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecalledObject {
    /// The handle.
    pub handle: String,
    /// The name.
    pub name: String,
    /// The kind's token.
    pub kind: String,
    /// What the object holds now, one value for each key.
    pub fields: BTreeMap<String, String>,
}

/// **What an app's read served.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecalledView {
    /// The objects, in the order the selection gave them.
    pub objects: Vec<RecalledObject>,
    /// How many objects the page left out.
    pub left_out: usize,
}

impl RecalledView {
    /// **The view of an answer**: at most `limit` objects, each with the keys the
    /// call narrowed to when it narrowed any, and the count the page left out.
    pub fn of(answered: Answered, only_keys: Option<&[String]>, limit: usize) -> RecalledView {
        let held = answered.selected.objects.len();
        let objects: Vec<RecalledObject> = answered
            .selected
            .objects
            .into_iter()
            .take(limit)
            .map(|object| {
                let mut fields = object.fields;
                if let Some(keep) = only_keys {
                    fields.retain(|key, _| keep.contains(key));
                }
                RecalledObject {
                    handle: object.entity.id.to_string(),
                    name: object.entity.name,
                    kind: object.entity.kind.as_token().to_string(),
                    fields,
                }
            })
            .collect();
        RecalledView {
            left_out: held - objects.len(),
            objects,
        }
    }

    /// The view as the JSON an app is served.
    pub fn to_json(&self) -> Value {
        json!({
            "objects": self.objects.iter().map(|object| json!({
                "handle": object.handle,
                "name": object.name,
                "kind": object.kind,
                "fields": object.fields,
            })).collect::<Vec<_>>(),
            "left_out": self.left_out,
        })
    }
}
