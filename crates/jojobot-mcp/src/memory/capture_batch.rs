//! `capture` with `entries` or `ticks` — many entries or many ticks on one list
//! in one call.
//!
//! **Every item goes through the one path a single entry or tick goes through**
//! ([`Jojobot::capture_one`], rule 498): first as a check of every item, which
//! writes nothing, and then as the write of each item, one part of the write
//! under the call's `request_id`. A check that refuses names the item by its
//! position. A write that stops partway leaves the id open, and the same call
//! again writes the rest and none twice.

use super::capture::{BATCH_CAP, CaptureArgs, EntryItem, Mode, TickArg};
use super::*;
use jojobot_domain::memory::entries;

/// What one pass of the single path answered.
enum Item {
    /// The pass went on: the answer's body.
    Went(serde_json::Value),
    /// The pass refused: why, in the refusal's own words.
    Refused(String),
}

/// **Every note the items of a batch earned**, gathered into one block for the
/// receipt (rule 498: one place builds a receipt's notes). A teaching line is kept
/// once, the possible-subject notes of the items are kept apart, and the unlocks
/// steps are merged by what they ask for, with the newest count of what the thing
/// holds. Every other key of an item's answer that is not the item's own
/// ([`ITEM_OWN`]) is kept too, by one rule.
#[derive(Default)]
struct Notes {
    teaching: Vec<String>,
    possible_subject: Vec<String>,
    unlocks: Option<serde_json::Value>,
    /// Every other note an item's answer carried, merged by key.
    others: serde_json::Map<String, serde_json::Value>,
}

/// **What an item's answer holds that is the item's own and not a note**: the
/// record it wrote, which a batch lists by address, the parts of a receipt
/// the batch builds itself, and the two lists that say which keys of the item
/// reached its thing. Every other key of an item's answer is a note, and
/// the batch carries it (rule 598).
const ITEM_OWN: &[&str] = &[
    "address",
    "changed",
    "content",
    "content_bytes",
    "content_elided",
    "content_head",
    "derived_from",
    "details",
    "details_bytes",
    "details_elided",
    "details_head",
    "edge",
    "fields",
    "fields_count",
    "fields_elided",
    "happened_at",
    "happened_through",
    "how_to_read",
    "inserted_at",
    "postcondition",
    "provenance",
    "reached_the_thing",
    "receipt",
    "recorded_at",
    "recorded_by",
    "refs",
    "revised",
    "revision_count",
    "sets",
    "sets_count",
    "sets_elided",
    "stale",
    "stale_after",
    "stale_note",
    "standing",
    "stands_for",
    "status",
    "stayed_on_the_record",
    "subject",
    "ticked",
];

