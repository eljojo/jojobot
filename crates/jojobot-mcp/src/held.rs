//! **What a session holds, and telling it when that changed.**
//!
//! Every answer a session is handed names things: entities, claims, messages.
//! The ledger ([`crate::seen`]) keeps, per session, how each stood when it was
//! served. Every answer is checked here, once, at the shared hook
//! ([`Jojobot::finish_unshaped`]) and never per verb: each thing the session
//! holds is read as it stands now, and one that moved since, by a write the
//! session did not make itself, is named under `held_changed`.
//!
//! **A version is one value that moves when the thing is written** ([`Version`]):
//! an entity's is the moment the store took in its newest write about it, a
//! claim's is how many times it was written and its status, a message's is its
//! state. **What changed is read from the stores' own histories**, not kept in
//! the ledger: the claims written since, the row versions written since, each
//! with the session that made it. That is also how a session's own writes stay
//! silent.
//!
//! **A thing's prose and its own row move its version; a link another thing
//! holds to it does not**, because the pointer is a record of the other thing.
//!
//! **A thing the session holds, unchanged, comes back as its handle alone**
//! ([`collapse`]), and the answer says how many and how to have them whole
//! (`reship`). Nothing is stored: the ledger lives in the process and a restart
//! empties it (a missed notice, and everything shipped whole, never a wrong one).

use super::*;

mod collapse;
#[cfg(feature = "room-harness")]
mod room_harness;

use crate::seen::{Held, Version};
use jiff::Timestamp;
use jojobot_domain::mailbox::{Message, MessageEvent, MessageEventKind, MessageId};
use jojobot_domain::memory::FactAddress;
use serde_json::{Map, Value};

/// The key a notice rides under.
pub(crate) const HELD_CHANGED: &str = "held_changed";

/// An internal mark, consumed before the answer reaches the transport.
const SEARCH_ANSWER: &str = "jojobot/search-answer";
const RESULTS_ONLY: &str = "jojobot/search-results-only";
/// The caller asked for what it holds served whole (`reship: true`).
const RESHIP: &str = "jojobot/reship";

/// The answer is one the held pass may collapse: its verb publishes `reship`.
const COLLAPSIBLE: &str = "jojobot/collapsible";

#[cfg(any(test, feature = "testing"))]
const VERB: &str = "jojobot/collapse-verb";

/// **The verbs whose answers may collapse**: those that publish `reship`, so a
/// collapsed answer always has a call that returns it whole.
const OFFERS_RESHIP: [&str; 3] = ["recall", "search", "list_entities"];

/// Mark the answer of `verb` for the held pass: a verb that publishes `reship` is
/// collapsible, and one whose caller sent `reship: true` is served whole. The
/// dispatch reads the argument once for every verb.
pub(crate) fn mark_call(answered: &mut CallToolResponse, verb: &str, reship: bool) {
    let CallToolResponse::Complete(result) = answered else {
        return;
    };
    let meta = &mut result.meta.get_or_insert_with(Default::default).0;
    #[cfg(any(test, feature = "testing"))]
    meta.insert(VERB.into(), verb.into());
    if reship {
        meta.insert(RESHIP.into(), Value::Bool(true));
    }
    if OFFERS_RESHIP.contains(&verb) {
        meta.insert(COLLAPSIBLE.into(), Value::Bool(true));
    }
}

/// **Take the marks [`mark_call`] put on an answer off it**, for whichever way
/// the answer leaves. The held pass takes them in the usual path, and a compacted
/// write receipt skips that pass; this is the one place that leaves none on an
/// answer a client reads.
pub(crate) fn take_call_marks(answered: &mut CallToolResponse) {
    let CallToolResponse::Complete(result) = answered else {
        return;
    };
    let Some(meta) = result.meta.as_mut() else {
        return;
    };
    #[cfg(any(test, feature = "testing"))]
    meta.0.remove(VERB);
    meta.0.remove(RESHIP);
    meta.0.remove(COLLAPSIBLE);
    if meta.0.is_empty() {
        result.meta = None;
    }
}

/// Only successful token-saving searches carry this mark; refusals keep guidance.
pub(crate) fn token_saving_search(mut result: CallToolResult, enabled: bool) -> CallToolResult {
    if enabled {
        result
            .meta
            .get_or_insert_with(Default::default)
            .0
            .insert(RESULTS_ONLY.into(), true.into());
    }
    result
}

pub(crate) fn results_only(answered: &CallToolResponse) -> bool {
    matches!(answered, CallToolResponse::Complete(result)
        if result.meta.as_ref().is_some_and(|meta| meta.0.contains_key(RESULTS_ONLY)))
}

/// Refused searches defer unrelated news just as successful searches do.
pub(crate) fn search_answer(mut result: CallToolResult) -> CallToolResult {
    result
        .meta
        .get_or_insert_with(Default::default)
        .0
        .insert(SEARCH_ANSWER.into(), Value::Bool(true));
    result
}

fn take_search(result: &mut CallToolResult) -> (bool, bool, bool, bool) {
    let search = result
        .meta
        .as_mut()
        .and_then(|meta| meta.0.remove(SEARCH_ANSWER))
        .is_some();
    let results_only = result
        .meta
        .as_mut()
        .and_then(|meta| meta.0.remove(RESULTS_ONLY))
        .is_some();
    let reship = result
        .meta
        .as_mut()
        .and_then(|meta| meta.0.remove(RESHIP))
        .is_some();
    let collapsible = result
        .meta
        .as_mut()
        .and_then(|meta| meta.0.remove(COLLAPSIBLE))
        .is_some();
    if result.meta.as_ref().is_some_and(|meta| meta.0.is_empty()) {
        result.meta = None;
    }
    (search, results_only, reship, collapsible)
}

/// The state a quarantined message is told as.
const QUARANTINED: &str = "quarantined";

/// **What reading a held thing's history said.**
enum Changed {
    /// Another session wrote after the version the session holds.
    Told(Value),
    /// Every write after the held version was the session's own.
    Own,
    /// A history read failed, so nothing is known: the held version stays where
    /// it was, and the next answer reads again.
    Unread,
}

/// **The one rule for what is news** (rule 498): the writes after the version a
/// session holds, named by the session that made each, include one the session
/// did not make. Things, claims and messages all ask it, so none of them can
/// answer "my write was the last" for a write that came after another's.
fn another_wrote<'a>(sessions: impl IntoIterator<Item = Option<&'a str>>, me: &str) -> bool {
    sessions.into_iter().any(|session| session != Some(me))
}

