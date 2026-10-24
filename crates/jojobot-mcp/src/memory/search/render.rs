use super::*;

pub(super) fn is_an_active_claim(hit: &serde_json::Value) -> bool {
    hit["hit"] == "fact" && hit["status"] == "active"
}

pub(super) fn hit_json(
    hit: &Hit,
    as_of: jiff::civil::Date,
    details_asked: bool,
    snippet: Option<&str>,
) -> serde_json::Value {
    match hit {
        Hit::Entity {
            entity,
            edges,
            answers,
            ..
        } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("edges".into(), edges.iter().map(edge_json).collect());
                // **How this THING answers the type that was asked for.**
                // Absent when no type was asked for, rather than rendered
                // empty: a caller who named no type is not being told this
                // thing answers nothing.
                if let Some(found) = answers {
                    obj.insert("answers_type".into(), answers_json(found));
                }
            }
            body
        }
        Hit::Fact {
            fact,
            subject,
            home,
            source,
            superseded_by,
        } => {
            let mut body = wire::listed_json(fact, as_of, None, details_asked);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "fact".into());
                if !details_asked && let Some(snippet) = snippet {
                    obj.insert("content".into(), snippet.into());
                }
                // Marked, so the answer counts the hits it kept and says it once.
                if wire::left_out_of(std::iter::once(&**fact), details_asked) > 0 {
                    obj.insert(wire::OBJECT_LEFT_OUT.into(), 1.into());
                    obj.insert(
                        wire::OBJECT_LEFT_OUT_AT.into(),
                        vec![fact.address().to_string()].into(),
                    );
                }
                obj.insert("parent".into(), entity_ref_json(home));
                obj.remove("subject");
                if subject.id != home.id {
                    obj.insert("subject".into(), entity_ref_json(subject));
                }
                // **What became of the claim under this one.** Absent — not
                // rendered empty — for a claim derived from nothing: there is
                // no source to report on, and a null here would read as a
                // source nobody could place.
                if let Some(standing) = source {
                    obj.insert("source_standing".into(), standing.as_token().into());
                }
                // **Which claim took this one's place.** Absent — not null —
                // for every claim nobody replaced: an active one, one taken
                // back, one only archived.
                if let Some(address) = superseded_by {
                    obj.insert("superseded_by".into(), address.to_string().into());
                }
            }
            body
        }
        // A mail hit is unmistakably mail: the whole envelope, so a reader can
        // tell live work from an archived report without a second call, and the
        // id that takes delivery of the rest. `body` is deliberately absent —
        // what is here is the snippet, and read_message is how the message is
        // taken whole.
        Hit::Message { message, snippet } => serde_json::json!({
            "hit": "message",
            "id": message.id.as_str(),
            "mailbox": message.mailbox.as_str(),
            "state": message.state.as_token(),
            "sender": message.sender,
            "subject": message.subject,
            "sent_at": message.sent_at.to_string(),
            "notes": message.notes,
            "snippet": snippet,
        }),
        // A beat from the caller's own run. It carries the run's handle rather
        // than the whole chronology: what a reader does next is resume it or
        // read on, and both take the handle.
        Hit::Session {
            session,
            bot,
            working_on,
            snippet,
        } => serde_json::json!({
            "hit": "session",
            "session": session.0,
            "bot": bot.to_string(),
            "working_on": working_on,
            "snippet": snippet,
        }),
        Hit::Prose {
            title,
            entity,
            edges,
            snippet,
            ..
        } => serde_json::json!({
            "hit": "prose",
            "title": title,
            "entity": entity.as_ref().map(entity_json),
            "edges": edges.iter().map(edge_json).collect::<Vec<_>>(),
            "snippet": snippet,
        }),
    }
}

/// Search omits graph links unless asked, and drops data that adds no result context.
pub(super) fn trim_hit(hit: &mut serde_json::Value, include_links: bool) {
    if let Some(fields) = hit.as_object_mut() {
        fields.remove("inserted_at");
        if !include_links {
            for key in [
                "edge",
                "edges",
                "refs",
                "derived_from",
                "stands_for",
                "superseded_by",
            ] {
                fields.remove(key);
            }
        }
        if let Some(held) = fields
            .get_mut("fields")
            .and_then(serde_json::Value::as_object_mut)
        {
            for key in ["edges", "order", "level"] {
                held.remove(key);
            }
        }
        if fields.get("hit").and_then(serde_json::Value::as_str) == Some("prose")
            && fields.get("title") == fields.get("entity").and_then(|entity| entity.get("name"))
        {
            fields.remove("title");
        }
    }
    fn empty(value: &mut serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => {
                fields.retain(|_, value| !empty(value));
                fields.is_empty()
            }
            serde_json::Value::Array(items) => {
                items.retain_mut(|value| !empty(value));
                items.is_empty()
            }
            serde_json::Value::String(text) => text.is_empty(),
            serde_json::Value::Null => true,
            _ => false,
        }
    }
    empty(hit);
}
