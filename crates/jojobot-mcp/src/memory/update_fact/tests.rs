use super::*;
use crate::harness::*;
use crate::memory::recall::FollowArgs;
use crate::memory::testing::*;
use crate::session::testing::journal_entry;
use jojobot_domain::memory::types::{Field, ValueType};

/// 🚨 **A rewrite says what it did NOT destroy**, because a caller that
/// does not know keeps its hands off.
///
/// The line said the words a rewrite replaced were "not kept", and while
/// that was true a session met a conflicting claim, would not overwrite it
/// on a guess, and wrote nothing at all. A claim's writes are kept now and
/// a caller can read them back — and a surface that does not say so leaves
/// the caution in place with nothing behind it.
///
/// The needle is the ARGUMENT that reads them, not a sentence: a phrase
/// breaks when the wording improves and proves nothing.
#[tokio::test]
async fn a_rewrite_says_the_words_it_replaced_are_still_readable() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("works at the new place".into()),
                ..update_args("person:alpha#f1")
            }))
            .await
            .expect("update ok"),
    );
    let said = edited["postcondition"]
        .as_str()
        .expect("an edit answers with a postcondition");
    assert!(
        said.contains("history_record"),
        "the receipt does not say how to read what the claim used to say: {said}"
    );
    assert!(
        said.contains("person:alpha#f1"),
        "…and it names the record to ask for: {said}"
    );
    assert!(
        !said.contains("not kept"),
        "the receipt still says the old words are gone, and they are not: {said}"
    );
}

/// 🚨 **Negating a claim about something that already happened names the
/// other path, at the moment somebody is reading a receipt.**
///
/// A run told that somebody was never at an event rewrote the claim in
/// place, twice, in two paid runs — the shape rule 58 taught before it
/// was killed. **The past does not change**, so a claim about a past
/// event turning into its own negation is the one shape where the edit
/// is usually the wrong verb: archive, with retract or an ordinary edit
/// setting the status, is what either reason for it calls for now.
///
/// ⛔️ **It is a line, never a gate.** A word search for a negation can be
/// wrong in either direction, so refusing on it would be worse than the
/// defect it catches.
///
/// ⚠️ **The paired negative is what keeps the line worth reading**: an
/// ordinary rewrite, and a negation of a claim that is not about a past
/// event, both come back without it. A receipt that always says it is a
/// receipt nobody reads.
#[tokio::test]
async fn negating_a_past_event_names_retraction_and_nothing_else_does() {
    let jojobot = handler();
    ensure(&jojobot, "event:leaving-party").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            recorded_at: Some("2026-03-01".into()),
            shape: Some("attendance".into()),
            object: Some("event:leaving-party".into()),
            ..capture_args("alpha", "was at the leaving party")
        },
    )
    .await;
    // An ordinary claim on the same thing, about no event at all.
    capture_ok(
        &jojobot,
        CaptureArgs {
            recorded_at: Some("2026-03-01".into()),
            ..capture_args("alpha", "closes the shop at six")
        },
    )
    .await;

    let said = async |address: &str, content: &str| {
        json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    provenance: Some("inference".to_string()),
                    content: Some(content.into()),
                    ..update_args(address)
                }))
                .await
                .expect("update ok"),
        )["postcondition"]
            .as_str()
            .expect("an edit answers with a postcondition")
            .to_string()
    };

    let negated = said("person:alpha#f1", "was NOT at the leaving party").await;
    assert!(
        negated.contains("retract"),
        "a claim about a past event was negated and the other path went unnamed: {negated}"
    );

    // ⚠️ **The negative, on the same store.** A claim about no event,
    // negated: the world changed, which is what a rewrite is for.
    let ordinary = said("person:alpha#f2", "does NOT close the shop at six").await;
    assert!(
        !ordinary.contains("retract"),
        "every negation names retraction, so the line says nothing: {ordinary}"
    );

    // A contraction says no exactly as the word does, and a caller writes
    // one as readily. **On a claim of its own**, because f1 already says
    // NOT: negating what is already negative adds nothing.
    capture_ok(
        &jojobot,
        CaptureArgs {
            recorded_at: Some("2026-03-01".into()),
            shape: Some("attendance".into()),
            object: Some("event:leaving-party".into()),
            ..capture_args("alpha", "stayed to the end of the leaving party")
        },
    )
    .await;
    let shortened = said("person:alpha#f3", "wasn't there at the end").await;
    assert!(
        shortened.contains("retract"),
        "a contraction negates and went unnoticed: {shortened}"
    );

    // …and an ordinary rewrite of the event claim, which is not a negation.
    let reworded = said("person:alpha#f1", "was at the leaving party, briefly").await;
    assert!(
        !reworded.contains("retract"),
        "a rewrite that negates nothing named retraction: {reworded}"
    );
}

/// **An edit says what it replaced and what it left alone.**
///
/// This is the verb that makes `capture`'s line worth believing. `capture`
/// appends and `update_fact` rewrites in place, so a postcondition that
/// read *nothing was changed* on both would be a false promise on one of
/// them — and a false promise in the one place a caller has been taught to
/// trust is worse than no line at all.
///
/// **Three shapes in one case, because the line is computed from the
/// patch**: a rewrite names the record it replaced and counts what it left
/// alone; a clear names the keys it took off; a rewrite that clears nothing
/// names none. A constant cannot produce all three.
#[tokio::test]
async fn an_update_says_what_it_replaced_and_what_it_left_alone() {
    let jojobot = handler();
    let first = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("mood".to_string(), "delighted".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:alpha", "said the kiln was lit")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        capture_args("person:alpha", "said the flue was the problem"),
    )
    .await;
    let address = address_of(&first);

    let rewritten = update_ok(
        &jojobot,
        UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("said the kiln was NOT lit — confirmed".into()),
            ..update_args(&address)
        },
    )
    .await;
    let replaced = postcondition_line(&rewritten);
    assert!(
        replaced.contains(&address),
        "the line has to name the record this call rewrote: {rewritten}",
    );
    assert!(
        replaced.contains('1'),
        "…and how much on this thing it left alone: {rewritten}",
    );
    assert!(
        !replaced.contains("mood"),
        "this call took no key off, so the line names none: {rewritten}",
    );

    let cleared = update_ok(
        &jojobot,
        UpdateFactArgs {
            clear_fields: Some(vec!["mood".into()]),
            ..update_args(&address)
        },
    )
    .await;
    assert!(
        postcondition_line(&cleared).contains("mood"),
        "this call DID remove something, and a line that cannot say so is the false promise \
             the computed line exists to avoid: {cleared}",
    );
}

/// The postcondition line of a receipt, which every write carries.
fn postcondition_line(body: &serde_json::Value) -> String {
    body["postcondition"]
        .as_str()
        .unwrap_or_else(|| panic!("a write states what now stands: {body}"))
        .to_string()
}

/// Update through the handler, expecting the guard to wave it through.
async fn update_ok(jojobot: &Jojobot, args: UpdateFactArgs) -> serde_json::Value {
    let body = json_of(
        &jojobot
            .update_fact(Parameters(args))
            .await
            .expect("update_fact ok"),
    );
    assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
    body
}

/// **The clause about what was left alone reads as English, and vanishes
/// when there is nothing to leave alone.**
///
/// A real model read *"The 0 other records on thing:floor-pump are as they
/// were"* off this line in a paid run — a reassurance about an empty set,
/// on the one call in that year where the agent overwrote an account it
/// should have kept. **Three edges, because the count decides both the
/// noun and the verb**, and a line that announces itself as generated
/// spends the trust it was added to build.
#[tokio::test]
async fn what_an_edit_left_alone_reads_as_english_at_every_count() {
    let jojobot = handler();
    let only = capture_ok(
        &jojobot,
        capture_args("person:alpha", "said the kiln was lit"),
    )
    .await;

    // Nothing else stands on the thing, so the clause is not there at all.
    let alone = update_ok(
        &jojobot,
        UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("said the kiln was NOT lit".into()),
            ..update_args(&address_of(&only))
        },
    )
    .await;
    let line = postcondition_line(&alone);
    assert!(
        !line.contains(" 0 "),
        "a reassurance about an empty set: {line}",
    );

    capture_ok(
        &jojobot,
        capture_args("person:alpha", "and the flue was blocked"),
    )
    .await;
    let one = postcondition_line(
        &update_ok(
            &jojobot,
            UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the kiln was cold all week".into()),
                ..update_args(&address_of(&only))
            },
        )
        .await,
    );
    assert!(
        one.contains("1 other record is") || one.contains("1 other record on person:alpha is"),
        "singular noun with a plural verb: {one}",
    );

    capture_ok(&jojobot, capture_args("person:alpha", "and the door stuck")).await;
    let two = postcondition_line(
        &update_ok(
            &jojobot,
            UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the kiln is lit again".into()),
                ..update_args(&address_of(&only))
            },
        )
        .await,
    );
    assert!(
        two.contains("2 other records") && two.contains("are as they were"),
        "…and the plural still has to be plural: {two}",
    );
}

