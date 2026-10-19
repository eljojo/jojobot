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

/// **The answer for a batch that stopped after `written` landed**, with or
/// without a `request_id`: the items that landed with their words and addresses,
/// and how to finish. It is a refusal of the item that failed, built as every
/// refusal is, and it says `wrote: true` because items did land. One builder for
/// both paths, so they agree field for field on `wrote` and `landed`.
fn half_landed(
    failed: usize,
    count: usize,
    label: &str,
    why: &str,
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
    let said = format!(
        "item {} of {count} ({label}) failed: {why}. {} landed and stay.",
        failed + 1,
        named.join(", "),
    );
    let way: crate::answer::WayForward = match finish {
        Finish::SendTheRest => format!(
            "{said} Sending the same call again would write them a second time. Send only the \
             items from item {} on, or send a request_id with the first call next time so the \
             same call can finish the batch.",
            failed + 1,
        )
        .into(),
        Finish::ResendTheId => crate::answer::WayForward::retry(format!(
            "{said} Send the same call again with the same request_id and it writes the rest \
             and none twice."
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
        let mut teaching = None;
        for (at, (mut one, label)) in items.into_iter().enumerate() {
            // **An item an earlier call wrote is not written, or judged, again**:
            // the list has moved on since, and a drop it made would be refused
            // against the list it changed. Its address is read back.
            if let Some(address) = jojobot_domain::request::recorded(&part_of(at))
                .as_ref()
                .and_then(address_of)
            {
                let _ = jojobot_domain::request::skip(&part_of(at));
                written.push((at, address));
                continue;
            }
            if !ticking && at > 0 {
                one.after = written.last().map(|(_, address)| address.clone());
            }
            let answered = self
                .capture_one(
                    one,
                    Mode {
                        check_only: false,
                        placing_held: true,
                        part: Some(part_of(at)),
                        tick_state: None,
                    },
                )
                .await;
            let answered = match answered {
                Ok(answered) => answered,
                // **A store that fails partway answers as an error, not as a
                // refusal of the item**, and the items before it have landed: the
                // answer says so rather than that nothing was written.
                Err(_) if !written.is_empty() => {
                    return Ok(half_landed(
                        at,
                        count,
                        &label,
                        "the store could not write it",
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
                    if teaching.is_none() {
                        teaching = body.get("teaching").cloned();
                    }
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
                            &why,
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
        if let Some(teaching) = teaching {
            body["teaching"] = teaching;
        }
        let changed = self.changed_on(&before, &subject).await;
        crate::changed::note_changed(&mut body, changed);
        crate::answer::note_postcondition(
            &mut body,
            format!("{count} {form} landed on {subject}, each its own record, in the order sent."),
        );
        json_result(&body)
    }
}
