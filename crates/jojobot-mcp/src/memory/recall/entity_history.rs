//! **The history of one thing's own row, as the answer to `recall` renders it.**

use super::*;

/// **The versions of one thing's own row**, oldest first.
///
/// One version carries its place in the order, when it happened and the run that
/// made it (the run's public name, never a key), then each key the row held
/// after that write. A key the row held nothing for is left out, so a version
/// that never had a parent does not say `null`. A version kept before jojobot
/// kept them carries no keys, and the answer says what that means. When a thing
/// folded into this one wrote some of them, each version names that thing.
pub(super) fn entity_history_json(history: &graph::EntityHistory) -> serde_json::Value {
    let versions: Vec<serde_json::Value> = history
        .versions
        .iter()
        .map(|version| {
            let mut rendered = serde_json::json!({
                "nth": version.ordinal,
                "written_at": version.written_at.map(|at| at.to_string()),
            });
            if let Some(fields) = rendered.as_object_mut() {
                if let Some(run) = &version.session {
                    fields.insert("run".into(), run.as_str().into());
                }
                if history.folded_in {
                    fields.insert("thing".into(), version.thing.to_string().into());
                }
                if let Some(held) = &version.snapshot {
                    fields.insert("kind".into(), held.kind.as_str().into());
                    fields.insert("name".into(), held.name.as_str().into());
                    fields.insert("source".into(), held.source.as_str().into());
                    fields.insert("boot".into(), held.boot.as_str().into());
                    fields.insert("aliases".into(), held.aliases.clone().into());
                    fields.insert("prose".into(), held.prose.as_str().into());
                    for (key, value) in [
                        ("crm", held.crm.clone()),
                        ("parent", held.parent.as_ref().map(ToString::to_string)),
                        ("archived_reason", held.archived_reason.clone()),
                        ("archived_at", held.archived_at.clone()),
                        (
                            "merged_into",
                            held.merged_into.as_ref().map(ToString::to_string),
                        ),
                    ] {
                        if let Some(value) = value {
                            fields.insert(key.into(), value.into());
                        }
                    }
                }
            }
            rendered
        })
        .collect();
    let unkept = history
        .versions
        .iter()
        .filter(|version| version.snapshot.is_none())
        .count();
    let mut body = serde_json::json!({
        "entity": history.entity.to_string(),
        "count": history.total,
        "shown": history.versions.len(),
        "versions": versions,
    });
    if let Some(fields) = body.as_object_mut() {
        if unkept > 0 {
            fields.insert(
                "values_not_kept".into(),
                format!(
                    "{unkept} versions were written before jojobot kept what the thing held, so \
                     they say that a write happened and when, and the earlier values are gone"
                )
                .into(),
            );
        }
        if history.elided() > 0 {
            fields.insert(
                "older".into(),
                format!(
                    "{} older versions are not here — ask again with a bigger history_most to \
                     reach them",
                    history.elided()
                )
                .into(),
            );
        }
    }
    body
}