/// **A field is set and cleared in place, and a plain recall shows it.**
///
/// Edit-in-place is the surface the model puts in front of an agent: it
/// edits and it sees the record change. This is that surface reaching the
/// one part of a record it could not reach — a record's fields could only
/// be written by the call that created it, so a key that turned out wrong
/// meant a second record beside the first.
///
/// **Set and clear are separate arguments** rather than one bag where an
/// empty value means "remove". An empty value is a value somebody wrote,
/// and the two moves must not be spelled the same.
/// **A clear that would drop the thing below a type it answers is blocked,
/// and the same clear on a thing that answers nothing goes through.**
///
/// Strict is a floor: what a type asks for has to survive. The pairing is
/// the point — a build that refused both would pass the first half and
/// make a half-described thing unrepairable, which is the failure the rule
/// is shaped to avoid.
#[tokio::test]
async fn a_clear_that_would_break_a_fit_is_blocked_and_says_what_it_would_cost() {
    let jojobot = handler();
    writing_as(&jojobot);
    // **A KIND, not a declared type.** What a write may take off a thing is
    // its own kind's question; a declared type is the vocabulary a caller
    // asks with and gates nothing.
    jojobot
        .memory
        .declare_kind(
            "thing",
            jojobot_domain::memory::types::Origin::Shipped,
            vec![
                Field::required("cost", ValueType::Number),
                Field::required("done_on", ValueType::Date),
            ],
        )
        .await
        .expect("a kind may name the keys its things keep");
    // A shipped kind rather than a new one: declaring a new kind fills the
    // set this process parses against, and every case beside this one that
    // stands a store up empties it again.

    let whole = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [
                    ("cost".to_string(), "40".to_string()),
                    ("done_on".to_string(), "2026-04-18".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("thing:gravel-bike", "the annual service")
        },
    )
    .await;
    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["cost".into()]),
                ..update_args(&address_of(&whole))
            }))
            .await
            .expect("a refusal is an answer, not a protocol failure"),
    );
    let advice = refused["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal carries its way forward: {refused}"));
    assert!(
        advice.contains("thing") && advice.contains("cost"),
        "the refusal names the kind and the key it would cost: {advice}"
    );
    assert_eq!(refused["wrote"], false, "{refused}");

    // The same clear, on a thing that answers no type: served. Nothing
    // here is protecting anything, and a record nobody can repair is worse
    // than a record with a key missing.
    let loose = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("cost".to_string(), "40".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("thing:road-bike", "somebody wrote down a price")
        },
    )
    .await;
    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["cost".into()]),
                ..update_args(&address_of(&loose))
            }))
            .await
            .expect("update ok"),
    );
    assert_ne!(
        edited["status"], "blocked",
        "a thing that fits nothing has nothing to protect: {edited}"
    );
    assert_eq!(
        edited["fields_count"], 0,
        "…so the key comes off, and the count is what says so: {edited}"
    );
}

#[tokio::test]
async fn update_fact_sets_and_clears_a_field() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [
                    ("cost".to_string(), "40".to_string()),
                    ("done_on".to_string(), "2026-04-18".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("alpha", "the annual service")
        },
    )
    .await;
    let address = address_of(&captured);

    let set = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("cost".to_string(), "45".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    // **The receipt counts the keys rather than reading them back.** Two
    // is the answer to both halves at once: the key named was rewritten
    // rather than added, and the key the patch did not name is still
    // there. What each one HOLDS is read below, off the record.
    assert_eq!(set["fields_count"], 2, "{set}");

    let cleared = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["done_on".into()]),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        cleared["fields_count"], 1,
        "the key named is gone and the other is not: {cleared}"
    );

    // …and both moves are on the record a later read takes, which is what
    // makes editing in place true rather than an answer's shape.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        recalled["objects"][0]["facts"][0]["fields"],
        serde_json::json!({"cost": "45"}),
        "{recalled}"
    );
}

/// **The same mechanism, through a patch instead of a fresh capture.**
///
/// An edit is a write like any other, and the due moment it can move
/// does not care whether the write that moved it was a capture or a
/// patch — the case `capture.rs`'s own version of this proves for a
/// fresh record, this one proves for an existing one edited in place.
///
/// **The record starts as a plain testimony claim with no schedule on
/// it at all**, so the provenance this test reads is genuinely this
/// patch's own doing — not, as an earlier draft of this case got wrong,
/// a record that was already forced to inference the moment it was
/// captured with a complete schedule on it.
#[tokio::test]
async fn an_edit_that_moves_the_cadence_keeps_the_due_moment_current() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "we should descale this regularly")
        },
    )
    .await;
    let address = address_of(&captured);

    // The patch ADDS the whole schedule at once, onto the SAME record —
    // this is the patch's own provenance forcing, not inherited.
    let opened = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        opened["provenance"], "inference",
        "the patch that completes the schedule computes a due moment, and overrides the \
             provenance this call named none of: {opened}",
    );

    // Now the case that matters: ONE input changes, on a second patch.
    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("cadence_days".to_string(), "14".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        edited["provenance"], "inference",
        "a moved due moment overrides the caller's own provenance on a patch too, though \
             this call named none of its own: {edited}",
    );

    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("rhythm:descale")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        read["objects"][0]["fields"]["due_on"], "2026-08-15",
        "the cadence moved through a patch, not a fresh capture, and the stored due moment \
             moved with it: {read}",
    );
}

/// **The mover could only ever say "here is a new date" — this proves
/// the other half, that it can also say "remove it".**
///
/// **Paired with the case above**: that one proves a changed cadence
/// still MOVES the due moment; this one proves a cleared one REMOVES
/// it — a build that only ever clears the value on every write would
/// pass this case and fail that one, and a build that only ever moves
/// it forward would pass that one and fail this.
///
/// The clear names `counts_from`, which flips the rhythm's answer from
/// `Due::On` to `Due::NotYetOpened` — the half-opened shape, not an
/// absent schedule — so the stale date left over from before the clear
/// is exactly the value nothing but this mechanism would ever remove.
#[tokio::test]
async fn clearing_a_schedule_key_removes_the_stale_due_moment() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "we should descale this regularly")
        },
    )
    .await;
    let address = address_of(&captured);

    update_ok(
        &jojobot,
        UpdateFactArgs {
            fields: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                    ("counts_from".to_string(), "2026-08-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..update_args(&address)
        },
    )
    .await;
    let opened = json_of(
        &jojobot
            .recall(Parameters(recall_args("rhythm:descale")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        opened["objects"][0]["fields"]["due_on"], "2026-08-08",
        "the schedule this case clears has to actually be stored first: {opened}",
    );

    update_ok(
        &jojobot,
        UpdateFactArgs {
            clear_fields: Some(vec!["counts_from".into()]),
            ..update_args(&address)
        },
    )
    .await;
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("rhythm:descale")))
            .await
            .expect("recall ok"),
    );
    assert!(
        read["objects"][0]["fields"].get("due_on").is_none(),
        "counts_from is gone, so the schedule reads NotYetOpened now — the due moment from \
             before the clear is stale and must not survive the write: {read}",
    );

    // Paired with the field-level check: the real attention read agrees
    // too, not only the stored key.
    let overdue = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                overdue: Some(super::recall::OverdueArgs {
                    as_of: Some("2026-12-31".into()),
                }),
                ..recall_args("rhythm:descale")
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        overdue["objects"]
            .as_array()
            .expect("objects is an array")
            .is_empty(),
        "a rhythm with no schedule is never owed, whatever a stale field used to say: \
             {overdue}",
    );
}