/// **Fold one note into the one the batch holds under its key.** Arrays keep
/// each item once, objects fold key by key, and two different texts are kept
/// apart; anything else keeps the first.
fn fold_note(held: &mut serde_json::Value, found: &serde_json::Value) {
    match (held, found) {
        (serde_json::Value::Array(kept), serde_json::Value::Array(more)) => {
            for item in more {
                if !kept.contains(item) {
                    kept.push(item.clone());
                }
            }
        }
        (serde_json::Value::Object(kept), serde_json::Value::Object(more)) => {
            for (key, value) in more {
                match kept.get_mut(key) {
                    Some(there) => fold_note(there, value),
                    None => {
                        kept.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (serde_json::Value::String(kept), serde_json::Value::String(more)) if kept != more => {
            kept.push_str("\n\n");
            kept.push_str(more);
        }
        _ => {}
    }
}

impl Notes {
    /// Take the notes one item's answer carried.
    fn collect(&mut self, answer: &serde_json::Value) {
        for line in answer["teaching"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|line| line.as_str())
        {
            if !self.teaching.iter().any(|kept| kept == line) {
                self.teaching.push(line.to_string());
            }
        }
        if let Some(note) = answer["possible_subject_note"].as_str()
            && !self.possible_subject.iter().any(|kept| kept == note)
        {
            self.possible_subject.push(note.to_string());
        }
        if let Some(found) = answer.get("unlocks") {
            match &mut self.unlocks {
                None => self.unlocks = Some(found.clone()),
                Some(held) => {
                    if let Some(steps) = found["steps"].as_array() {
                        let kept = held["steps"].as_array_mut();
                        if let Some(kept) = kept {
                            for step in steps {
                                if !kept.contains(step) {
                                    kept.push(step.clone());
                                }
                            }
                        }
                    }
                    if let Some(holds) = found.get("holds") {
                        held["holds"] = holds.clone();
                    }
                }
            }
        }
        for (key, found) in answer.as_object().into_iter().flatten() {
            if ITEM_OWN.contains(&key.as_str())
                || ["teaching", "possible_subject_note", "unlocks"].contains(&key.as_str())
            {
                continue;
            }
            match self.others.get_mut(key) {
                Some(held) => fold_note(held, found),
                None => {
                    self.others.insert(key.clone(), found.clone());
                }
            }
        }
    }

    /// Put the gathered notes on the batch's receipt.
    fn deliver(self, body: &mut serde_json::Value) {
        if !self.teaching.is_empty() {
            body["teaching"] = self.teaching.into();
        }
        if !self.possible_subject.is_empty() {
            body["possible_subject_note"] = self.possible_subject.join("\n\n").into();
        }
        if let Some(unlocks) = self.unlocks {
            body["unlocks"] = unlocks;
        }
        for (key, note) in self.others {
            if body.get(&key).is_none() {
                body[key] = note;
            }
        }
    }
}

/// **The address a part under a `request_id` landed**, read back from what the
/// ledger kept for it.
fn address_of(recorded: &jojobot_domain::request::Recorded) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&recorded.landed)
        .ok()?
        .get("address")?
        .as_str()
        .map(str::to_string)
}

/// **The local id of the entry a tick names**, when it names one on this list.
fn entry_of(list: &EntityId, tick: &TickArg) -> Option<String> {
    FactAddress::parse(tick.entry.trim())
        .ok()
        .filter(|address| &address.home == list)
        .map(|address| address.local.0)
}

/// **How the caller finishes a batch that stopped partway.**
#[derive(Clone, Copy)]
enum Finish {
    /// The call carried no `request_id`: a resend would write the landed items
    /// again, so the caller sends only the items from the failed one on.
    SendTheRest,
    /// The call carried a `request_id`: the same call again writes the rest and
    /// none twice.
    ResendTheId,
}

impl Finish {
    /// The way to finish the batch in flight: its `request_id` when it carries
    /// one, and the items from the failed one on when it does not.
    fn now() -> Finish {
        if jojobot_domain::request::carries_an_id() {
            Finish::ResendTheId
        } else {
            Finish::SendTheRest
        }
    }
}

/// **Why a batch stopped at the item it did**, which decides who fixes it.
enum Stopped<'a> {
    /// The item was refused. Sending the call again meets the same refusal, so
    /// the call, or something it names, has to change.
    Refused(&'a str),
    /// The store failed under the item. The same item can succeed when sent
    /// again.
    Store,
    /// The item ended in an error that is no store failure, so a person reads
    /// the server log.
    Unexplained,
}

impl<'a> Stopped<'a> {
    /// **The cause of an error the write of an item ended in**, from the word the
    /// error itself wears: `retry` is a store failure, `change` is a refusal with
    /// the error's own words, and any other error is not told as either.
    fn of_error(error: &'a McpError) -> Stopped<'a> {
        match crate::answer::fix_by_of(error) {
            Some(crate::answer::FixBy::Retry) => Stopped::Store,
            Some(crate::answer::FixBy::Change) => Stopped::Refused(error.message.as_ref()),
            Some(crate::answer::FixBy::Person) | None => Stopped::Unexplained,
        }
    }
}

/// **The answer for a batch that stopped after `written` landed**, with or
/// without a `request_id`: the items that landed with their words and addresses,
/// and how to finish. It is a refusal of the item that failed, built as every
/// refusal is, and it says `wrote: true` because items did land. One builder for
/// both paths, so they agree field for field on `wrote` and `landed`.
fn half_landed(
    failed: usize,
    count: usize,
    label: &str,
    cause: Stopped<'_>,
    written: &[(usize, String)],
    labels: &[String],
    finish: Finish,
) -> CallToolResult {
    let landed: Vec<serde_json::Value> = written
        .iter()
        .map(|(at, address)| {
            serde_json::json!({"position": at + 1, "item": labels[*at], "address": address})
        })
        .collect();
    let named: Vec<String> = written
        .iter()
        .map(|(at, address)| format!("item {} ({}, {address})", at + 1, labels[*at]))
        .collect();
    let why = match cause {
        Stopped::Refused(why) => why,
        Stopped::Store => "the store could not write it",
        Stopped::Unexplained => "it ended in an error that is not a refusal",
    };
    let said = format!(
        "item {} of {count} ({label}) failed: {why}. {} landed and stay.",
        failed + 1,
        named.join(", "),
    );
    let from_here = format!("Send only the items from item {} on", failed + 1);
    let way: crate::answer::WayForward = match (cause, finish) {
        // **A refusal is the call's to change** (rule 130). The same call again
        // meets it at the same item, so it never reads as a retry.
        (Stopped::Refused(_), Finish::SendTheRest) => format!(
            "{said} {from_here}, with item {} changed so it is accepted. Sending the same call \
             again would write the landed items a second time.",
            failed + 1,
        )
        .into(),
        (Stopped::Refused(_), Finish::ResendTheId) => format!(
            "{said} The same call is refused at item {} again until what the refusal names is \
             fixed. Fix that, then send the same call again with the same request_id and it \
             writes the rest and none twice. Or {}, with item {} changed, under a new \
             request_id.",
            failed + 1,
            from_here.to_lowercase(),
            failed + 1,
        )
        .into(),
        (Stopped::Store, Finish::SendTheRest) => crate::answer::WayForward::retry(format!(
            "{said} Sending the same call again would write the landed items a second time. \
             {from_here} once the store answers, or send a request_id with the first call next \
             time so the same call can finish the batch."
        )),
        (Stopped::Store, Finish::ResendTheId) => crate::answer::WayForward::retry(format!(
            "{said} Send the same call again with the same request_id and it writes the rest \
             and none twice."
        )),
        // An error nobody can name is not an outage that ends by waiting.
        (Stopped::Unexplained, _) => crate::answer::WayForward::person(format!(
            "{said} The server log holds the error. Do not send the same call again until a \
             person has read it."
        )),
    };
    marked_landed(crate::caller::handle_declined("capture", way), landed)
}

/// **An answer that says the items in it landed**: `wrote` true and each item with
/// its words and address.
fn marked_landed(mut answered: CallToolResult, landed: Vec<serde_json::Value>) -> CallToolResult {
    let mut body = read_body(&answered);
    body["wrote"] = true.into();
    body["landed"] = landed.into();
    answered.content = vec![ContentBlock::text(body.to_string())];
    answered
}

/// **The answer for a batch whose items all landed and whose last step did not**:
/// the step's own answer, with the items that landed named.
fn landed_and_unfinished(
    unfinished: CallToolResult,
    written: &[(usize, String)],
    labels: &[String],
) -> CallToolResult {
    let landed = written
        .iter()
        .map(|(at, address)| {
            serde_json::json!({"position": at + 1, "item": labels[*at], "address": address})
        })
        .collect();
    marked_landed(unfinished, landed)
}

/// The JSON body of an answer.
fn read_body(answered: &CallToolResult) -> serde_json::Value {
    answered
        .content
        .first()
        .and_then(|block| block.as_text())
        .and_then(|text| serde_json::from_str(&text.text).ok())
        .unwrap_or_default()
}

/// **The sentence every refusal ends with**, which the batch says once for the
/// whole call and not again for the item it names.
const NOTHING_MISSING: &str = " Nothing is missing from the store";

/// Read one pass's answer as a body, and tell a refusal from a success.
fn read_item(answered: &CallToolResult) -> Item {
    let body = read_body(answered);
    if body["status"] == "blocked" {
        let said = body["how_to_proceed"]
            .as_str()
            .unwrap_or("it was refused")
            .trim_start_matches("Nothing was written: ")
            .trim_start_matches("Nothing was written. ");
        // **The reason, without the tail every refusal ends with.** The batch's own
        // refusal carries that tail once, so the item's is cut here, and the full
        // stop that ended the reason goes with it: the batch adds its own.
        let reason = said
            .split_once(NOTHING_MISSING)
            .map_or(said, |(reason, _)| reason);
        let why = reason
            .trim_start_matches("invalid fact: ")
            .trim_end()
            .trim_end_matches('.')
            .to_string();
        return Item::Refused(why);
    }
    Item::Went(body)
}

/// **What writing one claim came to**: the write's own result, or an answer that
/// ends the capture at once.
pub(super) enum ClaimWrite {
    Ran(Box<Result<Guarded<Fact>, MemoryError>>),
    Early(Result<CallToolResult, McpError>),
}

impl Jojobot {
    /// **The write of one claim.** An item of a batch is one part under its own
    /// name, written once under a `request_id`, and keeps in the part's note that
    /// it ends what is owed, so a resend knows the batch still has that to do. A
    /// part an earlier call wrote is not written again: its address is read back.
    /// Any other claim is the whole write, armed as the first of two parts when it
    /// ends what is owed.
    pub(super) async fn write_the_claim(
        &self,
        part: Option<&str>,
        clears_due_on: bool,
        new: NewFact,
    ) -> ClaimWrite {
        let write =
            super::due::writing_a_claim(clears_due_on && part.is_none(), self.memory.capture(new));
        let Some(name) = part else {
            return ClaimWrite::Ran(Box::new(write.await));
        };
        let note = clears_due_on.then(|| super::due::CLEARS_DUE_ON.to_string());
        match jojobot_domain::request::part(name, note, write).await {
            jojobot_domain::request::Part::Ran(written) => ClaimWrite::Ran(Box::new(written)),
            jojobot_domain::request::Part::Recorded(recorded) => {
                ClaimWrite::Early(match address_of(&recorded) {
                    Some(address) => {
                        json_result(&serde_json::json!({ "address": address, "recorded": true }))
                    }
                    None => Ok(crate::caller::handle_declined(
                        "capture",
                        crate::answer::WayForward::person(
                            "A claim was written under this request_id, but its address \
                             cannot be read back, so the rest cannot be finished.",
                        ),
                    )),
                })
            }
        }
    }
}

impl Jojobot {
    /// **The tick setting of a list and the state each of its entries reads as**,
    /// by the entry's local id: what the first tick of a batch is judged against.
    async fn tick_standing(
        &self,
        list: &EntityId,
    ) -> Result<
        (
            entries::Setting,
            std::collections::BTreeMap<String, &'static str>,
        ),
        MemoryError,
    > {
        let held = self.memory.recall(list).await?;
        let setting = entries::Setting::of(&self.memory.fields(list).await?);
        let states = entries::view(&held, setting).states;
        Ok((setting, states))
    }

    /// **Add many entries to a list, or tick many, in one call.**
    pub(super) async fn capture_batch(
        &self,
        args: CaptureArgs,
    ) -> Result<CallToolResult, McpError> {
        let refuse = |why: String| memory_declined("capture", MemoryError::InvalidFact(why));
        let subject = EntityId::person(&args.subject);
        let ticking = args.ticks.is_some();
        let form = if ticking { "ticks" } else { "entries" };
        if args.entries.is_some() && ticking {
            return refuse(
                "`entries` adds entries and `ticks` ticks them: they are two calls".into(),
            );
        }
        if args.tick.is_some() || (!ticking && !args.content.trim().is_empty()) {
            return refuse(format!(
                "`{form}` carries the words or the ticks of every item. Leave `content` and \
                 `tick` off, or send the single form alone"
            ));
        }
        if ticking && args.after.is_some() {
            return refuse(
                "`ticks` tick entries and `after` places new ones: they are two calls".into(),
            );
        }
        if subject.kind() != Some(EntityKind::LIST) {
            return refuse(format!(
                "`{form}` works on a list, and {subject} is not a thing of kind list"
            ));
        }
        let count =
            args.entries.as_ref().map_or(0, Vec::len) + args.ticks.as_ref().map_or(0, Vec::len);
        if count == 0 {
            return refuse(format!("`{form}` is empty: send at least one item"));
        }
        if count > BATCH_CAP {
            return refuse(format!(
                "`{form}` holds {count} items and a call takes at most {BATCH_CAP}. Send them \
                 in calls of {BATCH_CAP} or fewer"
            ));
        }

        // **One single-form call per item**, sharing everything else the call sent.
        let mut items: Vec<(CaptureArgs, String)> = Vec::with_capacity(count);
        for entry in args.entries.iter().flatten() {
            let mut one = args.clone();
            one.entries = None;
            one.request_id = None;
            let label = match entry {
                EntryItem::Text(text) => {
                    one.content = text.clone();
                    text.clone()
                }
                EntryItem::Full {
                    content,
                    details,
                    shape,
                    object,
                } => {
                    one.content = content.clone();
                    one.details = details.clone();
                    one.shape = shape.clone();
                    one.object = object.clone();
                    content.clone()
                }
            };
            items.push((one, label));
        }
        for tick in args.ticks.iter().flatten() {
            let mut one = args.clone();
            one.ticks = None;
            one.request_id = None;
            if one.content.trim().is_empty() {
                one.content = tick.act.clone();
            }
            one.tick = Some(tick.clone());
            items.push((one, format!("{} {}", tick.entry, tick.act)));
        }

        // **The caller is identified once for the whole batch**, and every item
        // reuses that read of the run.
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **Held for the whole batch**, so no other caller's entry lands between
        // two of ours.
        let _placing = match self.hold_placing(&subject).await {
            Ok(guard) => guard,
            Err(e) => return memory_declined("capture", e),
        };
        let part_of = |at: usize| format!("item-{at}");

        // **Every item is checked before any is written**, by the path that writes
        // it. An item an earlier call already wrote is not checked again: its
        // tick would now read as already made.
        // **Each tick is judged against the list as the ticks before it leave
        // it**, by the one tick judge, so a batch that cannot stand is refused
        // whole and one that only stands in order is not.
        let mut standing = if ticking {
            match self.tick_standing(&subject).await {
                Ok(standing) => Some(standing),
                Err(e) => return memory_declined("capture", e),
            }
        } else {
            None
        };
        for (at, (one, label)) in items.iter().enumerate() {
            if jojobot_domain::request::recorded(&part_of(at)).is_some() {
                continue;
            }
            let named = one.tick.as_ref().and_then(|tick| entry_of(&subject, tick));
            let tick_state = standing
                .as_ref()
                .zip(named.as_ref())
                .and_then(|((_, states), local)| states.get(local).copied());
            let checked = self
                .capture_one(
                    one.clone(),
                    Mode {
                        check_only: true,
                        placing_held: true,
                        part: None,
                        tick_state,
                        ends_later: None,
                        caller: Some(caller.clone()),
                    },
                )
                .await?;
            if let Item::Refused(why) = read_item(&checked) {
                return refuse(format!(
                    "item {} of {count} ({label}) is refused, so nothing was written: {why}",
                    at + 1
                ));
            }
            if let (Some((setting, states)), Some(local), Some(act)) = (
                standing.as_mut(),
                named,
                one.tick
                    .as_ref()
                    .and_then(|tick| entries::Act::of(&tick.act)),
            ) {
                states.insert(local, entries::state_after(*setting, act));
            }
        }

        let labels: Vec<String> = items.iter().map(|(_, label)| label.clone()).collect();
        let before = self.as_it_stands(&subject).await;
        let mut written: Vec<(usize, String)> = Vec::with_capacity(count);
        // **How many items an earlier call wrote**, counted by item and not by the
        // parts the ledger names.
        let mut already_written = 0usize;
        // **Set by an item that ends what is owed**: the batch takes the stored due
        // moment off once, after its last item, so no item finishes the batch's
        // id before the rest are written.
        let ends_later = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // **The notes the items earn are held back and delivered together**: each
        // item's ledger spends wait in `spends`, and the receipt carries the notes.
        let spends = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let mut notes = Notes::default();
        for (at, (mut one, label)) in items.into_iter().enumerate() {
            // **An item an earlier call wrote is not written, or judged, again**:
            // the list has moved on since, and a drop it made would be refused
            // against the list it changed. Its address is read back.
            let earlier = jojobot_domain::request::recorded(&part_of(at));
            if let Some(address) = earlier.as_ref().and_then(address_of) {
                let _ = jojobot_domain::request::skip(&part_of(at));
                already_written += 1;
                // **An item that ended what is owed leaves the batch that still
                // has to take it off**, whether or not that was done before the cut.
                if earlier.as_ref().and_then(|r| r.note.as_deref())
                    == Some(super::due::CLEARS_DUE_ON)
                {
                    ends_later.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                // **What that item earned is told now**: the call that wrote it was
                // cut before it delivered, and held back what it would have spent.
                if let Ok(held) = self.memory.recall(&subject).await
                    && let Some(fact) = held.iter().find(|f| f.address().to_string() == address)
                {
                    let mut said = serde_json::json!({});
                    crate::teaching::BATCH_SPENDS
                        .scope(
                            spends.clone(),
                            self.note_possible_subject(fact, &caller, &mut said),
                        )
                        .await;
                    notes.collect(&said);
                }
                written.push((at, address));
                continue;
            }
            if !ticking && at > 0 {
                one.after = written.last().map(|(_, address)| address.clone());
            }
            let answered = crate::teaching::BATCH_SPENDS
                .scope(
                    spends.clone(),
                    self.capture_one(
                        one,
                        Mode {
                            check_only: false,
                            placing_held: true,
                            part: Some(part_of(at)),
                            tick_state: None,
                            ends_later: Some(ends_later.clone()),
                            caller: Some(caller.clone()),
                        },
                    ),
                )
                .await;
            let answered = match answered {
                Ok(answered) => answered,
                // **A store that fails partway answers as an error, not as a
                // refusal of the item**, and the items before it have landed: the
                // answer says so rather than that nothing was written.
                Err(e) if !written.is_empty() => {
                    tracing::warn!(error = %e.message, "a batch item ended in an error after earlier items landed");
                    return Ok(half_landed(
                        at,
                        count,
                        &label,
                        Stopped::of_error(&e),
                        &written,
                        &labels,
                        Finish::now(),
                    ));
                }
                Err(e) => return Err(e),
            };
            match read_item(&answered) {
                Item::Went(body) => {
                    let Some(address) = body["address"].as_str() else {
                        return refuse(format!(
                            "item {} of {count} ({label}) was written but its address could not \
                             be read back",
                            at + 1
                        ));
                    };
                    notes.collect(&body);
                    written.push((at, address.to_string()));
                }
                Item::Refused(why) => {
                    // **Stopped at the item that failed.** With an id the batch is
                    // not finished, and the same call finishes it.
                    let landed = written.len();
                    if landed > 0 {
                        return Ok(half_landed(
                            at,
                            count,
                            &label,
                            Stopped::Refused(&why),
                            &written,
                            &labels,
                            Finish::now(),
                        ));
                    }
                    return refuse(format!(
                        "item {} of {count} ({label}) failed: {why}. {landed} items landed and stay",
                        at + 1
                    ));
                }
            }
        }
        // **Every item has landed. What they ended is taken off once.** A failure
        // here leaves all of them written and the same call to finish it.
        if ends_later.load(std::sync::atomic::Ordering::SeqCst)
            && let Some(unfinished) = self.end_what_is_owed("capture", &subject, &caller).await
        {
            return Ok(landed_and_unfinished(unfinished, &written, &labels));
        }
        // **The notes are delivered, so their ledger rows are written now**, and
        // never before: a batch cut partway never reaches this line.
        let held_back: Vec<String> =
            std::mem::take(&mut *spends.lock().expect("a batch's spends are poisoned"));
        for domain in held_back {
            self.first_contact(&domain, Some(&caller)).await;
        }
        jojobot_domain::request::finished();

        let listed: Vec<serde_json::Value> = written
            .iter()
            .map(
                |(at, address)| match args.ticks.as_ref().and_then(|t| t.get(*at)) {
                    Some(tick) => serde_json::json!({
                        "position": at + 1, "record": address, "entry": tick.entry, "act": tick.act,
                    }),
                    None => serde_json::json!({ "position": at + 1, "address": address }),
                },
            )
            .collect();
        let mut body = serde_json::json!({
            "list": subject.as_str(),
            "count": count,
            form: listed,
        });
        if already_written > 0 {
            body["finished_a_cut_batch"] = serde_json::json!({
                "already_written": already_written,
                "written_now": count - already_written,
            });
        }
        notes.deliver(&mut body);
        let changed = self.changed_on(&before, &subject).await;
        crate::changed::note_changed(&mut body, changed);
        crate::answer::note_postcondition(
            &mut body,
            format!("{count} {form} landed on {subject}, each its own record, in the order sent."),
        );
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `fix_by` word an answer wears.
    fn word(answered: &CallToolResult) -> String {
        read_body(answered)["fix_by"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn stopped_at_three(cause: Stopped<'_>, finish: Finish) -> CallToolResult {
        let labels = vec!["post".to_string(), "buy".to_string(), "fix".to_string()];
        let written = vec![
            (0, "list:homer-chores#f2".to_string()),
            (1, "list:homer-chores#f3".to_string()),
        ];
        half_landed(2, 3, "fix", cause, &written, &labels, finish)
    }

    /// **Who fixes a batch that stopped is decided by why it stopped, and an
    /// id changes only how the rest is sent.** A refused item is the call's to
    /// change with an id and without one; a store failure is a retry both ways; an
    /// error that is no store failure is a person's.
    #[test]
    fn a_batch_that_stopped_wears_the_word_its_cause_earns_with_an_id_and_without() {
        for finish in [Finish::SendTheRest, Finish::ResendTheId] {
            let refused = stopped_at_three(Stopped::Refused("the edge names nothing"), finish);
            assert_eq!(word(&refused), "change");
            let store = stopped_at_three(Stopped::Store, finish);
            assert_eq!(word(&store), "retry");
            let unexplained = stopped_at_three(Stopped::Unexplained, finish);
            assert_eq!(word(&unexplained), "person");
        }
    }

    /// **An error is told as the cause its own word names**: `retry` is a store
    /// failure, `change` is a refusal and carries the error's words, and an error
    /// that wears neither is not told as either. A store that answered and refused
    /// after earlier items landed arrives as an error that wears `change`, and the
    /// batch answer for it is the call's to change, with and without an id.
    #[test]
    fn an_error_is_told_as_the_cause_its_own_word_names() {
        let outage = crate::memory::declined::memory_error(MemoryError::Store("down".into()));
        assert!(matches!(Stopped::of_error(&outage), Stopped::Store));

        let refusal = crate::memory::declined::memory_error(MemoryError::Refused(
            "the store refused the row".into(),
        ));
        let Stopped::Refused(why) = Stopped::of_error(&refusal) else {
            panic!("a store refusal is told as a refusal");
        };
        assert!(why.contains("refused"), "the error's own words: {why}");
        for finish in [Finish::SendTheRest, Finish::ResendTheId] {
            assert_eq!(
                word(&stopped_at_three(Stopped::of_error(&refusal), finish)),
                "change"
            );
        }

        let other = McpError::internal_error("a bug", None);
        assert!(matches!(Stopped::of_error(&other), Stopped::Unexplained));
    }

    use crate::harness::*;
    use crate::memory::testing::*;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::request::testing::InMemoryRequests;

    /// A handler whose writes under a `request_id` are kept in a ledger the case
    /// holds, so it can make one part of a write fail once.
    fn handler_with_ledger() -> (Jojobot, Arc<InMemoryRequests>) {
        let ledger = InMemoryRequests::new();
        // The store spends a part into the same ledger the handler reads, as the real
        // store does in its own table.
        let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
        (handler_over(memory).with_requests(ledger.clone()), ledger)
    }

    /// **A capture sent as the dispatch sends one that carries a `request_id`**: the id
    /// is looked up, the verb runs inside the scope that lookup made, and the
    /// answer is handed back to the ledger. A direct call to the verb has no scope,
    /// so a part would never be recorded.
    async fn sent_under_its_id(
        jojobot: &Jojobot,
        args: CaptureArgs,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = args.request_id.clone().expect("a request id");
        let params = rmcp::model::CallToolRequestParams::new("capture").with_arguments(
            serde_json::json!({ "request_id": id })
                .as_object()
                .expect("an object")
                .clone(),
        );
        let crate::request::Begun::Fresh(scope, _flight) =
            jojobot.begin_request(&params, Some(TEST_SID)).await
        else {
            panic!("the id is new or only partly written");
        };
        let ran =
            jojobot_domain::request::within(scope.clone(), jojobot.capture(Parameters(args))).await;
        if let Ok(answered) = &ran {
            let complete = rmcp::model::CallToolResponse::Complete(answered.clone());
            let _ = jojobot.end_request(&scope, &complete).await;
        }
        ran
    }

    /// A list with two chores, and their addresses.
    async fn two_chores(jojobot: &Jojobot) -> (String, String) {
        let mut addresses = Vec::new();
        for text in ["mow the lawn", "wash the car"] {
            let landed = capture_ok(
                jojobot,
                CaptureArgs {
                    provenance: Some("testimony".into()),
                    ..capture_args("list:homer-chores", text)
                },
            )
            .await;
            addresses.push(
                landed["address"]
                    .as_str()
                    .unwrap_or_else(|| panic!("a capture answers with its address: {landed}"))
                    .to_string(),
            );
        }
        (addresses[0].clone(), addresses[1].clone())
    }

    fn ticking(ticks: &[(&str, &str)], request_id: Option<&str>) -> CaptureArgs {
        CaptureArgs {
            provenance: Some("testimony".into()),
            ticks: Some(
                ticks
                    .iter()
                    .map(|(entry, act)| TickArg {
                        entry: entry.to_string(),
                        act: act.to_string(),
                    })
                    .collect(),
            ),
            request_id: request_id.map(str::to_string),
            ..capture_args("list:homer-chores", "")
        }
    }

    /// The ticks the list holds and the state each entry reads, by text.
    async fn the_list(jojobot: &Jojobot) -> (usize, Vec<String>) {
        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    ticks: Some(true),
                    ..recall_args("list:homer-chores")
                }))
                .await
                .expect("recall ok"),
        );
        let object = &read["objects"][0];
        let ticks = object["ticks"].as_array().map_or(0, Vec::len);
        let states = object["entries"]
            .as_array()
            .unwrap_or_else(|| panic!("a list answers with its entries: {read}"))
            .iter()
            .map(|entry| {
                format!(
                    "{}:{}",
                    entry["text"].as_str().unwrap_or_default(),
                    entry["state"].as_str().unwrap_or_default()
                )
            })
            .collect();
        (ticks, states)
    }

    /// **A batch cut after its first item is finished by the same call sent again.**
    /// The drop landed before the cut. The resend finds it recorded under its
    /// `request_id`, reads its address back and does not judge it again against a
    /// list it has already changed, which would refuse it. The lawn is dropped
    /// once and the car is done once.
    #[tokio::test]
    async fn a_cut_batch_with_a_drop_is_finished_by_the_same_call_again() {
        let (jojobot, ledger) = handler_with_ledger();
        let (lawn, car) = two_chores(&jojobot).await;
        let batch = || ticking(&[(&lawn, "dropped"), (&car, "done")], Some("chores-1"));

        ledger.fail_the_next_commit_of("item-1");
        let cut = json_of(
            &sent_under_its_id(&jojobot, batch())
                .await
                .expect("a cut batch answers"),
        );
        // **The cut is told as the half-landed answer it is**: a store that
        // stopped between two items is a `retry` that says an item landed.
        assert_eq!(cut["status"], "blocked", "{cut}");
        assert_eq!(cut["fix_by"], "retry", "{cut}");
        assert_eq!(cut["wrote"], true, "{cut}");
        assert_eq!(cut["landed"].as_array().map(Vec::len), Some(1), "{cut}");
        assert_eq!(
            the_list(&jojobot).await.0,
            1,
            "the drop before the cut landed"
        );

        let finished = json_of(
            &sent_under_its_id(&jojobot, batch())
                .await
                .expect("the resend answers"),
        );
        assert_ne!(
            finished["status"], "blocked",
            "the resend is not refused: {finished}"
        );
        assert_eq!(
            finished["finished_a_cut_batch"]["already_written"], 1,
            "{finished}"
        );
        assert_eq!(
            finished["finished_a_cut_batch"]["written_now"], 1,
            "{finished}"
        );
        let (ticks, states) = the_list(&jojobot).await;
        assert_eq!(ticks, 2, "no tick was written twice");
        assert_eq!(
            states,
            ["wash the car:done"],
            "the lawn is dropped once and the car is done once"
        );
    }

    /// A batch of entries on the chores list, as one call.
    fn entries_of(words: &[&str], request_id: Option<&str>) -> CaptureArgs {
        CaptureArgs {
            provenance: Some("testimony".into()),
            entries: Some(
                words
                    .iter()
                    .map(|text| crate::memory::capture::EntryItem::Text(text.to_string()))
                    .collect(),
            ),
            request_id: request_id.map(str::to_string),
            ..capture_args("list:homer-chores", "")
        }
    }

    /// The paragraphs of a batch's `possible_subject_note`, none when it has none.
    fn note_paragraphs(answered: &serde_json::Value) -> usize {
        answered["possible_subject_note"]
            .as_str()
            .map_or(0, |note| note.split("\n\n").count())
    }

    /// **A whole batch spends a note once and delivers it once; a repeat batch
    /// naming the same person spends nothing.** Two entries name Marge: the receipt
    /// carries one note about her, not two and not none. The same words sent as a
    /// second batch are told nothing, because the first delivered what it spent.
    #[tokio::test]
    async fn a_whole_batch_spends_a_note_once_and_a_repeat_batch_spends_nothing() {
        let (jojobot, _) = handler_with_ledger();
        ensure(&jojobot, "list:homer-chores").await;
        ensure(&jojobot, "person:marge").await;

        let first = json_of(
            &jojobot
                .capture(Parameters(entries_of(&["call marge", "email marge"], None)))
                .await
                .expect("the batch answers"),
        );
        assert!(
            first["possible_subject_note"]
                .as_str()
                .is_some_and(|note| note.contains("person:marge")),
            "the batch names the person its items name: {first}"
        );
        assert_eq!(note_paragraphs(&first), 1, "once, not once for each item");

        let repeat = json_of(
            &jojobot
                .capture(Parameters(entries_of(&["phone marge", "text marge"], None)))
                .await
                .expect("the batch answers"),
        );
        assert_eq!(
            note_paragraphs(&repeat),
            0,
            "a batch that was already told spends nothing: {repeat}"
        );
    }

    /// **A batch cut after its first item spends no note, and the resend delivers
    /// the note the first item earned, once.** The first item names Marge and
    /// lands; the second is cut. The cut call held back what it would have spent,
    /// so the resend, which does not write the first item again, still tells her.
    /// Paired: a plain capture naming her afterwards is told nothing, because the
    /// resend delivered it.
    #[tokio::test]
    async fn a_cut_batch_spends_no_note_and_its_resend_delivers_it_once() {
        let (jojobot, ledger) = handler_with_ledger();
        ensure(&jojobot, "list:homer-chores").await;
        ensure(&jojobot, "person:marge").await;
        let batch = || entries_of(&["call marge", "wash the car"], Some("errands-1"));

        ledger.fail_the_next_commit_of("item-1");
        let cut = sent_under_its_id(&jojobot, batch()).await;
        assert!(
            cut.as_ref()
                .map_or(true, |answered| json_of(answered)["status"] == "blocked"),
            "the cut item fails the call: {cut:?}"
        );

        let finished = json_of(
            &sent_under_its_id(&jojobot, batch())
                .await
                .expect("the resend answers"),
        );
        assert_eq!(
            finished["finished_a_cut_batch"]["already_written"], 1,
            "{finished}"
        );
        assert!(
            finished["possible_subject_note"]
                .as_str()
                .is_some_and(|note| note.contains("person:marge")),
            "the resend delivers the note the first item earned: {finished}"
        );
        assert_eq!(note_paragraphs(&finished), 1, "once");

        let later = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("list:homer-chores", "ring marge again")
            },
        )
        .await;
        assert!(
            later.get("possible_subject_note").is_none(),
            "the resend delivered it, so it is spent: {later}"
        );
    }

    /// **Each tick of a batch is judged against the list the ticks before it
    /// leave**, so a batch that cannot stand is refused whole, naming the item,
    /// and one that only stands in order lands in order.
    #[tokio::test]
    async fn ticks_are_judged_in_order_and_a_batch_that_cannot_stand_writes_nothing() {
        let (jojobot, _) = handler_with_ledger();
        let (lawn, car) = two_chores(&jojobot).await;

        for second in ["dropped", "done"] {
            let refused = blocked(
                &jojobot
                    .capture(Parameters(ticking(
                        &[(&lawn, "dropped"), (&lawn, second)],
                        None,
                    )))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["wrote"], false, "{refused}");
            assert!(
                refused["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains("item 2")),
                "the refusal names the second item: {refused}"
            );
            assert_eq!(
                the_list(&jojobot).await.0,
                0,
                "dropped then {second} wrote nothing"
            );
        }

        // The pair: ticks that stand only in order land.
        capture_ok(
            &jojobot,
            CaptureArgs {
                tick: Some(TickArg {
                    entry: lawn.clone(),
                    act: "dropped".into(),
                }),
                provenance: Some("testimony".into()),
                ..capture_args("list:homer-chores", "not the lawn")
            },
        )
        .await;
        capture_ok(
            &jojobot,
            ticking(&[(&lawn, "undone"), (&lawn, "done")], None),
        )
        .await;
        capture_ok(&jojobot, ticking(&[(&car, "done"), (&car, "undone")], None)).await;
        let (ticks, states) = the_list(&jojobot).await;
        assert_eq!(ticks, 5);
        assert_eq!(states, ["wash the car:open", "mow the lawn:done"]);
    }
    /// **A batch of three entries on a list, cut at its second item**, sent with
    /// or without a `request_id`. The first entry lands, so the answer is the
    /// half-landed one. `cut` says how the second fails: the store fails the
    /// write for a cause, or the item itself is refused.
    async fn a_batch_cut_at_its_second_item(
        cut: Cut2,
        request_id: Option<&str>,
    ) -> serde_json::Value {
        // **An item the guard refuses needs no failing store**: the handler that
        // works answers both.
        let (healthy, blind) = healthy_and_down(Down::WritesAfter(
            1,
            Default::default(),
            match cut {
                Cut2::Store(cause) => cause,
                Cut2::Item => crate::memory::testing::Cause::Outage,
            },
        ));
        healthy
            .add_entity(Parameters(add_args("list", "list:errands", "Errands")))
            .await
            .expect("add ok");
        let second = match cut {
            Cut2::Store(_) => EntryItem::Text("second".into()),
            // An edge to a thing that does not exist: the store takes the first
            // entry and the guard refuses this one.
            Cut2::Item => EntryItem::Full {
                content: "second".into(),
                details: None,
                shape: Some("about".into()),
                object: Some("thing:kettle".into()),
            },
        };
        let handler = match cut {
            Cut2::Store(_) => &blind,
            Cut2::Item => &healthy,
        };
        let call = handler.capture(Parameters(CaptureArgs {
            entries: Some(vec![
                EntryItem::Text("first".into()),
                second,
                EntryItem::Text("third".into()),
            ]),
            request_id: request_id.map(Into::into),
            provenance: Some("testimony".into()),
            ..capture_args("list:errands", "")
        }));
        // The dispatch puts a call that sent an id inside the id's scope.
        let answered = match request_id {
            Some(id) => {
                let scope = jojobot_domain::request::Scope::new(jojobot_domain::request::Request {
                    bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
                    id: jojobot_domain::request::RequestId::parse(id).expect("a valid id"),
                    verb: "capture".into(),
                    digest: "digest".into(),
                });
                jojobot_domain::request::within(scope, call).await
            }
            None => call.await,
        }
        .expect("a half-landed batch is an answer");
        json_of(&answered)
    }

    #[derive(Clone, Copy)]
    enum Cut2 {
        Store(crate::memory::testing::Cause),
        Item,
    }

    /// What one cut answers: the word, and whether the way forward sends the
    /// same call again or only the rest.
    fn cut_answer(body: &serde_json::Value) -> (String, bool, bool) {
        let how = body["how_to_proceed"].as_str().unwrap_or_default();
        (
            body["fix_by"].as_str().unwrap_or_default().to_string(),
            how.contains("same call again with the same request_id"),
            how.contains("Send only the items from item 2 on"),
        )
    }

    /// **Every way a batch is cut, through the surface**, each reached
    /// by a cut that picks it and checked on the word and on what the way forward
    /// tells the caller to do. The items that landed are named in every one.
    /// **Paired by the arms beside each other**: the word is the error's class,
    /// so no arm passes on a build where every cut answers the same word.
    #[tokio::test]
    async fn a_half_landed_batch_wears_the_class_of_the_cut_and_says_how_to_finish() {
        use crate::memory::testing::Cause;
        // (cut, request id, word, resend-with-id advice, send-the-rest advice)
        let arms = [
            // An outage, with an id: resend the id.
            (
                Cut2::Store(Cause::Outage),
                Some("arm-outage"),
                "retry",
                true,
                false,
            ),
            // An outage, no id: retry, but send only the rest.
            (Cut2::Store(Cause::Outage), None, "retry", false, true),
            // Stored damage is a person's, with or without an id.
            (
                Cut2::Store(Cause::Damage("a row edited by hand")),
                Some("arm-damage"),
                "person",
                false,
                false,
            ),
            (
                Cut2::Store(Cause::Damage("a row edited by hand")),
                None,
                "person",
                false,
                false,
            ),
            // A store refusal with no id: the call must change, and send only the rest.
            (
                Cut2::Store(Cause::Refusal("a constraint")),
                None,
                "change",
                false,
                true,
            ),
            // A store refusal with an id: the call must change before the same id
            // finishes the batch.
            (
                Cut2::Store(Cause::Refusal("a constraint")),
                Some("arm-refusal"),
                "change",
                true,
                false,
            ),
            // An item the guard refused, with no id: change, send only the rest.
            (Cut2::Item, None, "change", false, true),
            // An item the guard refused, with an id.
            (Cut2::Item, Some("arm-item"), "change", true, false),
        ];
        for (cut, id, word, resend, rest) in arms {
            let body = a_batch_cut_at_its_second_item(cut, id).await;
            assert_eq!(body["wrote"], true, "{id:?}: {body}");
            assert_eq!(
                body["landed"].as_array().map(Vec::len),
                Some(1),
                "{id:?}: {body}"
            );
            assert_eq!(
                cut_answer(&body),
                (word.to_string(), resend, rest),
                "{id:?}: {body}"
            );
        }
    }

    /// **A single capture under a request id wears the class of the store error
    /// that refused it**, as it does without one: a rule the store enforces is a
    /// `change`, an outage is a `retry`, and damage is a `person`. The id changes
    /// none of the three. Paired in one case so a build that answered every cause
    /// the same word would not pass.
    #[tokio::test]
    async fn a_capture_under_an_id_wears_the_class_of_the_store_error() {
        use crate::memory::testing::{Cause, ensure};
        for (cause, word) in [
            (Cause::Outage, "retry"),
            (Cause::Refusal("a constraint"), "change"),
            (Cause::Damage("a row edited by hand"), "person"),
        ] {
            for id in [None, Some("single-r1")] {
                let (healthy, blind) =
                    healthy_and_down(Down::WritesAfter(0, Default::default(), cause));
                ensure(&healthy, "person:homer").await;
                let call = blind.capture(Parameters(CaptureArgs {
                    request_id: id.map(Into::into),
                    ..capture_args("person:homer", "likes donuts")
                }));
                let answered = match id {
                    Some(id) => {
                        let scope =
                            jojobot_domain::request::Scope::new(jojobot_domain::request::Request {
                                bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
                                id: jojobot_domain::request::RequestId::parse(id)
                                    .expect("a valid id"),
                                verb: "capture".into(),
                                digest: "digest".into(),
                            });
                        jojobot_domain::request::within(scope, call).await
                    }
                    None => call.await,
                };
                let said = match answered {
                    Ok(answer) => json_of(&answer)["fix_by"].clone(),
                    Err(error) => error.data.expect("a stamped error")["fix_by"].clone(),
                };
                assert_eq!(said, word, "{id:?} under {word}");
            }
        }
    }
}

#[cfg(test)]
mod notes_cases;
