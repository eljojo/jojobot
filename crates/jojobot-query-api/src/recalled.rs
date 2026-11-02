//! The typed answer of an app's read: the objects it may read, each as its
//! handle, name, kind and folded fields, and never the handler's text.

use std::collections::BTreeMap;

use jojobot_domain::memory::{EntityId, MemoryError, graph};
use serde_json::{Value, json};

use crate::{Answered, Verdict};

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
    /// **Whether the object can be started, and what stops it**, when the call
    /// asked what is ready. `None` when it did not. A call that names one card
    /// gets it back whatever the verdict, so this is what tells the two apart.
    pub verdict: Option<Verdict>,
}

/// **What an app's read served.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecalledView {
    /// The objects, in the order the selection gave them.
    pub objects: Vec<RecalledObject>,
    /// How many objects the page left out.
    pub left_out: usize,
    /// How many pieces of work the ready question left out: the ones that are
    /// not ready, which the list does not carry. Zero when the call did not ask.
    pub ready_left_out: usize,
    /// **The things the read could not read, each with why.** They are not in
    /// `objects`: the same rule the bots' recall uses took them out
    /// ([`graph::take_unreadable`]), so a damaged record is named and never silently absent.
    pub unreadable: Vec<(EntityId, MemoryError)>,
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
        let mut selected = answered.selected;
        let mut backed = answered.backing;
        // The verdicts are aligned with the objects the question kept, so each is
        // tied to its object before any object is taken out.
        let (verdicts, ready_left_out) = match answered.narrowed.ready {
            Some(pass) => (pass.verdicts, pass.left_out),
            None => (Vec::new(), 0),
        };
        let mut verdicts: BTreeMap<EntityId, Verdict> = selected
            .objects
            .iter()
            .map(|object| object.entity.id.clone())
            .zip(verdicts)
            .collect();
        let unreadable = graph::take_unreadable(&mut selected.objects, &selected.unreadable_scans);
        let held = selected.objects.len();
        let kept = |key: &String| only_keys.is_none_or(|keep| keep.contains(key));
        let mut objects = Vec::new();
        for object in selected.objects.into_iter().take(limit) {
            let backing = match backed.remove(&object.entity.id) {
                None => None,
                Some(read) => {
                    let mut entries = BTreeMap::new();
                    for (key, from) in read?.into_iter().filter(|(key, _)| kept(key)) {
                        let mut entry = json!({
                            "claim": from.fact.as_str(),
                            "provenance": from.provenance.as_token(),
                            "standing": from.standing.as_token(),
                        });
                        // **The instant of the write that holds this value**,
                        // not the object's newest write.
                        if let Some(at) = graph::key_written_at(&object, &key)? {
                            entry["written_at"] = json!(at.to_string());
                        }
                        entries.insert(key, entry);
                    }
                    Some(entries)
                }
            };
            let mut fields = object.fields;
            fields.retain(|key, _| kept(key));
            let verdict = verdicts.remove(&object.entity.id);
            objects.push(RecalledObject {
                handle: object.entity.id.to_string(),
                name: object.entity.name,
                kind: object.entity.kind.as_token().to_string(),
                fields,
                backing,
                verdict,
            });
        }
        Ok(RecalledView {
            left_out: held - objects.len(),
            objects,
            ready_left_out,
            unreadable,
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