/// **Clearing `cadence_days` alone is refused, and clearing it with
/// `advances_from` leaves `counts_from` behind, which reads overdue rather
/// than never-due.** The postcondition names that key, and clearing it
/// too, as the receipt says to, actually reaches never-due — proven here
/// rather than asserted.
#[tokio::test]
async fn a_schedule_cleared_in_part_names_what_is_left_and_the_act_that_finishes_it() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "we should descale this regularly")
        },
    )
    .await;
    let address = address_of(&captured);

    update_ok(
        &jojobot,
        UpdateFactArgs {
            fields: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                    ("counts_from".to_string(), "2026-08-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..update_args(&address)
        },
    )
    .await;

    // **One trigger key cleared alone is refused**: it would leave the other
    // key holding half a schedule. Both together are a legal write, and it
    // leaves `counts_from` behind.
    let refused = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["cadence_days".into()]),
                ..update_args(&address)
            }))
            .await
            .expect("a refusal is an answer"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    let half_cleared = update_ok(
        &jojobot,
        UpdateFactArgs {
            clear_fields: Some(vec!["cadence_days".into(), "advances_from".into()]),
            ..update_args(&address)
        },
    )
    .await;
    let said = postcondition_line(&half_cleared);
    assert!(
        said.contains("counts_from") && !said.contains("advances_from, counts_from"),
        "the postcondition must name the schedule key this write left behind: {said}",
    );

    // The rhythm now reads overdue with the fields it still holds,
    // exactly as a half-built schedule always has — not the quiet shape
    // the caller likely meant to reach.
    let after_half_clear = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                overdue: Some(super::recall::OverdueArgs {
                    as_of: Some("2026-08-02".into()),
                }),
                ..recall_args("rhythm:descale")
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        !after_half_clear["objects"]
            .as_array()
            .expect("objects is an array")
            .is_empty(),
        "half a schedule reads overdue rather than quiet: {after_half_clear}",
    );

    // The finishing act the postcondition points at: clear what is left.
    update_ok(
        &jojobot,
        UpdateFactArgs {
            clear_fields: Some(vec!["counts_from".into()]),
            ..update_args(&address)
        },
    )
    .await;
    let finished = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                overdue: Some(super::recall::OverdueArgs {
                    as_of: Some("2026-08-02".into()),
                }),
                ..recall_args("rhythm:descale")
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        finished["objects"]
            .as_array()
            .expect("objects is an array")
            .is_empty(),
        "clearing the keys the postcondition named actually reaches never-due: {finished}",
    );
}

/// **`counts_from` is not a trigger key.** Clearing it alone reaches
/// `Due::NotYetOpened` — the correct, quiet shape a declared,
/// not-yet-opened loop takes — and the postcondition must stay silent
/// about it, exactly as [`clearing_a_schedule_key_removes_the_stale_due_moment`]
/// already proves the due moment itself is removed rather than left
/// stale.
#[tokio::test]
async fn clearing_counts_from_alone_gets_no_half_cleared_advisory() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "we should descale this regularly")
        },
    )
    .await;
    let address = address_of(&captured);

    update_ok(
        &jojobot,
        UpdateFactArgs {
            fields: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                    ("counts_from".to_string(), "2026-08-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..update_args(&address)
        },
    )
    .await;

    let cleared = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["counts_from".into()]),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    let said = postcondition_line(&cleared);
    assert!(
        !said.contains("cadence_days") && !said.contains("advances_from"),
        "counts_from alone reaches the correct quiet NotYetOpened shape, so the \
             half-cleared advisory must not fire: {said}",
    );
}

/// **The key jojobot writes itself is refused here too.** A verb that
/// could set `retracts` would be a way to mark somebody else's record
/// taken back without going through the verb that decides whether it may
/// be — so the gate is on both write paths, not on the one somebody
/// thought of first.
#[tokio::test]
async fn update_fact_refuses_the_reserved_key() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("retracts".to_string(), "person:alpha#f1".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .expect("advice")
            .contains("retracts"),
        "the refusal names the key: {refused}"
    );
}

/// 🚨 **`clear_edge` takes the edge off, and an edit that never mentions
/// edges leaves it alone** — through the surface a caller holds.
///
/// **The second half is the load-bearing one.** A build where every rewrite
/// silently dropped the edge would pass the first and be a worse defect
/// than the one this fixes: correcting a typo would cost the link and
/// nobody would be told.
///
/// The rest of the patch still lands, so an empty edge is the argument
/// doing its work rather than the whole edit failing quietly.
#[tokio::test]
async fn clear_edge_takes_the_edge_off_and_a_silent_edit_does_not() {
    let jojobot = handler();
    ensure(&jojobot, "event:winter-fest").await;
    let drawn = |said: &str| CaptureArgs {
        shape: Some("attendance".into()),
        object: Some("event:winter-fest".into()),
        ..capture_args("person:alpha", said)
    };

    let kept = capture_ok(&jojobot, drawn("was at the winter fest")).await;
    let reworded = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("was at the winter fest, both nights".into()),
                ..update_args(&address_of(&kept))
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        reworded["edge"]["object"], "event:winter-fest",
        "an edit that never mentioned edges took one off: {reworded}",
    );

    let wrong = capture_ok(&jojobot, drawn("was at the winter fest")).await;
    let corrected = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("was never at the winter fest".into()),
                clear_edge: Some(true),
                ..update_args(&address_of(&wrong))
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        corrected["edge"].is_null(),
        "the edge is still on a claim that now denies it: {corrected}",
    );
    assert_eq!(
        corrected["status"], "active",
        "taking the edge off is not a retraction: {corrected}",
    );
    assert_eq!(
        corrected["content_bytes"].as_u64(),
        Some("was never at the winter fest".len() as u64),
        "the rewrite itself did not land: {corrected}",
    );
}

/// `update_fact` attaches an edge to a fact that didn't have one.
#[tokio::test]
async fn update_fact_attaches_an_edge() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "was at the festival")).await;
    assert!(captured["edge"].is_null());
    ensure(&jojobot, "event:winter-fest").await;

    let updated = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                shape: Some("attendance".into()),
                object: Some("event:winter-fest".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(updated["edge"]["type"], "attendee");
    assert_eq!(updated["edge"]["object"], "event:winter-fest");
}

/// 🚨 **A correction can carry the day it was made, and omitting it leaves
/// the record's day untouched.**
///
/// `update_fact` had no way to say when a rewrite happened, so a claim
/// captured under one day and corrected under a later one kept the
/// original day forever — the one thing a later reader most wants about a
/// correction had no argument to carry it. `retract` already took a date
/// for exactly this reason; this is the same argument on the verb the
/// caller is actually sent to for the common case of correcting a claim
/// about what is true now.
///
/// **Both halves.** A day given is the day carried, and no day given
/// leaves the day the claim already held — without the second, a build
/// that always overwrote the day with today (or dropped the argument on
/// the floor) would satisfy the first alone.
#[tokio::test]
async fn a_correction_carries_the_day_it_is_given_and_leaves_it_otherwise() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            recorded_at: Some("2026-06-01".into()),
            ..capture_args("alpha", "the club meets on Tuesdays")
        },
    )
    .await;
    let address = address_of(&captured);
    assert_eq!(captured["recorded_at"], "2026-06-01");

    let redated = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Wednesdays".into()),
                recorded_at: Some("2026-08-15".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        redated["recorded_at"], "2026-08-15",
        "a correction given a day must carry that day rather than the day of the claim it \
             replaces: {redated}"
    );

    let untouched = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Thursdays".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        untouched["recorded_at"], "2026-08-15",
        "an edit naming no day must leave the record's existing day alone: {untouched}"
    );
}