/// **The events of a message after the one that made the state a session
/// holds.** A change of state is an event: delivery makes `read`, a retirement
/// makes `processed`, and a message that was never delivered has made none. The
/// events after the last one that made the held state are the writes since.
fn events_after<'a>(events: &'a [MessageEvent], held: &str) -> &'a [MessageEvent] {
    let made = match held {
        "read" => Some(MessageEventKind::Read),
        "processed" => Some(MessageEventKind::Processed),
        _ => None,
    };
    let start = made
        .and_then(|kind| events.iter().rposition(|event| event.kind == kind))
        .map_or(0, |at| at + 1);
    &events[start..]
}

/// **A read of a held thing's history failed**: said in the log, and the thing's
/// held version is not moved, so the next answer tries again.
fn unread(what: &Held, error: &dyn std::fmt::Display) {
    tracing::warn!(
        error = %error,
        held = ?what,
        "a held thing could not be read; its held version stays and the next answer reads again"
    );
}

/// **The claim notices that stand beside the entity notices an answer carries.**
/// A claim on an entity that is named is part of that notice only when the
/// notice lists it: the entity notice counts the claims written since the
/// entity's own held version, and an edit made before that version is not among
/// them. A claim the entity's notice leaves out is told on its own.
fn notices_to_send(told: &[(Held, Value)]) -> Vec<Value> {
    let listed: Vec<&str> = told
        .iter()
        .filter(|(what, _)| matches!(what, Held::Entity(_)))
        .flat_map(|(_, notice)| ["claims_new", "claims_edited"].map(|key| notice.get(key)))
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    told.iter()
        .filter(|(what, _)| match what {
            Held::Claim(address) => !listed.contains(&address.as_str()),
            _ => true,
        })
        .map(|(_, notice)| notice.clone())
        .collect()
}

/// **The address an object names itself by, when the object is a claim**: an
/// `address` with a `#`, beside the claim's words and provenance (a receipt of a
/// write names an address and carries neither).
pub(crate) fn claim_address_of(fields: &Map<String, Value>) -> Option<&str> {
    let text = |key: &str| fields.get(key).and_then(Value::as_str);
    let address = text("address")?;
    (address.contains('#') && text("content").is_some() && text("provenance").is_some())
        .then_some(address)
}

