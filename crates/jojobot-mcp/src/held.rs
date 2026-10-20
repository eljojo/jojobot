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
//! Nothing a verb returns is shortened by this, and nothing is stored: the
//! ledger lives in the process and a restart empties it (a missed notice, never
//! a wrong one).

use super::*;

use crate::seen::{Held, Version};
use jiff::Timestamp;
use jojobot_domain::mailbox::{Message, MessageEvent, MessageEventKind, MessageId};
use jojobot_domain::memory::FactAddress;
use serde_json::{Map, Value};

/// The key a notice rides under.
pub(crate) const HELD_CHANGED: &str = "held_changed";

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
                if let Some(address) = text("address")
                    && address.contains('#')
                    && text("content").is_some()
                    && text("provenance").is_some()
                {
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
                Some(Version::Entity(ages.newest))
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
    /// Each thing the session holds that this answer does not serve again is
    /// read as it stands. One whose version moved is told when another session
    /// made the write; a move the session made itself is taken in silently.
    /// Whatever this answer serves is then held at the version it has now.
    pub(crate) async fn add_held_changed(
        &self,
        answered: &mut CallToolResponse,
        sid: Option<&str>,
    ) {
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let me = caller.sid.as_str().to_string();
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
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
        let served = served_in(&body);
        let held = self.registry.held_by(&me);
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
                continue;
            };
            if now == *was {
                continue;
            }
            match self.what_changed(what, was, &me, &messages).await {
                Changed::Told(notice) => {
                    told.push((what.clone(), notice));
                    moved.push((what.clone(), now));
                }
                Changed::Own => moved.push((what.clone(), now)),
                // The version stays, so the next answer reads again.
                Changed::Unread => {}
            }
        }
        for what in served {
            if let Some(version) = self.version_of(&what, &messages).await {
                moved.push((what, version));
            }
        }
        self.registry.note_held(&me, moved);
        let notices = notices_to_send(&told);
        if notices.is_empty() {
            return;
        }
        body[HELD_CHANGED] = Value::Array(notices);
        result.content[at] = ContentBlock::text(body.to_string());
    }

    /// **What another session did to a held thing since it was served.**
    async fn what_changed(
        &self,
        what: &Held,
        was: &Version,
        me: &str,
        messages: &HeldMessages,
    ) -> Changed {
        match (what, was) {
            (Held::Entity(handle), Version::Entity(since)) => {
                self.entity_changed(handle, *since, me).await
            }
            (Held::Claim(address), Version::Claim { writes, .. }) => {
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
            (Held::Message(id), Version::Message(held)) => {
                let Some(message) = messages.0.get(id) else {
                    return Changed::Own;
                };
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
                    "state": format!("{:?}", message.state).to_lowercase(),
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
            Some(Version::Entity(newest)) => newest,
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
            serde_json::json!({"handle": "person:homer", "claims_edited": ["person:homer#f1"]}),
        );
        let listed = (
            Held::Claim("person:homer#f1".into()),
            serde_json::json!({"address": "person:homer#f1", "claim": "edited"}),
        );
        let older = (
            Held::Claim("person:homer#f2".into()),
            serde_json::json!({"address": "person:homer#f2", "claim": "edited"}),
        );
        let sent = notices_to_send(&[entity.clone(), listed, older.clone()]);
        assert_eq!(sent, vec![entity.1, older.1]);
    }
}