/// **The receipt says when an explicit `recorded_at` differs from the
/// run's own stated day, and stays silent on every other shape** —
/// mirrors `capture`'s own case, on the edit verb.
#[tokio::test]
async fn the_update_receipt_names_a_recorded_at_that_differs_from_the_runs_day() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;

    let booted = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-03-15".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
            }))
            .await
            .expect("the boot call is ok"),
    );
    let acting = sid_of(&booted).expect("a handle");

    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(acting.clone()),
            ..capture_args("alpha", "the club meets on Tuesdays")
        },
    )
    .await;
    let address = address_of(&captured);

    // A different day: the sentence names both.
    let differing = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Wednesdays".into()),
                recorded_at: Some("2026-03-01".into()),
                sid: Some(acting.clone()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    let note = differing["recorded_at_note"]
        .as_str()
        .unwrap_or_else(|| panic!("a sentence naming both days: {differing}"));
    assert!(
        note.contains("2026-03-01") && note.contains("2026-03-15"),
        "{note}"
    );

    // The same day as the run: silent.
    let same = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Thursdays".into()),
                recorded_at: Some("2026-03-15".into()),
                sid: Some(acting.clone()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(same["recorded_at_note"].is_null(), "{same}");

    // No explicit day at all: silent.
    let omitted = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Fridays".into()),
                sid: Some(acting.clone()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(omitted["recorded_at_note"].is_null(), "{omitted}");

    // A run with no stated day — `update_args`'s own default sid, bound
    // to no day by `writing_as` — is silent even naming a day that would
    // otherwise disagree.
    let no_stated_day = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("the club meets on Saturdays".into()),
                recorded_at: Some("2026-01-01".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        no_stated_day["recorded_at_note"].is_null(),
        "{no_stated_day}"
    );
}

/// **A refutation is a content edit, and `negated` is refused by name.** The
/// rewritten row stays `active` and keeps its address — the negative truth is
/// the current truth, so it has to be what a plain read returns. Asking for
/// the retired status is a client error that says what to do instead, rather
/// than an alias that would file the correction where nobody looks.
#[tokio::test]
async fn a_refutation_is_a_content_edit_and_negated_is_refused() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        capture_args("alpha", "a close contact of the user"),
    )
    .await;

    let err = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("negated".into()),
            ..update_args(&address_of(&captured))
        }))
        .await
        .expect_err("the retired status must be refused, not aliased");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.contains("rewrite"),
        "the error must say what to do instead: {}",
        err.message
    );
    assert!(
        err.message.contains("retract"),
        "a past-event negation is retract's case, and the refusal that hands out the \
             rewrite instruction to every caller never carves that out: {}",
        err.message
    );

    let updated = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("NOT a close contact — do not re-infer".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect("the refutation is an ordinary edit"),
    );
    assert_eq!(
        updated["status"], "active",
        "the negative truth is the truth"
    );
    assert_eq!(
        updated["content_elided"], true,
        "the refutation is not read back to whoever just wrote it: {updated}"
    );
    assert_eq!(
        updated["address"], "person:alpha#f1",
        "the row keeps its address"
    );
    // **The edit is proven where it landed, and the receipt cannot prove
    // it.** The answer says the write happened; only a read says what the
    // record now holds — and a case named for a content edit that asserts
    // nothing about the content passes on a build where the edit is
    // dropped.
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        read["objects"][0]["facts"][0]["content"], "NOT a close contact — do not re-infer",
        "the refutation is what the record says now: {read}"
    );
}

/// 🚨 **`archived` covers a claim that was merely edited, not only one
/// taken back — so a walk's `retracted` marker now fires on both, and its
/// wording has to stop claiming to know which.** When a replacement is
/// reachable through `derived_from`, the note has to say so rather than
/// telling a reader there is nothing to act on.
#[tokio::test]
async fn a_walks_retracted_note_names_the_path_to_a_replacement_when_one_exists() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    ensure(&jojobot, "event:birthday-party").await;
    ensure(&jojobot, "person:beta").await;
    let original = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            shape: Some("attendance".into()),
            object: Some("event:birthday-party".into()),
            ..capture_args("person:beta", "was at the party")
        },
    )
    .await;
    let address = address_of(&original);
    update_ok(
        &jojobot,
        UpdateFactArgs {
            status: Some("archived".into()),
            ..update_args(&address)
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            derived_from: Some(address.clone()),
            ..capture_args("person:beta", "actually only stopped by for cake")
        },
    )
    .await;

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                follow: Some(FollowArgs {
                    shape: Some("attendance".into()),
                    relation: None,
                    direction: Some("in".into()),
                    depth: None,
                    keeping: None,
                    fits_type: None,
                }),
                sid: Some(sid.clone()),
                ..recall_args("event:birthday-party")
            }))
            .await
            .expect("a walk from the party"),
    );
    let note = walked["objects"][0]["connected"]
            .as_array()
            .unwrap_or_else(|| panic!("the walk reached its guest: {walked}"))
            .iter()
            .find(|o| o["id"] == "person:beta")
            .unwrap_or_else(|| panic!("person:beta was not reached at all: {walked}"))["via"]
            ["retracted"]
            .as_str()
            .unwrap_or_else(|| panic!("the edited claim's link carries no note at all: {walked}"))
            .to_string();
    assert!(
        note.contains("derived_from"),
        "a replacement is reachable through derived_from and the note gives no path to it: \
             {note}"
    );
}

/// Promotion to testimony needs the explicit confirmation flag.
#[tokio::test]
async fn promoting_to_testimony_requires_the_confirmation_flag() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "prefers mornings")).await;
    let promote = |confirmed: Option<bool>| UpdateFactArgs {
        provenance: Some("testimony".into()),
        confirmed_by_user: confirmed,
        ..update_args(&address_of(&captured))
    };

    // Refused as a blocked ANSWER: the call is well formed and jojobot is
    // declining to bless a claim the operator has not blessed, which is a
    // next move rather than a failure (rule 68).
    let refused = blocked(
        &jojobot
            .update_fact(Parameters(promote(None)))
            .await
            .expect("an unconfirmed promotion is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    let advice = refused["how_to_proceed"].as_str().expect("advice");
    assert!(
        advice.contains("confirmed_by_user"),
        "the way forward names the flag the operator's word unlocks: {advice}"
    );

    let ok = json_of(
        &jojobot
            .update_fact(Parameters(promote(Some(true))))
            .await
            .expect("a confirmed promotion is allowed"),
    );
    assert_eq!(ok["provenance"], "testimony");
}

/// **The confirmation gate is on PROMOTION, not on assertion**, and this
/// is the scope the surface has to state.
///
/// `check_standing` fires on one move only: an existing OPEN claim being
/// made SETTLED. A fresh capture may declare `settled` on an inference and
/// is accepted — standing is declared on honour exactly as provenance is,
/// and the gate is on promotion rather than on assertion, by design.
///
/// Both halves in one read. The refusal alone reads as "settling is
/// guarded" and the acceptance alone reads as a hole in the gate; it is
/// the pair that says where the line actually is, and a reader of the
/// surface who has only one of them believes the wrong thing.
#[tokio::test]
async fn the_confirmation_gate_is_on_promotion_and_not_on_assertion() {
    let jojobot = handler();

    // Asserted, and ungated: nobody is asked to confirm this.
    let asserted = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("inference".into()),
            standing: Some("settled".into()),
            ..capture_args("alpha", "shuts early on sundays")
        },
    )
    .await;
    assert_eq!(asserted["provenance"], "inference", "{asserted}");
    assert_eq!(
        asserted["standing"], "settled",
        "a capture declares its standing on honour: {asserted}"
    );

    // Promoted, and gated: the operator hedged this one, so only the
    // operator withdraws the hedge.
    let hedged = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            standing: Some("open".into()),
            ..capture_args("alpha", "thinks the ferry moved")
        },
    )
    .await;
    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                standing: Some("settled".into()),
                ..update_args(&address_of(&hedged))
            }))
            .await
            .expect("an unconfirmed settling is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
}

/// **A malformed address and a missed one are different answers**, and
/// never a new fact. Malformed is the caller writing something that is not
/// an address at all — a protocol error. Missed is a well-formed address
/// naming nothing, which is the same "you named what does not exist" every
/// gate answers, so it wears the blocked shape and carries the addresses
/// that do exist.
#[tokio::test]
async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "the only fact here")).await;

    let err = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("nope".into()),
            ..update_args("not-an-address")
        }))
        .await
        .expect_err("a string that is no address is a malformed call");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

    let missed = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("nope".into()),
                ..update_args("person:alpha#f99")
            }))
            .await
            .expect("an address that names nothing is an answer, not a protocol failure"),
    );
    assert_eq!(missed["attempted"], "person:alpha#f99");
    let advice = missed["how_to_proceed"].as_str().expect("advice");
    assert!(
        advice.contains("person:alpha#f1"),
        "the addresses that DO exist are what makes this repairable: {advice}"
    );
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args("alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        body["objects"][0]["facts"].as_array().unwrap().len(),
        1,
        "nothing was created"
    );
}

/// An unknown status token is a client error, not a silently-active fact.
#[tokio::test]
async fn an_unknown_status_is_a_client_error() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;
    let err = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("retired".into()),
            ..update_args(&address_of(&captured))
        }))
        .await
        .expect_err("must reject an unknown status");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
}

