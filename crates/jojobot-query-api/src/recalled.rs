//! The typed answer of an app's read: the objects it may read, each as its
//! handle, name, kind and folded fields, and never the handler's text.

use std::collections::BTreeMap;

use jojobot_domain::memory::MemoryError;
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
    /// **Which claim holds each value and when it was written**, for the keys
    /// kept, when the call asked for backing. `None` when it did not.
    pub backing: Option<BTreeMap<String, Value>>,
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
    ///
    /// **A backing that could not be read is an error**, never an object with
    /// fewer claims named.
    pub fn of(
        answered: Answered,
        only_keys: Option<&[String]>,
        limit: usize,
    ) -> Result<RecalledView, MemoryError> {
        let held = answered.selected.objects.len();
        let mut backed = answered.backing;
        let kept = |key: &String| only_keys.is_none_or(|keep| keep.contains(key));
        let mut objects = Vec::new();
        for object in answered.selected.objects.into_iter().take(limit) {
            let backing = match backed.remove(&object.entity.id) {
                None => None,
                Some(read) => Some(
                    read?
                        .into_iter()
                        .filter(|(key, _)| kept(key))
                        .map(|(key, from)| {
                            let mut entry = json!({
                                "claim": from.fact.as_str(),
                                "provenance": from.provenance.as_token(),
                                "standing": from.standing.as_token(),
                            });
                            // **The instant of the write that holds this value**,
                            // not the object's newest write.
                            if let Some(at) = object.ages.as_ref().and_then(|a| a.keys.get(&key)) {
                                entry["written_at"] = json!(at.to_string());
                            }
                            (key, entry)
                        })
                        .collect(),
                ),
            };
            let mut fields = object.fields;
            fields.retain(|key, _| kept(key));
            objects.push(RecalledObject {
                handle: object.entity.id.to_string(),
                name: object.entity.name,
                kind: object.entity.kind.as_token().to_string(),
                fields,
                backing,
            });
        }
        Ok(RecalledView {
            left_out: held - objects.len(),
            objects,
        })
    }

    /// The view as the JSON an app is served.
    pub fn to_json(&self) -> Value {
        json!({
            "objects": self.objects.iter().map(|object| {
                let mut served = json!({
                    "handle": object.handle,
                    "name": object.name,
                    "kind": object.kind,
                    "fields": object.fields,
                });
                if let Some(backing) = &object.backing {
                    served["fields_backing"] = json!(backing);
                }
                served
            }).collect::<Vec<_>>(),
            "left_out": self.left_out,
        })
    }
}
