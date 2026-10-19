//! `capture` with `entries` or `ticks` — many entries or many ticks on one list
//! in one call.
//!
//! **Every item goes through the one path a single entry or tick goes through**
//! ([`Jojobot::capture_one`], rule 498): first as a check of every item, which
//! writes nothing, and then as the write of each item, one part of the write
//! under the call's `request_id`. A check that refuses names the item by its
//! position. A write that stops partway leaves the id open, and the same call
//! again writes the rest and none twice.

use super::capture::{BATCH_CAP, CaptureArgs, EntryItem, Mode};
use super::*;

/// What one pass of the single path answered.
enum Item {
    /// The pass went on: the answer's body.
    Went(serde_json::Value),
    /// The pass refused: why, in the refusal's own words.
    Refused(String),
}

/// Read one pass's answer as a body, and tell a refusal from a success.
fn read_item(answered: &CallToolResult) -> Item {
    let body: serde_json::Value = answered
        .content
        .first()
        .and_then(|block| block.as_text())
        .and_then(|text| serde_json::from_str(&text.text).ok())
        .unwrap_or_default();
    if body["status"] == "blocked" {
        let why = body["how_to_proceed"]
            .as_str()
            .unwrap_or("it was refused")
            .trim_start_matches("Nothing was written: ")
            .to_string();
        return Item::Refused(why);
    }
    Item::Went(body)
}

impl Jojobot {
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
        for (at, (one, label)) in items.iter().enumerate() {
            if jojobot_domain::request::recorded(&part_of(at)).is_some() {
                continue;
            }
            let checked = self
                .capture_one(
                    one.clone(),
                    Mode {
                        check_only: true,
                        placing_held: true,
                        part: None,
                    },
                )
                .await?;
            if let Item::Refused(why) = read_item(&checked) {
                return refuse(format!(
                    "item {} of {count} ({label}) is refused, so nothing was written: {why}",
                    at + 1
                ));
            }
        }

        let before = self.as_it_stands(&subject).await;
        let mut written: Vec<(usize, String)> = Vec::with_capacity(count);
        let mut teaching = None;
        for (at, (mut one, label)) in items.into_iter().enumerate() {
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
                    },
                )
                .await?;
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
                    let how = if jojobot_domain::request::carries_an_id() && landed > 0 {
                        return Ok(crate::caller::handle_declined(
                            "capture",
                            crate::answer::WayForward::retry(format!(
                                "item {} of {count} ({label}) failed: {why}. {landed} items \
                                 landed and stay. Send the same call again with the same \
                                 request_id and it writes the rest and none twice.",
                                at + 1
                            )),
                        ));
                    } else {
                        format!("{landed} items landed and stay")
                    };
                    return refuse(format!(
                        "item {} of {count} ({label}) failed: {why}. {how}",
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