/// 🚨 **The made-on field's served description must not carry the
/// happened-on field's definition.** `recorded_at` answers when a claim
/// was said, decided or worked out; `happened_at` answers when the thing
/// it describes occurred. "True of [a day]" is this codebase's own
/// phrase for the second question — a description of `recorded_at` that
/// uses it is printing the wrong field's definition on this field's
/// name.
///
/// Checked on both surfaces a caller can read it from: the argument's own
/// schema description and the tool-level prose.
#[test]
fn the_recorded_at_argument_does_not_carry_happened_ats_definition() {
    let tools = Jojobot::tool_router().list_all();
    let update_fact = tools
        .iter()
        .find(|t| t.name.as_ref() == "update_fact")
        .expect("update_fact is a tool");
    let schema = serde_json::to_value(&update_fact.input_schema).expect("the schema serializes");
    let recorded_at = schema["properties"]["recorded_at"]["description"]
        .as_str()
        .expect("recorded_at carries its own description")
        .to_lowercase();
    assert!(
        !recorded_at.contains("true of"),
        "the made-on field's schema description carries the happened-on field's definition: \
             {recorded_at}"
    );
    let tool_description = update_fact.description.as_deref().unwrap_or_default();
    assert!(
        !tool_description.to_lowercase().contains("true of"),
        "the tool-level description carries the happened-on field's definition on the \
             wrong field: {tool_description}"
    );
}

/// **A session whose whole claim-writing life is edits still learns the
/// subject/purpose convention.** `capture` taught it; `update_fact` did
/// not, so a session that only ever edits — the one write path that
/// actually takes `fields`/`clear_fields` and can write those keys —
/// never met it.
#[tokio::test]
async fn a_session_that_only_ever_edits_is_taught_the_subject_convention() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "plays go")).await;

    make_bot(&jojobot, "gamma").await;
    let sid = booted(&jojobot, "gamma").await;

    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("plays go and chess".into()),
                sid: Some(sid),
                ..update_args("person:alpha#f1")
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        edited["teaching"]
            .as_array()
            .expect("a list")
            .contains(&serde_json::json!(CLAIM_SUBJECT_TEACHING)),
        "a session that only edits never learns the subject/purpose convention: {edited}"
    );
}

/// 🚨 **A mark set through `update_fact` reads back through `recall`, and
/// an ordinary field wearing the same name does not become one.**
///
/// Paired in the same case, per the dispatch: without the second half, a
/// build where any key named `stands_for` counted as the mark would pass
/// the first half alone.
#[tokio::test]
async fn a_mark_set_through_update_fact_reads_back_and_an_ordinary_field_is_not_it() {
    let jojobot = handler();
    let first = capture_ok(&jojobot, capture_args("alpha", "said the kiln was lit")).await;
    let second = capture_ok(&jojobot, capture_args("alpha", "and the flue was blocked")).await;
    let synthesis = capture_ok(
        &jojobot,
        capture_args("alpha", "the kiln trouble is resolved now"),
    )
    .await;
    let first_address = address_of(&first);
    let second_address = address_of(&second);
    let synthesis_address = address_of(&synthesis);

    let written = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                stands_for: Some(vec![first_address.clone(), second_address.clone()]),
                ..update_args(&synthesis_address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        written["stands_for"],
        serde_json::json!([first_address, second_address]),
        "the receipt does not show the mark it was just given: {written}"
    );

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    let facts = recalled["objects"][0]["facts"].as_array().expect("facts");
    let synthesis_read = facts
        .iter()
        .find(|f| f["address"] == synthesis_address)
        .expect("the synthesis record is still there");
    assert_eq!(
        synthesis_read["stands_for"],
        serde_json::json!([first_address, second_address]),
        "a plain recall does not show the mark that was set: {synthesis_read}"
    );

    // An ordinary field of the same name, on a record that never got the
    // mark: it stays a field, and the dedicated line stays empty.
    let plain = capture_ok(&jojobot, capture_args("alpha", "an unrelated claim")).await;
    let plain_address = address_of(&plain);
    let with_field = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("stands_for".to_string(), "not a mark".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&plain_address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        with_field["stands_for"],
        serde_json::json!([]),
        "an ordinary field named stands_for was read as the mark: {with_field}"
    );
}

/// **A mark must name claims that exist**, exactly as `derived_from` does
/// — refused with the addresses that do exist, never a new fact.
#[tokio::test]
async fn a_stands_for_must_name_claims_that_exist() {
    let jojobot = handler();
    let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
    let source_address = address_of(&source);
    let synthesis = capture_ok(&jojobot, capture_args("alpha", "so the crossing is longer")).await;

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                stands_for: Some(vec!["person:alpha#f99".into()]),
                ..update_args(&address_of(&synthesis))
            }))
            .await
            .expect("a miss is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .is_some_and(|advice| advice.contains(&source_address)),
        "the addresses that DO exist are what makes it repairable: {refused}"
    );
}

/// ⭐ **An empty set is refused rather than stored** — a mark with no
/// sources would read back as an ordinary record, indistinguishable from
/// one that never got a mark at all.
#[tokio::test]
async fn stands_for_refuses_an_empty_set() {
    let jojobot = handler();
    let synthesis = capture_ok(&jojobot, capture_args("alpha", "a record on its own")).await;

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                stands_for: Some(vec![]),
                ..update_args(&address_of(&synthesis))
            }))
            .await
            .expect("an empty set is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
}

/// A mark naming its own record's address is refused.
#[tokio::test]
async fn stands_for_refuses_its_own_address() {
    let jojobot = handler();
    let synthesis = capture_ok(&jojobot, capture_args("alpha", "a record on its own")).await;
    let address = address_of(&synthesis);

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                stands_for: Some(vec![address.clone()]),
                ..update_args(&address)
            }))
            .await
            .expect("self-reference is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
}

