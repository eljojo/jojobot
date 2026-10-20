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
/// holds.
#[derive(Default)]
struct Notes {
    teaching: Vec<String>,
    possible_subject: Vec<String>,
    unlocks: Option<serde_json::Value>,
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
    let mut answered = crate::caller::handle_declined("capture", way);
    let mut body = read_body(&answered);
    body["wrote"] = true.into();
    body["landed"] = landed.into();
    answered.content = vec![ContentBlock::text(body.to_string())];
    answered
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

/// Read one pass's answer as a body, and tell a refusal from a success.
fn read_item(answered: &CallToolResult) -> Item {
    let body = read_body(answered);
    if body["status"] == "blocked" {
        let why = body["how_to_proceed"]
            .as_str()
            .unwrap_or("it was refused")
            .trim_start_matches("Nothing was written: ")
            .trim_start_matches("Nothing was written. ")
            .to_string();
        return Item::Refused(why);
    }
    Item::Went(body)
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
        // **The notes the items earn are held back and delivered together**: each
        // item's ledger spends wait in `spends`, and the receipt carries the notes.
        let spends = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let mut notes = Notes::default();
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        for (at, (mut one, label)) in items.into_iter().enumerate() {
            // **An item an earlier call wrote is not written, or judged, again**:
            // the list has moved on since, and a drop it made would be refused
            // against the list it changed. Its address is read back.
            if let Some(address) = jojobot_domain::request::recorded(&part_of(at))
                .as_ref()
                .and_then(address_of)
            {
                let _ = jojobot_domain::request::skip(&part_of(at));
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
        let skipped = jojobot_domain::request::skipped();
        if !skipped.is_empty() {
            body["finished_a_cut_batch"] = serde_json::json!({
                "already_written": skipped.len(),
                "written_now": count - skipped.len(),
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
        let crate::request::Begun::Fresh(scope) =
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
        let cut = sent_under_its_id(&jojobot, batch()).await;
        assert!(
            cut.as_ref()
                .map_or(true, |answered| json_of(answered)["status"] == "blocked"),
            "the cut item fails the call: {cut:?}"
        );
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
}
