//! Accepted guesses stay outside the published schema's canonical vocabulary.

use super::*;

type Name = (&'static str, &'static str, &'static str);

pub(super) const NAMES: &[Name] = &[
    ("read_message", "id", "message_id"),
    ("mark_processed", "message", "message_id"),
    ("mark_processed", "ids", "message_ids"),
    ("mark_processed", "note", "notes"),
    ("recall", "handle", "subject"),
    ("read_document", "handle", "document"),
    ("read_document", "subject", "document"),
    ("add_entity", "slug", "handle"),
    ("set_charter", "charter", "prose"),
];

/// Conflicting names stay unknown, so validation refuses them before any verb
/// runs. Equal alias and canonical values collapse before fingerprinting: a
/// retry using only the canonical spelling is still the same request.
pub(crate) fn accept_names(request: &mut CallToolRequestParams) -> Vec<Name> {
    let mut accepted = Vec::new();
    let Some(arguments) = request.arguments.as_mut() else {
        return accepted;
    };
    for &(verb, sent, canonical) in NAMES {
        if verb == request.name
            && arguments
                .get(canonical)
                .is_none_or(|value| arguments.get(sent) == Some(value))
            && !NAMES.iter().any(|&(v, other, target)| {
                v == verb && target == canonical && other != sent && arguments.contains_key(other)
            })
            && let Some(value) = arguments.remove(sent)
        {
            arguments.insert(canonical.to_string(), value);
            accepted.push((verb, sent, canonical));
        }
    }
    accepted
}

pub(crate) fn teaching(verb: &str, canonical: &str) -> String {
    format!("Use `{canonical}` on {verb}.")
}

impl Jojobot {
    pub(crate) async fn teach_argument_names(
        &self,
        names: &[Name],
        answered: &mut CallToolResponse,
        sid: Option<&str>,
    ) {
        if names.is_empty() {
            return;
        }
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        for block in &mut result.content {
            let Some(text) = block.as_text() else {
                continue;
            };
            let Ok(mut body) = serde_json::from_str::<serde_json::Value>(&text.text) else {
                continue;
            };
            if !body.is_object() {
                continue;
            }
            let caller = self.caller(sid).ok().flatten();
            for &(verb, sent, canonical) in names {
                let domain = format!("argument-name:{verb}:{sent}");
                // Anonymous calls have no session ledger and keep their note.
                if caller.is_none() || self.first_contact(&domain, caller.as_ref()).await {
                    answer::note_teaching(&mut body, &teaching(verb, canonical));
                }
            }
            *block = ContentBlock::text(body.to_string());
            return;
        }
    }
}