/// **Every entity, claim and message an answer names**, in the order the answer
/// has them. An entity is what [`crate::seen::entities_in`] reads. A claim is an
/// object with an `address` and the words and provenance of a claim (a receipt
/// of a write names an address and carries neither). A message is an object with
/// an `id`, a `mailbox` and a `state`.
pub(crate) fn served_in(body: &Value) -> Vec<Held> {
    fn walk(value: &Value, out: &mut Vec<Held>) {
        match value {
            Value::Object(fields) => {
                let text = |key: &str| fields.get(key).and_then(Value::as_str);
                if let Some(address) = claim_address_of(fields) {
                    out.push(Held::Claim(address.to_string()));
                }
                if let Some(id) = text("id")
                    && !id.contains(':')
                    && text("mailbox").is_some()
                    && text("state").is_some()
                {
                    out.push(Held::Message(id.to_string()));
                }
                for held in fields.values() {
                    walk(held, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out: Vec<Held> = crate::seen::entities_in(body)
        .into_iter()
        .map(|shown| Held::Entity(shown.handle))
        .collect();
    walk(body, &mut out);
    out
}

/// **Take the notice out of an answer**, so a receipt stored for a repeat holds
/// the answer to the write and nothing a later reader would take for news.
pub(crate) fn without_notice(result: &mut CallToolResult) {
    for block in &mut result.content {
        let Some(text) = block.as_text() else {
            continue;
        };
        let Ok(mut body) = serde_json::from_str::<Value>(&text.text) else {
            continue;
        };
        if body
            .as_object_mut()
            .and_then(|fields| fields.remove(HELD_CHANGED))
            .is_some()
        {
            *block = ContentBlock::text(body.to_string());
        }
    }
}

/// **The messages one answer's check reads**, by id, asked of the store once
/// ([`Jojobot::held_messages`]). Every version and every notice of a message in
/// that answer is taken from here, so no message is read twice.
#[derive(Default)]
pub(crate) struct HeldMessages(std::collections::HashMap<String, Message>);

impl Jojobot {
    /// Recall knows a claim's subject and home; an address alone only names its home.
    async fn search_news_scope(&self, results: &Value, served: &[Held]) -> Vec<Held> {
        let mut scope = served.to_vec();
        scope.extend(
            crate::seen::entities_in(results)
                .into_iter()
                .filter_map(|shown| shown.parent)
                .map(Held::Entity),
        );
        for what in scope.clone() {
            if let Held::Entity(handle) = what {
                match self.memory.recall(&EntityId(handle.clone())).await {
                    Ok(facts) => scope.extend(
                        facts
                            .into_iter()
                            .map(|fact| Held::Claim(fact.address().to_string())),
                    ),
                    Err(error) => unread(&Held::Entity(handle), &error),
                }
            }
        }
        scope
    }

    /// **The one place held messages are read** (rule 498): the messages among
    /// `wanted`, in a single query however many there are, and no query at all
    /// when none is a message. A store that cannot answer reads as no message
    /// found, and a version nobody can read is no change.
    async fn held_messages<'a>(&self, wanted: impl Iterator<Item = &'a Held>) -> HeldMessages {
        let mut ids: Vec<MessageId> = wanted
            .filter_map(|what| match what {
                Held::Message(id) => Some(MessageId(id.clone())),
                _ => None,
            })
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return HeldMessages::default();
        }
        match self.mailboxes.messages_by_id(&ids).await {
            Ok(found) => HeldMessages(
                found
                    .into_iter()
                    .map(|message| (message.id.0.clone(), message))
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!(error = %e, "held messages could not be read; their held versions stay");
                HeldMessages::default()
            }
        }
    }

    /// **The newest write about anything this entity's served text is worked out
    /// from**, which its own writes never count (rule 498: this is the one place
    /// that says what a part derives from).
    ///
    /// A field that names another thing is served under that thing's handle; a
    /// list reads the state of the work its entries link and where each entry was
    /// carried; the held block reads the records that point at the thing. A source
    /// that cannot be read adds nothing, as an unreadable version is no change.
    /// A plan reads the day and a walk reads every claim that names the thing,
    /// which no version follows, so [`collapse`] never withholds them.
    async fn around(&self, handle: &str) -> Option<Timestamp> {
        let id = EntityId(handle.to_string());
        let mut sources: std::collections::BTreeSet<EntityId> = Default::default();
        let fields = self.memory.fields(&id).await.ok()?;
        for value in fields.values() {
            sources.extend(jojobot_domain::memory::handles_in_value(value));
        }
        if let Ok(facts) = self.memory.referring_to(&id).await {
            sources.extend(facts.into_iter().map(|fact| fact.home));
        }
        if id.kind() == Some(jojobot_domain::memory::EntityKind::LIST)
            && let Ok(held) = self.memory.recall(&id).await
        {
            let setting = jojobot_domain::memory::entries::Setting::of(&fields);
            let linked = jojobot_domain::memory::entries::counted_links(&held, setting);
            // **Readiness reads what the linked work depends on, and progress
            // reads the status words the instance maps.**
            for work in &linked {
                if let Ok(theirs) = self.memory.fields(work).await {
                    for value in theirs.values() {
                        sources.extend(jojobot_domain::memory::handles_in_value(value));
                    }
                }
            }
            if !linked.is_empty() {
                sources.insert(EntityId(
                    jojobot_domain::memory::INSTANCE_RECORD.to_string(),
                ));
            }
            sources.extend(linked);
            for entry in &jojobot_domain::memory::entries::view(&held, setting).entries {
                if let Ok(built) = self.memory.built_on(&entry.fact.address()).await {
                    sources.extend(built.into_iter().map(|fact| fact.home));
                }
            }
        }
        sources.remove(&id);
        let mut newest = None;
        for source in sources {
            if let Ok(ages) = self.memory.field_ages(&source).await {
                newest = newest.max(ages.newest);
            }
        }
        newest
    }

    /// **How a held thing stands now**, or `None` when it cannot be read (a
    /// thing that is gone, a store that cannot answer): a version nobody can read
    /// is no change. A message stands as `messages` read it.
    pub(crate) async fn version_of(&self, what: &Held, messages: &HeldMessages) -> Option<Version> {
        match what {
            Held::Entity(handle) => {
                let ages = self
                    .memory
                    .field_ages(&EntityId(handle.clone()))
                    .await
                    .inspect_err(|e| unread(what, e))
                    .ok()?;
                Some(Version::Entity {
                    own: ages.newest,
                    around: self.around(handle).await,
                })
            }
            Held::Claim(address) => {
                let chain = self
                    .memory
                    .claim_history(&FactAddress::parse(address).ok()?)
                    .await
                    .inspect_err(|e| unread(what, e))
                    .ok()?;
                let last = chain.last()?;
                Some(Version::Claim {
                    writes: chain.len(),
                    status: format!("{:?}", last.status).to_lowercase(),
                })
            }
            Held::Message(id) => {
                let message = messages.0.get(id)?;
                Some(Version::Message(
                    format!("{:?}", message.state).to_lowercase(),
                ))
            }
        }
    }

    /// **Add `held_changed` to an answer, and remember what it served.**
    ///
    /// Each eligible thing the session holds that this answer does not serve again is
    /// read as it stands. One whose version moved is told when another session
    /// made the write; a move the session made itself is taken in silently.
    /// Search defers unrelated things without advancing their held versions.
    /// Whatever this answer serves is then held at the version it has now.
    pub(crate) async fn add_held_changed(
        &self,
        answered: &mut CallToolResponse,
        sid: Option<&str>,
    ) {
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        #[cfg(any(test, feature = "testing"))]
        let verb = result
            .meta
            .as_mut()
            .and_then(|meta| meta.0.remove(VERB))
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".into());
        let (search, results_only, reship, collapsible) = take_search(result);
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let me = caller.sid.as_str().to_string();
        let Some(at) = result
            .content
            .iter()
            .position(|block| block.as_text().is_some())
        else {
            return;
        };
        let Some(text) = result.content[at].as_text().map(|text| text.text.clone()) else {
            return;
        };
        let Ok(mut body) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        if !body.is_object() {
            return;
        }
        let served = served_in(if search { &body["results"] } else { &body });
        let shapes_before_collapse = collapse::shapes_in(&body);
        #[cfg(any(test, feature = "testing"))]
        let values_before_collapse = collapse::values_in(&body);
        let scope = if search {
            self.search_news_scope(&body["results"], &served).await
        } else {
            Vec::new()
        };
        let held_before = self.registry.held_by(&me);
        let held: Vec<_> = held_before
            .iter()
            .filter(|(what, _)| !results_only && (!search || scope.contains(what)))
            .cloned()
            .collect();
        let messages = self
            .held_messages(held.iter().map(|(what, _)| what).chain(served.iter()))
            .await;
        let mut told: Vec<(Held, Value)> = Vec::new();
        let mut moved: Vec<(Held, Version)> = Vec::new();
        for (what, was) in &held {
            if served.contains(what) {
                continue;
            }
            let Some(now) = self.version_of(what, &messages).await else {
                // **A held message no card answers for may have been quarantined**:
                // its history still answers, and the session must stop believing
                // it is owed.
                match self.quarantine_news(what, was, &me).await {
                    Some(Some(notice)) => {
                        told.push((what.clone(), notice));
                        moved.push((what.clone(), Version::Message(QUARANTINED.into())));
                    }
                    Some(None) => moved.push((what.clone(), Version::Message(QUARANTINED.into()))),
                    None => {}
                }
                continue;
            };
            if now == *was {
                continue;
            }
            match self.what_changed(what, was, &now, &me).await {
                Changed::Told(notice) => {
                    told.push((what.clone(), notice));
                    moved.push((what.clone(), now));
                }
                Changed::Own => moved.push((what.clone(), now)),
                // The version stays, so the next answer reads again.
                Changed::Unread => {}
            }
        }
        let mut served_now: Vec<(Held, Version)> = Vec::new();
        for what in served {
            if let Some(version) = self.version_of(&what, &messages).await {
                served_now.push((what, version));
            }
        }
        // **Decided before the answer is held in turn**: what the session held
        // before this call, at the version it stands at now, is not shipped again.
        let held_shapes = self.registry.shapes_by(&me);
        #[cfg(any(test, feature = "testing"))]
        let held_values = self.registry.values_by(&me);
        let collapsed = if results_only || reship || !collapsible {
            0
        } else {
            let unchanged = collapse::unchanged(&held_before, &served_now);
            #[cfg(any(test, feature = "testing"))]
            collapse::assert_received(&body, &unchanged, &held_shapes, &held_values, &verb);
            collapse::collapse(&mut body, &unchanged, &held_shapes)
        };
        let received = collapse::received(
            &held_before,
            &held_shapes,
            &served_now,
            shapes_before_collapse,
        );
        // **A held thing that moved without being served has new text the session
        // was not handed**, whether another session or this one wrote it, so what
        // it had received of it starts again.
        let received: Vec<_> = moved
            .iter()
            .map(|(what, _)| (what.clone(), crate::seen::Shape::new()))
            .chain(received)
            .collect();
        #[cfg(any(test, feature = "testing"))]
        let values = moved
            .iter()
            .map(|(what, _)| (what.clone(), crate::seen::PartValues::new()))
            .chain(collapse::received_values(
                &collapse::unchanged(&held_before, &served_now),
                &held_values,
                values_before_collapse,
            ))
            .collect();
        moved.extend(served_now);
        self.registry.note_held(&me, moved);
        self.registry.note_shapes(&me, received);
        #[cfg(any(test, feature = "testing"))]
        self.registry.note_values(&me, values);
        let notices = notices_to_send(&told);
        if notices.is_empty() && collapsed == 0 {
            return;
        }
        if !notices.is_empty() {
            body[HELD_CHANGED] = Value::Array(notices);
        }
        result.content[at] = ContentBlock::text(body.to_string());
    }

    /// **The notice for a held message that has been quarantined since it was
    /// served**: `Some(Some(notice))` when another session did it, `Some(None)`
    /// when the session did it itself, `None` when no quarantine is known.
    /// A quarantined card is not readable, so the notice names the state and
    /// nothing the message said. A message already told as quarantined is not
    /// read again, and a history that cannot be read leaves the held version
    /// where it was, so the next answer asks again.
    async fn quarantine_news(&self, what: &Held, was: &Version, me: &str) -> Option<Option<Value>> {
        let (Held::Message(id), Version::Message(held)) = (what, was) else {
            return None;
        };
        if held == QUARANTINED {
            return None;
        }
        let mut events = self
            .mailboxes
            .message_history(&MessageId(id.clone()))
            .await
            .inspect_err(|e| unread(what, e))
            .ok()?;
        events.sort_by_key(|event| event.ordinal);
        let since = events_after(&events, held);
        let mut quarantines = since
            .iter()
            .filter(|event| event.kind == MessageEventKind::Quarantined)
            .peekable();
        quarantines.peek()?;
        let news = quarantines
            .any(|event| event.by_session.as_deref() != Some(me))
            .then(|| serde_json::json!({"message": id, "state": QUARANTINED}));
        Some(news)
    }

    /// **What another session did to a held thing since it was served.**
    async fn what_changed(&self, what: &Held, was: &Version, now: &Version, me: &str) -> Changed {
        match (what, was, now) {
            (Held::Entity(handle), Version::Entity { own: since, .. }, _) => {
                self.entity_changed(handle, *since, me).await
            }
            (Held::Claim(address), Version::Claim { writes, .. }, _) => {
                let Ok(parsed) = FactAddress::parse(address) else {
                    return Changed::Own;
                };
                let chain = match self.memory.claim_history(&parsed).await {
                    Ok(chain) => chain,
                    Err(e) => {
                        unread(what, &e);
                        return Changed::Unread;
                    }
                };
                let since = chain.iter().skip(*writes);
                if !another_wrote(since.map(|write| write.session.as_deref()), me) {
                    return Changed::Own;
                }
                let Some(last) = chain.last() else {
                    return Changed::Own;
                };
                Changed::Told(serde_json::json!({
                    "address": address,
                    "claim": if format!("{:?}", last.status).to_lowercase() == "active" {
                        "edited"
                    } else {
                        "archived"
                    },
                }))
            }
            (Held::Message(id), Version::Message(held), Version::Message(state)) => {
                let mut events = match self.mailboxes.message_history(&MessageId(id.clone())).await
                {
                    Ok(events) => events,
                    Err(e) => {
                        unread(what, &e);
                        return Changed::Unread;
                    }
                };
                events.sort_by_key(|event| event.ordinal);
                let since = events_after(&events, held);
                if !another_wrote(since.iter().map(|event| event.by_session.as_deref()), me) {
                    return Changed::Own;
                }
                Changed::Told(serde_json::json!({
                    "message": id,
                    "state": state,
                }))
            }
            _ => Changed::Own,
        }
    }

    /// The writes other sessions made to an entity since `since`.
    async fn entity_changed(&self, handle: &str, since: Option<Timestamp>, me: &str) -> Changed {
        let id = EntityId(handle.to_string());
        let after = |at: Option<Timestamp>| match (at, since) {
            (Some(at), Some(since)) => at > since,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let mut notice = Map::new();
        notice.insert("handle".into(), handle.into());

        let histories = match self.memory.claim_histories(&id).await {
            Ok(histories) => histories,
            Err(e) => {
                unread(&Held::Entity(handle.to_string()), &e);
                return Changed::Unread;
            }
        };
        let (mut new, mut edited): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for (fact, writes) in &histories {
            let theirs = another_wrote(
                writes
                    .iter()
                    .filter(|write| after(write.written_at))
                    .map(|write| write.session.as_deref()),
                me,
            );
            if !theirs {
                continue;
            }
            let address = format!("{handle}#{}", fact.0);
            if writes.first().is_some_and(|first| after(first.written_at)) {
                new.push(address);
            } else {
                edited.push(address);
            }
        }
        new.sort();
        edited.sort();
        let claims_moved = !new.is_empty() || !edited.is_empty();
        if !new.is_empty() {
            notice.insert("claims_new".into(), new.into());
        }
        if !edited.is_empty() {
            notice.insert("claims_edited".into(), edited.into());
        }
        if claims_moved && let Ok(ages) = self.memory.field_ages(&id).await {
            let keys: Vec<&String> = ages
                .keys
                .iter()
                .filter(|(_, at)| since.is_none_or(|since| **at > since))
                .map(|(key, _)| key)
                .collect();
            if !keys.is_empty() {
                notice.insert("fields".into(), serde_json::json!(keys));
            }
        }

        if let Ok(mut versions) = self.memory.entity_history(&id).await {
            versions.sort_by_key(|version| version.ordinal);
            let mut before = None;
            for version in &versions {
                if let (Some(now), true) = (
                    &version.snapshot,
                    after(version.written_at)
                        && another_wrote(std::iter::once(version.session.as_deref()), me),
                ) && let Some(Some(was)) = before.as_ref().map(|b: &Option<_>| b.as_ref())
                {
                    let was: &jojobot_domain::memory::EntitySnapshot = was;
                    if now.name != was.name {
                        notice.insert("name".into(), now.name.clone().into());
                    }
                    if now.archived_at.is_some() != was.archived_at.is_some() {
                        notice.insert(
                            if now.archived_at.is_some() {
                                "archived"
                            } else {
                                "restored"
                            }
                            .into(),
                            true.into(),
                        );
                    }
                    if now.prose != was.prose {
                        notice.insert("page".into(), true.into());
                    }
                    if now.parent != was.parent {
                        notice.insert("moved".into(), true.into());
                    }
                    if now.aliases != was.aliases {
                        notice.insert("aliases".into(), true.into());
                    }
                }
                before = Some(version.snapshot.clone());
            }
        }

        if notice.len() > 1 {
            Changed::Told(Value::Object(notice))
        } else {
            Changed::Own
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    #[tokio::test]
    async fn collapse_guard_accepts_both_received_forms_of_each_graph_root() {
        async fn read(jojobot: &Jojobot, sid: &str) -> CallToolResponse {
            jojobot
                .recall(rmcp::handler::server::wrapper::Parameters(
                    serde_json::from_value(serde_json::json!({
                        "kind":"person", "facts":false, "sid":sid,
                        "follow":{"shape":"connection", "direction":"out", "depth":1}
                    }))
                    .expect("recall arguments"),
                ))
                .await
                .expect("the graph read answers")
                .into()
        }

        let jojobot = handler();
        let sid = crate::mailboxes::testing::owning(&jojobot, "gamma").await;
        for (subject, object, content) in [
            ("person:homer", "person:marge", "saw the neighbor"),
            ("person:marge", "person:homer", "met at the assembly"),
        ] {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    sid: Some(sid.clone()),
                    shape: Some("connection".into()),
                    object: Some(object.into()),
                    ..capture_args(subject, content)
                },
            )
            .await;
        }

        let mut first = read(&jojobot, &sid).await;
        let CallToolResponse::Complete(result) = &first else {
            panic!("the graph read completes");
        };
        let body = json_of(result);
        let roots = body["objects"].as_array().expect("the real graph roots");
        assert_eq!(roots.len(), 2, "{body}");
        for root in roots {
            let children = root["connected"].as_array().expect("the root's walk");
            assert_eq!(children.len(), 1, "{root}");
            assert_ne!(root["id"], children[0]["id"]);
            assert_eq!(children[0]["connected"], serde_json::json!([]));
        }
        mark_call(&mut first, "recall", false);
        jojobot.add_held_changed(&mut first, Some(&sid)).await;

        // Each person was received both as a root with a neighbor and as a
        // depth-limited child. Repeating that read withholds no unseen form.
        let mut again = read(&jojobot, &sid).await;
        mark_call(&mut again, "recall", false);
        jojobot.add_held_changed(&mut again, Some(&sid)).await;
        let CallToolResponse::Complete(result) = again else {
            panic!("the repeated graph read completes");
        };
        // A walk that found a neighbor reads every claim that names the thing,
        // which no version follows, so a person carrying one is served whole.
        assert!(
            json_of(&result).get(collapse::COLLAPSED).is_none(),
            "{}",
            json_of(&result)
        );
    }

    #[tokio::test]
    async fn collapse_guard_catches_a_changed_part_at_an_unchanged_version() {
        fn answer(prose: &str) -> CallToolResponse {
            let mut answered =
                CallToolResponse::from(CallToolResult::success(vec![ContentBlock::text(
                    serde_json::json!({"objects":[{
                        "id":"person:homer", "name":"Homer", "prose":prose
                    }]})
                    .to_string(),
                )]));
            mark_call(&mut answered, "recall", false);
            answered
        }

        let jojobot = handler();
        let sid = crate::mailboxes::testing::owning(&jojobot, "gamma").await;
        ensure(&jojobot, "homer").await;
        let mut first = answer("received page");
        jojobot.add_held_changed(&mut first, Some(&sid)).await;
        let mut same = answer("received page");
        jojobot.add_held_changed(&mut same, Some(&sid)).await;
        let CallToolResponse::Complete(same) = same else {
            panic!("the unchanged control completes");
        };
        assert_eq!(json_of(&same)[collapse::COLLAPSED]["count"], 1);

        // A missed stamp or a derived read can change a part while its owner's
        // store version stays put. The guard must catch the text it would hide.
        let caught = tokio::spawn(async move {
            let mut changed = answer("unreceived page");
            jojobot.add_held_changed(&mut changed, Some(&sid)).await;
        })
        .await
        .expect_err("the guard must catch unseen prose at the same version");
        assert!(caught.is_panic());
        let payload = caught.into_panic();
        let diagnostic = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        for identifier in ["person:homer", "prose", "recall"] {
            assert!(diagnostic.contains(identifier), "{diagnostic}");
        }
    }

    /// **An answer reads the messages a session holds with one query to the
    /// store, however many it holds.** The check rides on every answer, so a
    /// query for each held message is a query for each message on every call. The
    /// positives that make the count mean something: what the answer served is
    /// held (a later change to the four is named, all four), and the change is
    /// found from the same one query.
    #[tokio::test]
    async fn an_answer_reads_the_held_messages_in_one_query_not_one_each() {
        use crate::mailboxes::testing::{counting_handler, owning, send};
        let (jojobot, board) = counting_handler();
        let otto = owning(&jojobot, "otto").await;
        owning(&jojobot, "gamma").await;
        let mut ids = Vec::new();
        for n in 0..4 {
            let sent = send(&jojobot, "otto", "gamma", &format!("errand {n}")).await;
            ids.push(sent["id"].as_str().expect("an id").to_string());
        }
        let answer = |body: Value| -> CallToolResponse {
            CallToolResult::success(vec![ContentBlock::text(body.to_string())]).into()
        };
        let body_of = |answered: &CallToolResponse| -> Value {
            let CallToolResponse::Complete(result) = answered else {
                panic!("a completed call");
            };
            let text = result.content[0]
                .as_text()
                .expect("a text block")
                .text
                .clone();
            serde_json::from_str(&text).expect("the answer is JSON")
        };

        // ── serving four messages reads them in one query ───────────────────
        let mut served = answer(serde_json::json!({
            "messages": ids.iter()
                .map(|id| serde_json::json!({"id": id, "mailbox": "otto", "state": "new"}))
                .collect::<Vec<_>>(),
        }));
        let before = board.message_queries();
        jojobot.add_held_changed(&mut served, Some(&otto)).await;
        assert_eq!(
            board.message_queries() - before,
            1,
            "four served messages are read with one query"
        );

        // ── another run of the box takes delivery, so the four have moved ───
        let other = crate::harness::as_bot(&jojobot, "otto");
        jojobot
            .read_mailbox(rmcp::handler::server::wrapper::Parameters(
                crate::mailboxes::ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(other),
                },
            ))
            .await
            .expect("the other run takes delivery");

        // ── an answer that serves none of them checks the four in one query,
        //    and finds that all four moved ───────────────────────────────────
        let mut quiet = answer(serde_json::json!({"status": "ok"}));
        let before = board.message_queries();
        jojobot.add_held_changed(&mut quiet, Some(&otto)).await;
        assert_eq!(
            board.message_queries() - before,
            1,
            "four held messages are checked with one query"
        );
        let told = body_of(&quiet);
        assert_eq!(
            told["held_changed"].as_array().map(Vec::len),
            Some(4),
            "all four are named as moved: {told}"
        );
    }

    /// The moment of the newest write about a thing, as a version holds it.
    async fn newest(jojobot: &Jojobot, handle: &str) -> Option<Timestamp> {
        match jojobot
            .version_of(&Held::Entity(handle.into()), &HeldMessages::default())
            .await
        {
            Some(Version::Entity { own, .. }) => own,
            other => panic!("an entity has an entity version: {other:?}"),
        }
    }

    /// **A thing's prose moves its version, and a link another thing holds to it
    /// does not.** The page is part of the thing's own row, so a write to it is a
    /// write about the thing. A claim on another thing that points at it is a
    /// record of that other thing, and the pointed-at thing was not written.
    #[tokio::test]
    async fn a_things_prose_moves_its_version_and_a_link_held_on_another_thing_does_not() {
        let jojobot = handler();
        ensure(&jojobot, "person:homer").await;
        ensure(&jojobot, "person:lisa").await;
        capture_ok(&jojobot, capture_args("person:homer", "likes donuts")).await;
        let before = newest(&jojobot, "person:homer").await;
        assert!(before.is_some(), "a written thing has a version");

        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("about".into()),
                object: Some("person:homer".into()),
                ..capture_args("person:lisa", "is proud of her father")
            },
        )
        .await;
        assert_eq!(
            newest(&jojobot, "person:homer").await,
            before,
            "a link held on another thing does not move the version of the thing it names"
        );

        jojobot
            .memory
            .set_prose(
                &EntityId::person("person:homer"),
                "A page about Homer.",
                None,
            )
            .await
            .expect("the page is set");
        assert!(
            newest(&jojobot, "person:homer").await > before,
            "a write to the page moves the version"
        );
    }

    /// How a held thing stands now, as the held pass reads it.
    async fn version(jojobot: &Jojobot, handle: &str) -> Version {
        jojobot
            .version_of(&Held::Entity(handle.into()), &HeldMessages::default())
            .await
            .expect("a written thing has a version")
    }

    /// **A thing's version moves when the things its text is worked out from
    /// move**, and holds when something it does not derive from does. One row per
    /// way a read derives text: a field that names another thing follows that
    /// thing's rename; a list's progress reads the work its entries link; the held
    /// block reads the records that point at the thing.
    #[tokio::test]
    async fn a_things_version_moves_when_what_its_text_derives_from_moves() {
        let jojobot = handler();
        for handle in [
            "thing:omega",
            "thing:alpha",
            "list:checklist",
            "work:sigma",
            "person:bart",
            "person:lisa",
        ] {
            ensure(&jojobot, handle).await;
        }
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("ruled_by".to_string(), "thing:alpha".to_string())].into()),
                ..capture_args("thing:omega", "is ruled by alpha")
            },
        )
        .await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("about".into()),
                object: Some("work:sigma".into()),
                ..capture_args("list:checklist", "finish sigma")
            },
        )
        .await;
        capture_ok(&jojobot, capture_args("work:sigma", "has begun")).await;
        let (omega, list) = (
            version(&jojobot, "thing:omega").await,
            version(&jojobot, "list:checklist").await,
        );

        capture_ok(&jojobot, capture_args("person:bart", "skates")).await;
        assert_eq!(
            version(&jojobot, "thing:omega").await,
            omega,
            "a write to a thing it does not name moves nothing"
        );
        assert_eq!(version(&jojobot, "list:checklist").await, list);

        jojobot
            .memory
            .rename_entity(
                &EntityId::person("thing:alpha"),
                &EntityId::person("thing:beta"),
                None,
                jiff::civil::date(2026, 9, 1),
                jojobot_domain::memory::Override::none(),
                None,
            )
            .await
            .expect("the rename lands");
        assert_ne!(
            version(&jojobot, "thing:omega").await,
            omega,
            "a field that names a renamed thing is served under its new handle"
        );

        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("status".to_string(), "done".to_string())].into()),
                ..capture_args("work:sigma", "is finished")
            },
        )
        .await;
        assert_ne!(
            version(&jojobot, "list:checklist").await,
            list,
            "a list's progress reads the state of the work its entries link"
        );

        // Readiness reads the linked work's own dependencies, and progress reads
        // the status words the instance maps: a write to either, and to neither
        // the list nor the work it links, still changes what the list says.
        ensure(&jojobot, "work:phi").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("depends_on".to_string(), "work:phi".to_string())].into()),
                ..capture_args("work:sigma", "waits for the slab")
            },
        )
        .await;
        let list = version(&jojobot, "list:checklist").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("status".to_string(), "done".to_string())].into()),
                ..capture_args("work:phi", "is poured")
            },
        )
        .await;
        assert_ne!(
            version(&jojobot, "list:checklist").await,
            list,
            "a list's readiness reads what the linked work depends on"
        );
        let list = version(&jojobot, "list:checklist").await;
        capture_ok(
            &jojobot,
            capture_args(
                jojobot_domain::memory::INSTANCE_RECORD,
                "maps a status word",
            ),
        )
        .await;
        assert_ne!(
            version(&jojobot, "list:checklist").await,
            list,
            "a list's progress reads the status words the instance maps"
        );

        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("looks_after".to_string(), "person:lisa".to_string())].into()),
                ..capture_args("person:bart", "looks after lisa")
            },
        )
        .await;
        let lisa = version(&jojobot, "person:lisa").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("mood".to_string(), "proud".to_string())].into()),
                ..capture_args("person:bart", "is proud")
            },
        )
        .await;
        assert_ne!(
            version(&jojobot, "person:lisa").await,
            lisa,
            "the held block reads the records that point at the thing"
        );
    }

    /// **A history read that fails once does not lose the notice.** Otto holds a
    /// claim; another session edits it; the read of the claim's history that
    /// would tell Otto fails once. That answer says nothing, and Otto's held
    /// version stays where it was, so the next answer reads again and names the
    /// edit. The counter says the failure happened, so the case cannot pass on a
    /// read that never failed.
    #[tokio::test]
    async fn a_history_read_that_fails_once_keeps_the_held_version_for_the_next_answer() {
        use rmcp::handler::server::wrapper::Parameters;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let (healthy, blind) = healthy_and_down(Down::ClaimHistoryFailsAt(counter.clone()));
        ensure(&healthy, "person:homer").await;
        make_bot(&healthy, "otto").await;
        make_bot(&healthy, "gamma").await;
        let claim =
            address_of(&capture_ok(&healthy, capture_args("person:homer", "likes donuts")).await);
        let otto = as_bot(&blind, "otto");
        let other = as_bot(&healthy, "gamma");
        let answer = |body: Value| -> CallToolResponse {
            CallToolResult::success(vec![ContentBlock::text(body.to_string())]).into()
        };
        let body_of = |answered: &CallToolResponse| -> Value {
            let CallToolResponse::Complete(result) = answered else {
                panic!("a completed call");
            };
            serde_json::from_str(&result.content[0].as_text().expect("text").text)
                .expect("the answer is JSON")
        };

        // ── Otto is served the claim, so he holds it as written once ─────────
        let mut served = answer(serde_json::json!({
            "claims": [{"address": claim, "content": "likes donuts", "provenance": "inference"}],
        }));
        blind.add_held_changed(&mut served, Some(&otto)).await;

        // ── another session edits it ─────────────────────────────────────────
        healthy
            .update_fact(Parameters(UpdateFactArgs {
                sid: Some(other),
                content: Some("likes donuts and beer".into()),
                provenance: Some("inference".into()),
                ..update_args(&claim)
            }))
            .await
            .expect("the other session edits the claim");

        // ── the second history read of the next answer fails, the first does not
        counter.store(2, Ordering::SeqCst);
        let mut first = answer(serde_json::json!({"status": "ok"}));
        blind.add_held_changed(&mut first, Some(&otto)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "the read failed");
        assert!(
            body_of(&first).get(HELD_CHANGED).is_none(),
            "a failed read tells nothing: {first:?}"
        );

        // ── the next answer reads again and names the edit ───────────────────
        let mut next = answer(serde_json::json!({"status": "ok"}));
        blind.add_held_changed(&mut next, Some(&otto)).await;
        let told = body_of(&next);
        assert_eq!(
            told[HELD_CHANGED][0]["address"], claim,
            "the edit is told on the answer after the failed read: {told}"
        );
        assert_eq!(told[HELD_CHANGED][0]["claim"], "edited", "{told}");
    }

    /// **The marks `mark_call` puts on an answer are off it when it leaves, on
    /// the compact write-receipt path too.** Lean mode compacts a write receipt
    /// and skips the held pass that used to take the marks off, so the verb mark
    /// of a testing build stayed on the answer a client reads.
    #[tokio::test]
    async fn a_compacted_write_receipt_carries_no_collapse_mark() {
        let jojobot = handler();
        let mut receipt = answered(serde_json::json!({"id": "person:homer", "status": "ok"}));
        mark_call(&mut receipt, "capture", false);
        crate::write_receipts::TOKEN_SAVING
            .scope(
                Some(crate::settings::TokenSaving::on()),
                jojobot.finish_unshaped(&mut receipt, None),
            )
            .await;
        let CallToolResponse::Complete(result) = &receipt else {
            panic!("a completed call");
        };
        assert_eq!(
            body_in(&receipt),
            serde_json::json!({"id": "person:homer"}),
            "the receipt was compacted"
        );
        assert!(
            result
                .meta
                .as_ref()
                .is_none_or(|meta| !meta.0.keys().any(|key| key.starts_with("jojobot/"))),
            "no jojobot mark is left on the answer: {:?}",
            result.meta
        );
    }

    fn answered(body: Value) -> CallToolResponse {
        CallToolResult::success(vec![ContentBlock::text(body.to_string())]).into()
    }

    fn body_in(answered: &CallToolResponse) -> Value {
        let CallToolResponse::Complete(result) = answered else {
            panic!("a completed call");
        };
        serde_json::from_str(&result.content[0].as_text().expect("text").text)
            .expect("the answer is JSON")
    }

    /// **An entity's history read that fails once does not lose the notice.**
    /// Otto holds Homer; another session adds a claim; the read of Homer's
    /// claims that would tell Otto fails once. That answer says nothing, and the
    /// next answer, with the read healthy, names the new claim. The entity path
    /// reads claim histories through the claim-history read, so the counter that
    /// fails it says the failure happened.
    #[tokio::test]
    async fn an_entity_history_read_that_fails_once_keeps_the_held_version_for_the_next_answer() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let (healthy, blind) = healthy_and_down(Down::ClaimHistoryFailsAt(counter.clone()));
        ensure(&healthy, "person:homer").await;
        make_bot(&healthy, "otto").await;
        make_bot(&healthy, "gamma").await;
        capture_ok(&healthy, capture_args("person:homer", "likes donuts")).await;
        let otto = as_bot(&blind, "otto");
        let other = as_bot(&healthy, "gamma");

        let mut served = answered(serde_json::json!({
            "objects": [{"id": "person:homer", "name": "Homer"}],
        }));
        blind.add_held_changed(&mut served, Some(&otto)).await;

        let added = capture_ok(
            &healthy,
            CaptureArgs {
                sid: Some(other),
                ..capture_args("person:homer", "likes beer")
            },
        )
        .await;
        let new_claim = address_of(&added);

        counter.store(1, Ordering::SeqCst);
        let mut failed = answered(serde_json::json!({"status": "ok"}));
        blind.add_held_changed(&mut failed, Some(&otto)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "the read failed");
        assert!(
            body_in(&failed).get(HELD_CHANGED).is_none(),
            "a failed read tells nothing: {failed:?}"
        );

        let mut next = answered(serde_json::json!({"status": "ok"}));
        blind.add_held_changed(&mut next, Some(&otto)).await;
        let told = body_in(&next);
        assert_eq!(told[HELD_CHANGED][0]["handle"], "person:homer", "{told}");
        assert_eq!(
            told[HELD_CHANGED][0]["claims_new"][0], new_claim,
            "the claim is told on the answer after the failed read: {told}"
        );
    }

    /// **A held message another run took delivery of, read while the store
    /// cannot answer, is told on a later answer.** `arm` makes the one read the
    /// case names fail once. The answer that meets the failure says nothing and
    /// the held state stays `new`; the next answer, with the store healthy, names
    /// the state `read`. The board says the failure was spent, so the case cannot
    /// pass on a read that never failed.
    async fn a_failed_message_read_is_told_after_it_recovers(
        arm: impl Fn(&crate::mailboxes::testing::CountingMailboxes),
    ) {
        use crate::mailboxes::testing::{counting_handler, owning, send};
        let (jojobot, board) = counting_handler();
        let otto = owning(&jojobot, "otto").await;
        owning(&jojobot, "gamma").await;
        let sent = send(&jojobot, "otto", "gamma", "an errand").await;
        let id = sent["id"].as_str().expect("an id").to_string();

        let mut served = answered(serde_json::json!({
            "messages": [{"id": id, "mailbox": "otto", "state": "new"}],
        }));
        jojobot.add_held_changed(&mut served, Some(&otto)).await;

        let other = crate::harness::as_bot(&jojobot, "otto");
        jojobot
            .read_mailbox(rmcp::handler::server::wrapper::Parameters(
                crate::mailboxes::ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(other),
                },
            ))
            .await
            .expect("the other run takes delivery");

        arm(&board);
        assert!(board.failure_pending(), "the failure is armed");
        let mut failed = answered(serde_json::json!({"status": "ok"}));
        jojobot.add_held_changed(&mut failed, Some(&otto)).await;
        assert!(!board.failure_pending(), "the read failed");
        assert!(
            body_in(&failed).get(HELD_CHANGED).is_none(),
            "a failed read tells nothing: {failed:?}"
        );

        let mut next = answered(serde_json::json!({"status": "ok"}));
        jojobot.add_held_changed(&mut next, Some(&otto)).await;
        let told = body_in(&next);
        assert_eq!(told[HELD_CHANGED][0]["message"], id, "{told}");
        assert_eq!(told[HELD_CHANGED][0]["state"], "read", "{told}");
    }

    /// The read of the held message's history fails.
    #[tokio::test]
    async fn a_message_history_read_that_fails_once_keeps_the_held_version_for_the_next_answer() {
        a_failed_message_read_is_told_after_it_recovers(|board| board.fail_the_next_history())
            .await;
    }

    /// The read of the held messages by id fails.
    #[tokio::test]
    async fn a_read_of_held_messages_that_fails_once_keeps_their_versions_for_the_next_answer() {
        a_failed_message_read_is_told_after_it_recovers(|board| board.fail_the_next_read_by_id())
            .await;
    }

    fn event(ordinal: usize, kind: MessageEventKind, by: &str) -> MessageEvent {
        MessageEvent {
            ordinal,
            kind,
            at: Timestamp::UNIX_EPOCH,
            by_session: Some(by.to_string()),
            by_box: None,
            prior_state: jojobot_domain::mailbox::MessageState::New,
            notes: None,
            reason: None,
        }
    }

    /// **News is any write after the held version by another session, not only
    /// the last one.** The paired negatives are the same writes made by the
    /// session alone, and no writes at all.
    #[test]
    fn another_wrote_asks_about_every_write_since_and_not_the_last() {
        assert!(another_wrote([Some("bart"), Some("homer")], "homer"));
        assert!(another_wrote(
            [Some("homer"), Some("bart"), Some("homer")],
            "homer"
        ));
        assert!(!another_wrote([Some("homer"), Some("homer")], "homer"));
        assert!(!another_wrote([], "homer"));
        // A write no run was known for is not the session's own.
        assert!(another_wrote([None], "homer"));
    }

    /// **The writes since a message's held state are the events after the one
    /// that made it.** A message held as `new` has made none, so every event is
    /// since; held as `read`, those after delivery; held as `processed`, those
    /// after the last retirement.
    #[test]
    fn the_events_of_a_message_since_its_held_state_follow_the_event_that_made_it() {
        let events = [
            event(1, MessageEventKind::Read, "bart"),
            event(2, MessageEventKind::Processed, "homer"),
            event(3, MessageEventKind::Processed, "lisa"),
        ];
        let ordinals = |held: &str| -> Vec<usize> {
            events_after(&events, held)
                .iter()
                .map(|e| e.ordinal)
                .collect()
        };
        assert_eq!(ordinals("new"), [1, 2, 3]);
        assert_eq!(ordinals("read"), [2, 3]);
        assert_eq!(ordinals("processed"), Vec::<usize>::new());
    }

    /// **A claim notice stands beside the entity's unless that notice lists the
    /// claim.** The entity's lists the claims written since its own held
    /// version; an older edit is not among them and must be told on its own. The
    /// paired negative is the claim it does list, which is not told twice.
    #[test]
    fn a_claim_notice_is_dropped_only_when_the_entity_notice_lists_that_claim() {
        let entity = (
            Held::Entity("person:homer".into()),
            serde_json::json!({
                "handle": "person:homer",
                "claims_new": ["person:homer#f1"],
                "claims_edited": ["person:homer#f3"],
            }),
        );
        let claim = |address: &str| {
            (
                Held::Claim(address.into()),
                serde_json::json!({"address": address, "claim": "edited"}),
            )
        };
        let new = claim("person:homer#f1");
        let older = claim("person:homer#f2");
        let edited = claim("person:homer#f3");
        // Each key lists one claim: dropping either list from the reading keeps a
        // notice that the entity's own notice already carries.
        let sent = notices_to_send(&[entity.clone(), new, older.clone(), edited]);
        assert_eq!(sent, vec![entity.1, older.1]);
    }
}