/// `clear_stands_for` takes the mark off, and the sources it named stay
/// exactly as they were — active, and readable on their own address.
#[tokio::test]
async fn clear_stands_for_takes_the_mark_off_and_leaves_the_sources_alone() {
    let jojobot = handler();
    let source = capture_ok(&jojobot, capture_args("alpha", "said the kiln was lit")).await;
    let source_address = address_of(&source);
    let synthesis = capture_ok(&jojobot, capture_args("alpha", "resolved now")).await;
    let synthesis_address = address_of(&synthesis);

    update_ok(
        &jojobot,
        UpdateFactArgs {
            stands_for: Some(vec![source_address.clone()]),
            ..update_args(&synthesis_address)
        },
    )
    .await;

    let cleared = update_ok(
        &jojobot,
        UpdateFactArgs {
            clear_stands_for: Some(true),
            ..update_args(&synthesis_address)
        },
    )
    .await;
    assert_eq!(
        cleared["stands_for"],
        serde_json::json!([]),
        "the mark is still there after clear_stands_for: {cleared}"
    );

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    let source_read = recalled["objects"][0]["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .find(|f| f["address"] == source_address)
        .expect("the source record is still there");
    assert_eq!(
        source_read["status"], "active",
        "the source is untouched by the mark being cleared: {source_read}"
    );
}

/// 🚨 **Discoverability: the verb's own description names the
/// argument.** A capability whose only path is that somebody read the
/// diff has no path.
#[test]
fn stands_for_is_named_on_the_verbs_own_description() {
    let tools = Jojobot::tool_router().list_all();
    let update_fact = tools
        .iter()
        .find(|t| t.name.as_ref() == "update_fact")
        .expect("update_fact is a tool");
    let tool_description = update_fact.description.as_deref().unwrap_or_default();
    assert!(
        tool_description.contains("stands_for"),
        "the tool-level description does not name the argument: {tool_description}"
    );
    let schema = serde_json::to_value(&update_fact.input_schema).expect("the schema serializes");
    assert!(
        schema["properties"]["stands_for"]["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()),
        "stands_for carries no schema description of its own: {schema}"
    );
}

/// 🚨 **Discoverability: the verb's own description names `keep`.** A
/// capability whose only path is that somebody read the diff has no
/// path — and this is the one capability this verb has that exists
/// specifically to replace an undiscoverable accident.
#[test]
fn keep_is_named_on_the_verbs_own_description() {
    let tools = Jojobot::tool_router().list_all();
    let update_fact = tools
        .iter()
        .find(|t| t.name.as_ref() == "update_fact")
        .expect("update_fact is a tool");
    let tool_description = update_fact.description.as_deref().unwrap_or_default();
    assert!(
        tool_description.contains("keep"),
        "the tool-level description does not name the argument: {tool_description}"
    );
    let schema = serde_json::to_value(&update_fact.input_schema).expect("the schema serializes");
    assert!(
        schema["properties"]["keep"]["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()),
        "keep carries no schema description of its own: {schema}"
    );
}

/// **The write says it landed, never that it failed, when only the fold
/// behind it could not confirm it** (rule 130) — `update_fact`'s own
/// catch of `MemoryError::FoldBehind`, the same shape `capture`'s own
/// case proves.
#[tokio::test]
async fn an_update_whose_fold_could_not_refresh_answers_landed_not_failed() {
    let jojobot = Jojobot::new(
        Arc::new(FoldBehindMemory(Arc::new(InMemoryMemory::booted()))),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    capture_ok(
        &jojobot,
        capture_args("person:alpha", "works at the old place"),
    )
    .await;

    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("works at the new place".into()),
                ..update_args("person:alpha#f1")
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(edited["fold"]["behind"], "stale", "{edited}");
    // **The positive half.** The edited record still comes back, exactly
    // as an ordinary update does.
    assert_eq!(edited["content_head"], "works at the new place", "{edited}");
}

/// 🚨 **A record can carry a caller-chosen mark, riding the fields bag it
/// already has** — rule 149: a per-claim marker with no column of its own
/// goes in the hatch. No new column, no new type: `capture` sets it like
/// any other field, and `update_fact` changes it like any other field.
/// Paired against the positive on purpose: a record that always read
/// back marked would pass the negative half alone.
///
/// **Nothing reads this key yet.** It rides the existing, generic fields
/// mechanism unchanged; this proves the round-trip a future cap will
/// need, and nothing more — no filtering, no ranking, no boot change.
#[tokio::test]
async fn a_record_can_be_marked_and_the_mark_can_be_changed() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;

    // A write's own receipt elides fields (never echoing what the caller
    // just sent), so every check here reads the mark back through
    // `recall` — the same door a later reader uses.
    let marked = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("starred".to_string(), "true".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:alpha", "carries the mark from the start")
        },
    )
    .await;
    let address = address_of(&marked);

    capture_ok(
        &jojobot,
        capture_args("person:alpha", "an ordinary claim with no mark"),
    )
    .await;

    let recall_fields = |recalled: &serde_json::Value, addr: &str| -> serde_json::Value {
        recalled["objects"][0]["facts"]
            .as_array()
            .expect("facts")
            .iter()
            .find(|f| f["address"] == addr)
            .unwrap_or_else(|| panic!("{addr} is there: {recalled}"))["fields"]
            .clone()
    };

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        recall_fields(&recalled, &address)["starred"],
        "true",
        "{recalled}"
    );
    let unmarked_facts = recalled["objects"][0]["facts"].as_array().expect("facts");
    let unmarked = unmarked_facts
        .iter()
        .find(|f| f["content"] == "an ordinary claim with no mark")
        .expect("the unmarked record is there");
    assert!(
        unmarked["fields"].get("starred").is_none(),
        "a record nobody marked reads back marked: {unmarked}"
    );

    // The verb that corrects one changes it, like any other field.
    update_ok(
        &jojobot,
        UpdateFactArgs {
            fields: Some(
                [("starred".to_string(), "false".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..update_args(&address)
        },
    )
    .await;
    let recalled_again = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        recall_fields(&recalled_again, &address)["starred"],
        "false",
        "{recalled_again}"
    );
}

/// 🚨 **A patch that names nothing is refused, not silently honoured.**
///
/// Before this, an entirely empty call still reached the store and
/// appended a write to the claim's own history — a real, permanent
/// re-assertion nothing on the receipt named as one, discoverable only
/// by trying it and noticing the moment moved. Naming nothing is now
/// refused, and the refusal names the real act.
#[tokio::test]
async fn a_patch_naming_nothing_is_refused_and_names_keep() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let captured = capture_ok(&jojobot, capture_args("person:alpha", "the kiln is lit")).await;
    let address = address_of(&captured);

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(update_args(&address)))
            .await
            .expect("update_fact answers rather than failing the protocol"),
    )
    .to_string();
    assert!(
        refused.contains("keep"),
        "a patch naming nothing must name the real act in its refusal: {refused}"
    );
}

/// 🚨 **`keep` bumps the moment and changes nothing — proven on the
/// claim's own write history, not merely on its receipt.** This is the
/// designed act the refusal above points a caller toward.
#[tokio::test]
async fn keep_bumps_the_moment_and_changes_nothing() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let captured = capture_ok(&jojobot, capture_args("person:alpha", "the kiln is lit")).await;
    let address = address_of(&captured);

    let kept = update_ok(
        &jojobot,
        UpdateFactArgs {
            keep: Some(true),
            ..update_args(&address)
        },
    )
    .await;
    assert!(
        postcondition_line(&kept)
            .to_lowercase()
            .contains("unchanged"),
        "a keep's own receipt must say it changed nothing on purpose, distinctly from the \
             ordinary edit line's own ambient use of the word \"kept\": {kept}"
    );

    let history = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some(address.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let record = &history["objects"][0]["record_history"];
    assert_eq!(record["count"], 2, "the capture plus one keep: {record}");
    let writes = record["writes"].as_array().expect("writes");
    assert_eq!(
        writes[0]["content"], writes[1]["content"],
        "a keep must change no content: {record}"
    );
    assert_ne!(
        writes[0]["written_at"], writes[1]["written_at"],
        "a keep must still move the moment, or it kept nothing: {record}"
    );
}

/// 🚨 **`keep` combined with an actual change is refused — there is no
/// partial keep.**
#[tokio::test]
async fn keep_combined_with_a_change_is_refused() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let captured = capture_ok(&jojobot, capture_args("person:alpha", "the kiln is lit")).await;
    let address = address_of(&captured);

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                keep: Some(true),
                content: Some("the kiln is out".into()),
                provenance: Some("inference".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update_fact answers rather than failing the protocol"),
    )
    .to_string();
    assert!(
        refused.contains("naming a change is not a keep"),
        "a keep combined with a change must be refused for THAT reason specifically — not \
             merely refused, and not the reason an empty patch with no keep is refused, which \
             also happens to mention the word \"keep\": {refused}"
    );
}

/// **The thing a ceiling binds cannot write that ceiling — through a
/// patch, exactly as through a fresh capture.** A patch is a write like
/// any other, and this one can reach the same key: bot:otto (this
/// harness's default caller) editing its own fact cannot add
/// `thought_capacity` to it.
#[tokio::test]
async fn a_bot_cannot_raise_its_own_thought_capacity_by_patch() {
    let jojobot = handler();
    ensure(&jojobot, "bot:otto").await;
    let captured = capture_ok(&jojobot, capture_args("bot:otto", "a claim about myself")).await;
    let address = address_of(&captured);

    let refused = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "5".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");

    let after = fields_of(&jojobot, "bot:otto").await;
    assert!(
        after
            .get(jojobot_domain::memory::THOUGHT_CAPACITY)
            .is_none(),
        "the refused patch must not be readable back: {after}"
    );
}

/// **The positive half.** A different identity patching the same key
/// onto the same kind of subject lands.
#[tokio::test]
async fn a_different_bot_can_raise_this_bots_thought_capacity_by_patch() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        capture_args("bot:milhouse", "a claim about milhouse"),
    )
    .await;
    let address = address_of(&captured);

    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "5".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_ne!(edited["status"], "blocked", "{edited}");

    let after = fields_of(&jojobot, "bot:milhouse").await;
    assert_eq!(
        after[jojobot_domain::memory::THOUGHT_CAPACITY],
        "5",
        "a different identity's patch must land: {after}"
    );
}

/// **The three side doors `update_fact` would otherwise leave open on a
/// role's own two fields**: setting the holder directly, clearing the
/// claim moment, and archiving the record that carries them — none of
/// this verb's business, because the claim and renewal paths never reach
/// it. Paired with the claim path itself: a renewal through the boot
/// door still moves the claim after all three refusals, so the guard is
/// proven to stop only the ordinary surface.
#[tokio::test]
async fn update_fact_refuses_to_set_clear_or_archive_a_roles_own_fields() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: Some("dev-dispatch".into()),
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
            }))
            .await
            .expect("start_here ok"),
    );
    let sid = sid_of(&booted).expect("a handle");
    assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");

    let bot = EntityId("bot:gamma".into());
    let claim_address = jojobot
        .memory
        .recall(&bot)
        .await
        .expect("recall ok")
        .into_iter()
        .find(|fact| fact.fields.contains_key("role/dev-dispatch/holder"))
        .expect("the claim left a fact carrying its own fields")
        .address()
        .to_string();

    let set = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [(
                        "role/dev-dispatch/holder".to_string(),
                        "epsilon".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..update_args(&claim_address)
            }))
            .await
            .expect("a refusal is an answer, not a protocol failure"),
    );
    assert_eq!(set["wrote"], false, "{set}");

    let cleared = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                clear_fields: Some(vec!["role/dev-dispatch/claimed_at".into()]),
                ..update_args(&claim_address)
            }))
            .await
            .expect("a refusal is an answer, not a protocol failure"),
    );
    assert_eq!(cleared["wrote"], false, "{cleared}");

    let archived = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&claim_address)
            }))
            .await
            .expect("a refusal is an answer, not a protocol failure"),
    );
    assert_eq!(archived["wrote"], false, "{archived}");

    let after = jojobot.memory.fields(&bot).await.expect("fields ok");
    assert_eq!(
        after.get("role/dev-dispatch/holder"),
        Some(&sid),
        "the claim's own holder must survive all three refused attempts: {after:?}"
    );

    // The claim path itself, untouched: a renewal through the boot door
    // still moves the claim moment after every side door was refused.
    let claimed_at_before = after
        .get("role/dev-dispatch/claimed_at")
        .cloned()
        .expect("the claim's own field is present");
    journal_entry(
        &jojobot,
        &sid,
        "kept working after the side doors were tried",
    )
    .await;
    let renewed = jojobot.memory.fields(&bot).await.expect("fields ok");
    assert_ne!(
        renewed.get("role/dev-dispatch/claimed_at"),
        Some(&claimed_at_before),
        "the claim path must still renew once the side doors are closed: {renewed:?}"
    );
}

/// **The same overflow `capture` proves, through the other verb that can
/// cause it.** `update_fact` is the other write shape that can set
/// `fields.starred` — turning an ordinary claim into a rule after the
/// fact — and it has to name the same fallout: the write lands, and its
/// receipt names the oldest starred rule that no longer rides the boot.
#[tokio::test]
async fn update_fact_names_the_rule_a_new_star_pushes_off_the_boot() {
    let jojobot = handler();
    let bot = "bot:mcp-seats-update";
    ensure(&jojobot, bot).await;

    let mut oldest = String::new();
    for n in 0..jojobot_domain::text::CARRIED_RULES {
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(bot, &format!("starred rule {n}"))
            },
        )
        .await;
        if n == 0 {
            oldest = address_of(&receipt);
        }
    }
    let sixth = capture_ok(
        &jojobot,
        capture_args(bot, "an ordinary rule, starred later"),
    )
    .await;
    let sixth_address = address_of(&sixth);

    let starred_later = update_ok(
        &jojobot,
        UpdateFactArgs {
            fields: Some(
                [("starred".to_string(), "true".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..update_args(&sixth_address)
        },
    )
    .await;
    assert_eq!(
        starred_later["seats"]["dropped"], oldest,
        "an edit that stars a rule past the cap has to name the oldest starred rule, exactly \
             as a capture does: {starred_later}"
    );
    let note = starred_later["seats"]["note"].as_str().unwrap_or_else(|| {
        panic!("the edit carries the same sentence the boot shows: {starred_later}")
    });
    assert!(note.contains("skill"), "{note}");
}

/// **`update_fact`'s own copy of the field-shadows-argument teaching** —
/// `capture`'s is proven in `teaching.rs`; this is the same mechanism
/// wired through the edit verb, whose own `status` argument makes the
/// PM's own motivating example ("a rule filed with fields:
/// {status: retired}" instead of `update_fact` with `status: archived`)
/// the natural key to prove it with here.
#[tokio::test]
async fn a_field_that_shadows_update_facts_own_argument_is_taught_once() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "the club meets Tuesdays")).await;
    let address = address_of(&captured);

    let shadowing = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("status".to_string(), "retired".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        shadowing["teaching"]
            .as_array()
            .unwrap_or_else(|| panic!("a teaching list: {shadowing}"))
            .iter()
            .any(|t| t.as_str().is_some_and(
                |t| t.contains("\"status\"") && t.contains("is stored as ordinary data")
            )),
        "a field shadowing update_fact's own status argument is taught: {shadowing}"
    );

    let second = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("status".to_string(), "retired".to_string())]
                        .into_iter()
                        .collect(),
                ),
                keep: None,
                content: Some("the club meets Wednesdays".into()),
                provenance: Some("inference".to_string()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        !second["teaching"]
            .as_array()
            .map(|t| t.iter().any(|t| t.as_str().is_some_and(
                |t| t.contains("\"status\"") && t.contains("is stored as ordinary data")
            )))
            .unwrap_or(false),
        "the same session shadowing the same argument again is not taught again: {second}"
    );
}

/// **The same wiring as `capture`'s, for `update_fact`.** `update_fact`
/// carries no entity of its own — only a `FactAddress`, home and local id
/// — so this reaches no extra store read: the written fact's own
/// `subject`, already returned by the write below, is what
/// `echoed_defaults` is asked about. `FactPatch` cannot move a fact to a
/// new subject, so what comes back is the entity these fields were
/// always about.
///
/// Paired exactly as `capture`'s own case is: a field equal to today's
/// default carries one note naming it, and an ordinary field carries
/// none.
#[tokio::test]
async fn an_edited_field_equal_to_todays_default_is_named_in_the_receipt() {
    let subject = EntityId::person("person:milhouse");
    let jojobot = handler_field_provisioned(subject, "one_liner", "The disposable implementer.");
    ensure(&jojobot, "person:milhouse").await;
    let posted = capture_ok(
        &jojobot,
        capture_args("person:milhouse", "milhouse's own record"),
    )
    .await;
    let address = posted["address"]
        .as_str()
        .expect("a capture receipt carries its own address")
        .to_string();

    let echoing = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [(
                        "one_liner".to_string(),
                        "The disposable implementer.".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    let notes = echoing["echoes_defaults"]
        .as_array()
        .expect("an array of notes");
    assert_eq!(notes.len(), 1, "{echoing}");
    assert!(
        notes[0]
            .as_str()
            .expect("a note string")
            .contains("one_liner"),
        "the note names the echoing key: {echoing}"
    );

    let ordinary = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: Some(
                    [("greeting".to_string(), "Hey, it's Milhouse.".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert!(
        ordinary.get("echoes_defaults").is_none(),
        "a field with no shipped default must carry no note: {ordinary}"
    );
}

/// A bot at capacity one, holding one thought and one plain claim that an
/// edit could turn into a second thought. Returns the handler and the plain
/// claim's address.
async fn a_full_room_and_a_claim_that_would_join_it(jojobot: &Jojobot, bot: &str) -> String {
    ensure(jojobot, bot).await;
    ensure(jojobot, "thing:the-couch").await;
    ensure(jojobot, "thing:the-air-filter").await;
    capture_ok(
        jojobot,
        CaptureArgs {
            fields: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;
    capture_ok(
        jojobot,
        CaptureArgs {
            shape: Some("connection".into()),
            object: Some("thing:the-couch".into()),
            ..capture_args(bot, "the couch needs a leg fixed")
        },
    )
    .await;
    let plain = capture_ok(jojobot, capture_args(bot, "the filter is in the hall")).await;
    address_of(&plain)
}

/// 🚨 **A refusal names only moves the refusing verb has** (rule 68). The
/// room-full answer is shared with `capture`, whose arguments include a drop
/// and a borrow; `update_fact` has neither, so advice to re-call it naming
/// either sends an agent in a loop.
///
/// Paired with `capture`'s own refusal at the same state, which must still
/// offer both: without that half this passes on a refusal that stopped
/// naming any way forward.
#[tokio::test]
async fn a_full_room_refusal_names_only_moves_the_refusing_verb_has() {
    let jojobot = handler();
    let bot = "bot:mcp-seats-update";
    let plain = a_full_room_and_a_claim_that_would_join_it(&jojobot, bot).await;

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                shape: Some("connection".into()),
                object: Some("thing:the-air-filter".into()),
                ..update_args(&plain)
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    for argument in ["drop", "drop_because", "borrow"] {
        assert!(
            !how.contains(argument),
            "update_fact has no `{argument}` argument, so the refusal must not name it: {how}"
        );
    }
    let update_fact_args =
        serde_json::to_value(schemars::schema_for!(UpdateFactArgs)).expect("the schema serialises");
    assert!(
        update_fact_args["properties"].get("status").is_some(),
        "the way forward below names `status`, so update_fact must have it"
    );
    assert!(
        how.contains("status") && how.contains("archived"),
        "the way forward is archiving a live thought through this verb: {how}"
    );
    assert!(
        how.contains("capture"),
        "the other way forward is making the change through capture, which can archive one \
         as it writes: {how}"
    );

    let capture_refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some("connection".into()),
                object: Some("thing:the-air-filter".into()),
                ..capture_args(bot, "the air filter is due")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    let capture_how = capture_refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        capture_how.contains("drop_because") && capture_how.contains("borrow"),
        "capture has both moves and still offers them: {capture_how}"
    );
}

/// 🚨 **A room an earlier borrow left over its capacity needs more than one
/// archive before an edit can add a thought**, and the refusal says how many.
/// Following it exactly lets the re-call land; one fewer does not. Paired with
/// the room exactly at capacity, where the count is one.
#[tokio::test]
async fn an_over_capacity_refusal_says_how_many_to_archive_and_that_many_is_enough() {
    let jojobot = handler();
    let bot = "bot:mcp-seats-update";
    let plain = a_full_room_and_a_claim_that_would_join_it(&jojobot, bot).await;
    ensure(&jojobot, "thing:the-gutter").await;
    let edit = || UpdateFactArgs {
        shape: Some("connection".into()),
        object: Some("thing:the-gutter".into()),
        ..update_args(&plain)
    };

    // At capacity one with one thought live: one archive is the count.
    let at_capacity = blocked(
        &jojobot
            .update_fact(Parameters(edit()))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(at_capacity["archive_needed"], 1, "{at_capacity}");

    // One borrow later the room holds two against a capacity of one.
    capture_ok(
        &jojobot,
        CaptureArgs {
            shape: Some("connection".into()),
            object: Some("thing:the-air-filter".into()),
            borrow: Some(true),
            ..capture_args(bot, "the filter needs changing")
        },
    )
    .await;
    let refused = blocked(
        &jojobot
            .update_fact(Parameters(edit()))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(
        refused["archive_needed"], 2,
        "live 2 over capacity 1: {refused}"
    );
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    // The count a reader meets is the word after "Archive", the same number
    // the structured field carries.
    let said = how
        .split("Archive ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next());
    assert_eq!(
        said,
        Some("2"),
        "the way forward must say how many to archive: {how}"
    );
    for argument in ["drop", "drop_because", "borrow"] {
        assert!(
            !how.contains(argument),
            "update_fact has no `{argument}` argument, so the refusal must not name it: {how}"
        );
    }
    let room: Vec<String> = refused["room"]
        .as_array()
        .expect("the refusal lists the room")
        .iter()
        .map(|thought| {
            thought["address"]
                .as_str()
                .expect("a thought carries its address")
                .to_string()
        })
        .collect();
    assert_eq!(room.len(), 2, "{refused}");

    // One archive is not enough: the room still holds as many as its capacity.
    let archive = |address: &str| UpdateFactArgs {
        status: Some("archived".into()),
        details: Some("no longer earns its slot".into()),
        ..update_args(address)
    };
    let one = json_of(
        &jojobot
            .update_fact(Parameters(archive(&room[0])))
            .await
            .expect("archive ok"),
    );
    assert_ne!(one["status"], "blocked", "{one}");
    let still = blocked(
        &jojobot
            .update_fact(Parameters(edit()))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(still["wrote"], false, "one archive was not enough: {still}");
    assert_eq!(still["archive_needed"], 1, "{still}");

    // The rest of what the refusal said, followed exactly, lets it land.
    let two = json_of(
        &jojobot
            .update_fact(Parameters(archive(&room[1])))
            .await
            .expect("archive ok"),
    );
    assert_ne!(two["status"], "blocked", "{two}");
    let landed = json_of(
        &jojobot
            .update_fact(Parameters(edit()))
            .await
            .expect("update_fact ok"),
    );
    assert_ne!(
        landed["status"], "blocked",
        "following the count must let the edit land: {landed}"
    );
}

/// 🚨 **An edit counts the room the way `capture` does, through the served
/// verbs.** The cutoff comes from the bot's own runs, which only the verb can
/// read, so this has to travel `update_fact` rather than the port: a bot that
/// has run `AGES_AFTER_RUNS` times has aged its old thought out of the count,
/// and an edit that makes a second thought lands with nothing dropped.
///
/// Paired with the same edit before any run exists, which is refused.
#[tokio::test]
async fn an_edit_into_a_room_counts_a_thought_that_aged_out_the_way_capture_does() {
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        sessions.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let bot = "bot:mcp-thought-aging";
    let plain = a_full_room_and_a_claim_that_would_join_it(&jojobot, bot).await;
    let join = || UpdateFactArgs {
        shape: Some("connection".into()),
        object: Some("thing:the-air-filter".into()),
        ..update_args(&plain)
    };

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(join()))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(
        refused["aged_out"], 0,
        "before any run exists nothing has aged: {refused}"
    );

    for n in 0..jojobot_domain::memory::AGES_AFTER_RUNS {
        sessions
            .begin(jojobot_domain::session::NewSession {
                bot: EntityId(bot.to_string()),
                sid: jojobot_domain::session::Sid(format!("ar{n:02}")),
                focus: "working".into(),
                started_at: jiff::Timestamp::now(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("seeding a run");
    }

    let landed = update_ok(&jojobot, join()).await;
    assert_eq!(
        landed["content_head"], "the filter is in the hall",
        "the old thought aged out of a room at capacity one, so the edit lands: {landed}"
    );
}

/// **A store that cannot be read refuses an archive of a role's own record.**
/// The guard reads the record to see whether it carries a role field, and a
/// read that failed used to read as "carries none". Paired with the healthy
/// store, where archiving an ordinary claim still lands.
#[tokio::test]
async fn update_fact_refuses_to_archive_a_role_record_when_the_store_cannot_be_read() {
    let (healthy, blind) = healthy_and_blind_to_claims();
    let claim_address = a_claimed_role(&healthy).await;

    let refused = blind
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("archived".into()),
            ..update_args(&claim_address)
        }))
        .await;
    let Err(err) = refused else {
        panic!("an archive went through while the store could not be read");
    };
    assert!(
        err.message.contains("storage"),
        "the refusal must name the storage failure and what to do next: {}",
        err.message
    );
    let after = healthy
        .memory
        .fields(&EntityId("bot:gamma".into()))
        .await
        .expect("fields ok");
    assert!(
        after.contains_key("role/dev-dispatch/holder"),
        "the claim's holder left the fold with the store unreadable: {after:?}"
    );

    // The positive half: the same patch on a store that reads, over an
    // ordinary claim, lands.
    let ordinary =
        address_of(&capture_ok(&healthy, capture_args("person:alpha", "ordinary")).await);
    let landed = json_of(
        &healthy
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&ordinary)
            }))
            .await
            .expect("update_fact ok"),
    );
    assert_ne!(landed["status"], "blocked", "{landed}");
}

/// **A role's own record cannot be archived through the handle its bot wore
/// before a rename**, for the reason `retract`'s own case gives. Paired with
/// archiving an ordinary claim through the former handle, which lands.
#[tokio::test]
async fn update_fact_refuses_to_archive_a_role_record_addressed_by_a_former_handle() {
    let jojobot = handler();
    let (claim, ordinary) = a_role_and_an_ordinary_claim_under_a_former_handle(&jojobot).await;
    assert!(claim.starts_with("bot:gamma#"), "{claim}");

    let refused = blocked(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&claim)
            }))
            .await
            .expect("a refusal is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    let after = jojobot
        .memory
        .fields(&EntityId("bot:delta".into()))
        .await
        .expect("fields ok");
    assert!(
        after.contains_key("role/dev-dispatch/holder"),
        "the claim's holder left the fold through a former handle: {after:?}"
    );

    let landed = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&ordinary)
            }))
            .await
            .expect("update_fact ok"),
    );
    assert_ne!(landed["status"], "blocked", "{landed}");
}

/// **Archiving a claim resolves the one handle it is about, and never lists
/// every entity to do it.** The role-field guard needs the record's own
/// claims, which one targeted read answers; listing the whole store for that
/// read made every archiving edit cost the size of the store. Paired with the
/// archive landing, so a count of zero cannot be a call that never ran.
#[tokio::test]
async fn archiving_a_claim_does_not_list_every_entity_to_resolve_one_handle() {
    let store = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over(store.clone());
    for handle in ["person:alpha", "person:beta", "person:delta"] {
        ensure(&jojobot, handle).await;
    }
    let held = capture_ok(&jojobot, capture_args("person:alpha", "was at the party")).await;

    let before = store.listings();
    let landed = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                details: Some("it did not happen".into()),
                ..update_args(&address_of(&held))
            }))
            .await
            .expect("update_fact ok"),
    );
    assert_ne!(landed["status"], "blocked", "{landed}");
    assert_eq!(landed["status"], "archived", "{landed}");
    assert_eq!(
        store.listings(),
        before,
        "the archiving edit listed every entity in the store"
    );
}
