//! Recall cases added after the file of recall cases reached its size: the walk over
//! a dependency an older record wrote as its own field, and the cap a page puts on
//! the history of a run. Kept apart so that file holds its size.

use super::*;

#[tokio::test]
async fn records_agree_with_the_subject_all_the_addresses_name() {
    let jojobot = handler();
    for content in ["Alpha keeps bees", "Alpha rides a bicycle"] {
        capture_ok(&jojobot, capture_args("alpha", content)).await;
    }
    for subject in ["person:alpha", "alpha"] {
        let answer = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some(subject.into()),
                    ..of_records(&["person:alpha#f1", "person:alpha#f2"])
                }))
                .await
                .expect("records read"),
        );
        assert_eq!(answer["count"], 2, "{subject}: {answer}");
        assert_eq!(answer["records"][0]["content"], "Alpha keeps bees");
        assert_eq!(answer["records"][1]["content"], "Alpha rides a bicycle");
    }
}

#[tokio::test]
async fn records_agree_with_facts_and_details() {
    let jojobot = handler();
    capture_ok(
        &jojobot,
        CaptureArgs {
            details: Some("Alpha tends the hive on Tuesdays".into()),
            ..capture_args("alpha", "Alpha keeps bees")
        },
    )
    .await;
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                details: Some(true),
                ..of_records(&["person:alpha#f1"])
            }))
            .await
            .expect("records read"),
    );
    assert_eq!(
        answer["records"][0]["content"], "Alpha keeps bees",
        "{answer}"
    );
    assert_eq!(
        answer["records"][0]["details"],
        "Alpha tends the hive on Tuesdays"
    );
}

#[tokio::test]
async fn records_agree_with_record_keys_and_keep_only_the_keys_asked_for() {
    let jojobot = handler();
    capture_ok(
        &jojobot,
        CaptureArgs {
            details: Some("Alpha tends the hive on Tuesdays".into()),
            ..capture_args("alpha", "Alpha keeps bees")
        },
    )
    .await;
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                record_keys: Some(vec!["details".into()]),
                ..of_records(&["person:alpha#f1"])
            }))
            .await
            .expect("records read"),
    );
    let record = &answer["records"][0];
    assert_eq!(record["address"], "person:alpha#f1", "{answer}");
    assert_eq!(record["details"], "Alpha tends the hive on Tuesdays");
    assert_eq!(record.as_object().expect("one record").len(), 2);
    let omitted = answer["records_narrowed"]["left_out_keys"]
        .as_array()
        .unwrap_or_else(|| panic!("narrowed keys are named: {answer}"));
    assert!(omitted.iter().any(|key| key == "content"), "{answer}");
    assert!(omitted.iter().any(|key| key == "provenance"), "{answer}");
}

#[tokio::test]
async fn records_agree_keeps_missing_reasons_beside_narrowed_records() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "Alpha keeps bees")).await;
    let asked = ["person:alpha#f1", "person:alpha#f999", "not-an-address"];
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                record_keys: Some(vec!["content".into()]),
                ..of_records(&asked)
            }))
            .await
            .expect("records read"),
    );
    assert_eq!(answer["count"], 3, "{answer}");
    let records = answer["records"].as_array().expect("the three addresses");
    assert_eq!(records.len(), asked.len(), "{answer}");
    assert_eq!(records[0]["address"], asked[0], "{answer}");
    assert_eq!(records[0]["content"], "Alpha keeps bees", "{answer}");
    assert_eq!(records[0].as_object().unwrap().len(), 2, "{answer}");
    for (record, address) in records[1..].iter().zip(&asked[1..]) {
        assert_eq!(record["address"], *address, "{answer}");
        assert!(
            record["missing"]
                .as_str()
                .is_some_and(|why| !why.is_empty()),
            "{answer}"
        );
        assert_eq!(record.as_object().unwrap().len(), 2, "{answer}");
    }
    let omitted = answer["records_narrowed"]["left_out_keys"]
        .as_array()
        .expect("real record narrowing is reported");
    assert!(!omitted.iter().any(|key| key == "missing"), "{answer}");
}

#[tokio::test]
async fn records_agree_only_when_every_address_belongs_to_the_subject() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "Alpha keeps bees")).await;
    capture_ok(&jojobot, capture_args("beta", "Beta rides a bicycle")).await;
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:alpha".into()),
                ..of_records(&["person:alpha#f1", "person:beta#f1"])
            }))
            .await
            .expect("refusal answers"),
    );
    assert_eq!(answer["status"], "blocked", "{answer}");
    assert_eq!(answer["wrote"], false);
    assert!(answer.to_string().contains("subject"));
    assert!(answer.get("records").is_none());
}

#[tokio::test]
async fn records_agree_does_not_hide_another_selection_behind_compatible_arguments() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "Alpha keeps bees")).await;
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:alpha".into()),
                facts: Some(true),
                record_keys: Some(vec!["content".into()]),
                kind: Some("person".into()),
                ..of_records(&["person:alpha#f1"])
            }))
            .await
            .expect("refusal answers"),
    );
    assert_eq!(answer["status"], "blocked", "{answer}");
    assert_eq!(answer["wrote"], false);
    assert!(answer.get("records").is_none());
}

async fn a_location_to_read_in_history(jojobot: &Jojobot) -> String {
    let written = capture_ok(
        jojobot,
        CaptureArgs {
            shape: Some("location".into()),
            object: Some("place:daredevil-gorge".into()),
            ..capture_args("person:alpha", "spends the afternoon at the gorge")
        },
    )
    .await;
    written["address"]
        .as_str()
        .expect("the claim's address")
        .into()
}

async fn read_location_history(jojobot: &Jojobot, address: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some(address.into()),
                ..of("person:alpha")
            }))
            .await
            .expect("history answers"),
    )
}

/// **A history edge is a writable edge**, with the same public handle an
/// ordinary record serves. Copying it must reach the object it names.
#[tokio::test]
async fn a_record_history_edge_can_be_copied_into_capture() {
    let jojobot = handler();
    let address = a_location_to_read_in_history(&jojobot).await;
    let history = read_location_history(&jojobot, &address).await;
    let edge = &history["objects"][0]["record_history"]["writes"][0]["edge"];
    let copied = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some(edge["type"].as_str().expect("shape").into()),
                object: Some(edge["object"].as_str().expect("object").into()),
                ..capture_args("person:alpha", "returns to the same place")
            }))
            .await
            .expect("a copied history edge is accepted"),
    );
    assert_ne!(copied["status"], "blocked", "{copied}");
    assert_eq!(copied["edge"], history["objects"][0]["facts"][0]["edge"]);
    assert_eq!(edge["object"], "place:daredevil-gorge", "{history}");
}

/// **The historical pointer follows a rename**, just as the same claim's
/// current edge does; the old handle is not the object's public name today.
#[tokio::test]
async fn a_record_history_edge_names_the_object_under_its_current_handle() {
    let jojobot = handler();
    let address = a_location_to_read_in_history(&jojobot).await;
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(RenameEntityArgs {
                request_id: None,
                handle: "place:daredevil-gorge".into(),
                to: "place:shelbyville".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(writing_as(&jojobot)),
            }))
            .await
            .expect("rename answers"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");
    let history = read_location_history(&jojobot, &address).await;
    let ordinary = &history["objects"][0]["facts"][0]["edge"];
    assert_eq!(ordinary["object"], "place:shelbyville", "{history}");
    assert_eq!(
        history["objects"][0]["record_history"]["writes"][0]["edge"], *ordinary,
        "{history}"
    );
}

/// **Archiving the far end leaves both reads naming the same thing**; a
/// historical pointer does not become an opaque id when the thing is archived.
#[tokio::test]
async fn a_record_history_edge_to_an_archived_object_matches_the_ordinary_edge() {
    let jojobot = handler();
    let address = a_location_to_read_in_history(&jojobot).await;
    let archived = json_of(
        &jojobot
            .archive_entity(Parameters(ArchiveEntityArgs {
                request_id: None,
                handle: "place:daredevil-gorge".into(),
                reason: "no longer visited".into(),
                restore: None,
                sid: Some(writing_as(&jojobot)),
            }))
            .await
            .expect("archive answers"),
    );
    assert_ne!(archived["status"], "blocked", "{archived}");
    let history = read_location_history(&jojobot, &address).await;
    let ordinary = &history["objects"][0]["facts"][0]["edge"];
    assert_eq!(ordinary["object"], "place:daredevil-gorge", "{history}");
    assert_eq!(
        history["objects"][0]["record_history"]["writes"][0]["edge"], *ordinary,
        "{history}"
    );
}

/// **An older record that kept a dependency as its own field is walked by what
/// the thing holds now.** On a store from before own fields stayed on their
/// record, such a key reaches its thing, so a later write that shortens it
/// removes a dependency from the walk as it does from the ready view.
#[tokio::test]
async fn a_walk_leaves_out_a_dependency_an_older_record_wrote_as_its_own_field() {
    let jojobot = crate::harness::handler_shipped_reaching();
    work_with(&jojobot, "work:lambda", &[("status", "next")]).await;
    work_with(&jojobot, "work:gamma", &[("status", "next")]).await;
    work_with(&jojobot, "work:iota", &[("status", "next")]).await;
    for (depends_on, content) in [
        ("work:lambda, work:gamma", "needs both"),
        ("work:gamma", "only gamma is needed now"),
    ] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("depends_on".to_string(), depends_on.to_string())]
                        .into_iter()
                        .collect(),
                ),
                sid: Some(writing_as(&jojobot)),
                ..capture_args("work:iota", content)
            },
        )
        .await;
    }
    walks_agree_with_ready(&jojobot).await;
}

/// **A page of a run's history holds at most `limit` of what `offset` counts**,
/// and reading on reaches each once. The journal of a run is counted in wordings:
/// an entry amended once has two. With a limit of one, each part carries one
/// wording, names the older ones it left out and the offset that reads on, and the
/// two parts together carry both once. **Paired** with the same call that sends no
/// limit, which carries both whole and names nothing left out.
#[tokio::test]
async fn a_limit_caps_a_page_of_a_runs_journal_history_the_way_offset_counts_it() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    let receipt = journal_with_focus(&jojobot, &sid, "a short beat", "a small run").await;
    amend_newest(&jojobot, &sid, "a short beat, said again").await;
    let handle = format!(
        "session:{}",
        receipt["session"].as_str().expect("the run it landed in")
    );
    let read = |offset: Option<u32>, limit: Option<u32>| {
        let (jojobot, sid, handle) = (&jojobot, sid.clone(), handle.clone());
        async move {
            json_of(
                &jojobot
                    .recall(Parameters(RecallArgs {
                        sid: Some(sid),
                        history: Some("journal".into()),
                        offset,
                        limit,
                        ..of(&handle)
                    }))
                    .await
                    .expect("recall answers"),
            )
        }
    };
    let revisions = |answer: &serde_json::Value| -> Vec<u64> {
        answer["objects"][0]["history"]["entries"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .flat_map(|entry| entry["wordings"].as_array().cloned().unwrap_or_default())
                    .map(|wording| wording["revision"].as_u64().expect("a revision"))
                    .collect()
            })
            .unwrap_or_default()
    };

    let whole = read(None, None).await;
    assert_eq!(whole["objects"][0]["history"]["shown"], 2, "{whole}");
    assert!(
        whole["objects"][0]["history"].get("not_shown").is_none(),
        "{whole}"
    );

    let first = read(None, Some(1)).await;
    let history = &first["objects"][0]["history"];
    assert_eq!(history["count"], 2, "{first}");
    assert_eq!(history["shown"], 1, "a page of one: {first}");
    assert_eq!(history["not_shown"]["count"], 1, "{first}");
    let next = history["not_shown"]["offset"].as_u64().expect("an offset");
    assert_eq!(next, 1, "{first}");

    let second = read(Some(next as u32), Some(1)).await;
    let history = &second["objects"][0]["history"];
    assert_eq!(history["shown"], 1, "{second}");
    assert!(history.get("not_shown").is_none(), "{second}");

    let mut reached = revisions(&first);
    reached.extend(revisions(&second));
    reached.sort_unstable();
    let mut every = revisions(&whole);
    every.sort_unstable();
    assert_eq!(reached, every, "each wording once");
    assert_eq!(reached.len(), 2, "{reached:?}");
}

/// **The same cap on the changes of a field of a run.** `focus` changes with each
/// beat that moves it; a limit of one carries one change a page, `not_shown` counts
/// the rest exactly, and reading on reaches every change once.
#[tokio::test]
async fn a_limit_caps_a_page_of_a_runs_field_history_the_way_offset_counts_it() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    let receipt = journal_with_focus(&jojobot, &sid, "first beat", "first focus").await;
    journal_with_focus(&jojobot, &sid, "second beat", "second focus").await;
    journal_with_focus(&jojobot, &sid, "third beat", "third focus").await;
    let handle = format!(
        "session:{}",
        receipt["session"].as_str().expect("the run it landed in")
    );
    let read = |offset: Option<u32>, limit: Option<u32>| {
        let (jojobot, sid, handle) = (&jojobot, sid.clone(), handle.clone());
        async move {
            json_of(
                &jojobot
                    .recall(Parameters(RecallArgs {
                        sid: Some(sid),
                        history: Some("focus".into()),
                        offset,
                        limit,
                        ..of(&handle)
                    }))
                    .await
                    .expect("recall answers"),
            )
        }
    };
    let whole = read(None, None).await;
    let total = whole["objects"][0]["history"]["count"]
        .as_u64()
        .expect("a count") as usize;
    assert!(
        total >= 3,
        "the focus changed at least three times: {whole}"
    );
    assert_eq!(whole["objects"][0]["history"]["shown"], total, "{whole}");

    let mut positions: Vec<u64> = Vec::new();
    let mut offset = None;
    let mut parts = 0;
    loop {
        let answer = read(offset, Some(1)).await;
        parts += 1;
        assert!(parts <= total + 1, "the parts never end: {answer}");
        let history = &answer["objects"][0]["history"];
        assert_eq!(history["shown"], 1, "a page of one: {answer}");
        positions.extend(
            history["writes"]
                .as_array()
                .expect("the changes")
                .iter()
                .map(|write| write["position"].as_u64().expect("a position")),
        );
        match history.get("not_shown") {
            Some(left) => {
                let next = left["offset"].as_u64().expect("an offset");
                assert_eq!(
                    left["count"].as_u64(),
                    Some(total as u64 - next),
                    "{answer}"
                );
                offset = Some(next as u32);
            }
            None => break,
        }
    }
    assert_eq!(parts, total, "one change a part");
    positions.sort_unstable();
    positions.dedup();
    assert_eq!(positions.len(), total, "every change once: {positions:?}");
}

/// **Each object carries its own distance when `overdue` is asked beside
/// `unreviewed`.** The owed pass sorts the objects by when they fall due and the
/// review pass sorts them again by when they last changed, so the distances have
/// to follow their objects and not their places. The thing that fell due first
/// was written second here, so the two orders disagree.
#[tokio::test]
async fn overdue_beside_unreviewed_serves_each_object_its_own_distance() {
    let jojobot = crate::harness::handler_shipped();
    let acting = run_on(&jojobot, "2026-03-15").await;
    for (slug, runs_out) in [("zeta", "2026-03-12"), ("epsilon", "2026-03-01")] {
        let made = jojobot
            .add_entity(Parameters(AddEntityArgs {
                sets: Some(
                    [("runs_out".to_string(), runs_out.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..add_args("thing", slug, slug)
            }))
            .await
            .expect("add ok");
        assert_ne!(json_of(&made)["status"], "blocked", "{}", json_of(&made));
    }

    let answered = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("thing".into()),
                overdue: Some(OverdueArgs { as_of: None }),
                unreviewed: Some(true),
                sid: Some(acting),
                ..of_nothing()
            }))
            .await
            .expect("recall answers"),
    );
    let days = |handle: &str| {
        answered["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .find(|row| row["id"] == handle)
            .map(|row| row["overdue_by_days"].clone())
    };
    assert_eq!(
        handles(&answered),
        ["thing:zeta", "thing:epsilon"],
        "the review pass puts the older change first, which is the order this case needs: {answered}"
    );
    assert_eq!(days("thing:epsilon"), Some(14.into()), "{answered}");
    assert_eq!(days("thing:zeta"), Some(3.into()), "{answered}");
}

/// **Each object carries its own readiness when `ready` is asked beside
/// `unreviewed`.** The readiness pass hands back one verdict per object in the
/// order it found them, and the review pass then sorts the objects by when they
/// last changed, so the verdicts have to follow their objects and not their
/// places. The ready piece was written first here, so the two orders disagree.
#[tokio::test]
async fn the_plan_beside_unreviewed_serves_each_piece_its_own_readiness() {
    let jojobot = crate::harness::handler_shipped();
    let acting = run_on(&jojobot, "2026-03-15").await;
    work_with(&jojobot, "work:zeta", &[("status", "next")]).await;
    work_with(
        &jojobot,
        "work:epsilon",
        &[("status", "next"), ("depends_on", "work:zeta")],
    )
    .await;

    let answered = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                view: Some("plan".into()),
                unreviewed: Some(true),
                sid: Some(acting),
                ..of_nothing()
            }))
            .await
            .expect("recall answers"),
    );
    assert_eq!(
        listed(&answered),
        ["work:zeta", "work:epsilon"],
        "the review pass puts the older change first, which is the order this case needs: {answered}"
    );
    assert_eq!(
        listed_object(&answered, "work:zeta")["ready"],
        true,
        "{answered}"
    );
    assert_eq!(
        listed_object(&answered, "work:epsilon")["ready"],
        false,
        "{answered}"
    );
}

/// A loop with only the keys given, made under a thing.
async fn a_loop_holding(jojobot: &Jojobot, handle: &str, keys: &[(&str, &str)]) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", handle, handle)
        }))
        .await
        .expect("add ok");
    if keys.is_empty() {
        return;
    }
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                keys.iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            ),
            ..capture_args(&format!("rhythm:{handle}"), "the loop, set up")
        },
    )
    .await;
}

/// **The owed pass marks every object it was asked of, the ones it drops too.**
/// One loop per way a due day reads, asked as of 2026-08-10: late, due that day,
/// a day ahead (dropped, still marked), a cadence never checked in, no schedule
/// at all, and a schedule half written. The kept ones are the owed ones; the marks
/// are all six.
#[tokio::test]
async fn the_owed_pass_marks_every_object_it_was_asked_of_whether_or_not_it_keeps_it() {
    use crate::memory::view_run::{self, DueMark};
    use jojobot_domain::attention;

    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over(memory.clone());
    a_rhythm(&jojobot, "alpha", "7", "2026-08-01").await;
    a_rhythm(&jojobot, "omega", "7", "2026-08-03").await;
    a_rhythm(&jojobot, "sigma", "90", "2026-08-01").await;
    a_loop_holding(
        &jojobot,
        "kappa",
        &[("cadence_days", "7"), ("advances_from", "due_date")],
    )
    .await;
    a_loop_holding(&jojobot, "theta", &[]).await;
    // A schedule half written: no verb writes it now, so it goes past the guard.
    a_loop_holding(&jojobot, "lambda", &[]).await;
    memory.fields_past_the_guard(
        &EntityId("rhythm:lambda".into()),
        &[("advances_from", "due_date")],
    );

    let question = view_run::Question::new(
        jojobot_domain::memory::graph::Selection {
            kind: Some(EntityKind::RHYTHM),
            ..Default::default()
        },
        jojobot_domain::memory::graph::Include::default(),
        None,
        None,
    );
    let mut selected = view_run::select(memory.as_ref(), &[], &question.query, None)
        .await
        .unwrap_or_else(|_| panic!("the rhythms are selected"));
    let carriers = attention::shipped();
    let carriers: Vec<&dyn attention::Carrier> =
        carriers.iter().map(std::convert::AsRef::as_ref).collect();
    let day = "2026-08-10".parse().expect("a day");
    let pass = view_run::keep_owed(
        &mut selected.objects,
        &carriers,
        &std::collections::BTreeMap::new(),
        day,
        day,
    );

    let mark = |handle: &str| -> DueMark {
        *pass
            .marks
            .get(&EntityId(format!("rhythm:{handle}")))
            .unwrap_or_else(|| panic!("rhythm:{handle} has a mark"))
    };
    let on = |day: &str| attention::Due::On(day.parse().expect("a day"));
    assert_eq!(
        pass.marks.len(),
        6,
        "every object is marked: {:?}",
        pass.marks
    );
    assert_eq!(
        (mark("alpha").due, mark("alpha").days_overdue),
        (on("2026-08-08"), Some(2))
    );
    assert_eq!(
        (mark("omega").due, mark("omega").days_overdue),
        (on("2026-08-10"), Some(0))
    );
    assert_eq!(
        (mark("sigma").due, mark("sigma").days_overdue),
        (on("2026-10-30"), Some(-81)),
        "a loop not yet due is dropped from the found and still marked"
    );
    assert_eq!(mark("kappa").due, attention::Due::NotYetOpened);
    assert_eq!(mark("theta").due, attention::Due::Never);
    assert_eq!(mark("lambda").due, attention::Due::Unreadable);
    assert_eq!(mark("lambda").days_overdue, None);

    let mut kept: Vec<_> = selected
        .objects
        .iter()
        .map(|object| object.entity.id.0.clone())
        .collect();
    kept.sort();
    assert_eq!(
        kept,
        ["rhythm:alpha", "rhythm:lambda", "rhythm:omega"],
        "what the pass keeps is unchanged by the marks"
    );
    assert_eq!(pass.excluded, 3);
}

/// **A key's history names the writer and the run of each write.** A capture
/// from a booted bot names both. The write that names neither is the storage
/// contract's case, `a_keys_history_names_the_writer_and_run_of_each_write`.
#[tokio::test]
async fn the_history_of_a_key_names_the_writer_and_the_run() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    let acting = sid_of(&json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-03-15".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("the boot call is ok"),
    ))
    .expect("a boot that states a day hands back a handle");
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("donuts_eaten".to_string(), "1".to_string())]
                    .into_iter()
                    .collect(),
            ),
            sid: Some(acting.clone()),
            ..capture_args("alpha", "ate one")
        },
    )
    .await;

    let asked = recall_json(
        &jojobot,
        RecallArgs {
            history: Some("donuts_eaten".into()),
            ..of("alpha")
        },
    )
    .await;
    let writes = &asked["objects"][0]["history"]["writes"];
    assert_eq!(writes[0]["writer"], "bot:otto", "{asked}");
    assert_eq!(
        writes[0]["run"],
        run_name(&jojobot, &acting),
        "the booted capture names its run by the public name: {asked}"
    );
}

/// **A recall over a kind serves every readable thing when one thing's document
/// cannot be read, and names the unreadable one** (rule 130). One damaged record
/// must not blind a bot to the rest, nor to which one is broken. The handler that
/// cannot scan `work:gamma` and the healthy one share a store, so the healthy
/// answer names all three and the other answers the two it can read whole.
#[tokio::test]
async fn a_recall_over_a_kind_serves_the_readable_things_and_names_the_unreadable_one() {
    let (healthy, blind) =
        crate::memory::testing::shipped_healthy_and_down(Down::ScanOf("work:gamma"));
    let _ = writing_as(&healthy);
    for (slug, status) in [("alpha", "next"), ("gamma", "next"), ("delta", "now")] {
        healthy
            .add_entity(Parameters(AddEntityArgs {
                sets: Some(std::collections::BTreeMap::from([(
                    "status".to_string(),
                    status.to_string(),
                )])),
                ..add_args("work", slug, slug)
            }))
            .await
            .expect("add ok");
    }
    let args = || RecallArgs {
        kind: Some("work".into()),
        ..of_nothing()
    };

    let whole = recall_json(&healthy, args()).await;
    assert_eq!(whole["count"], 3, "{whole}");
    assert!(whole.get("unreadable").is_none(), "{whole}");

    let served = recall_json(&blind, args()).await;
    let mut readable = handles(&served);
    readable.sort();
    assert_eq!(
        readable,
        vec!["work:alpha".to_string(), "work:delta".to_string()],
        "{served}"
    );
    assert_eq!(served["count"], 2, "{served}");
    assert_eq!(
        listed_object(&served, "work:alpha")["fields"]["status"],
        "next",
        "{served}"
    );
    let unreadable = served["unreadable"]
        .as_array()
        .expect("the damage is named");
    assert_eq!(unreadable.len(), 1, "{served}");
    assert_eq!(unreadable[0]["handle"], "work:gamma", "{served}");
    assert_eq!(unreadable[0]["fix_by"], "person", "{served}");
    assert!(
        unreadable[0]["unreadable"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "{served}"
    );
}

/// **The ready view serves every readable piece of work when one cannot be read,
/// and names the unreadable one**, as a recall over a kind does (rule 130).
#[tokio::test]
async fn the_ready_and_plan_views_serve_the_readable_work_and_name_the_unreadable_one() {
    let (healthy, blind) =
        crate::memory::testing::shipped_healthy_and_down(Down::ScanOf("work:gamma"));
    let _ = writing_as(&healthy);
    for slug in ["alpha", "gamma", "delta"] {
        healthy
            .add_entity(Parameters(AddEntityArgs {
                sets: Some(std::collections::BTreeMap::from([(
                    "status".to_string(),
                    "next".to_string(),
                )])),
                ..add_args("work", slug, slug)
            }))
            .await
            .expect("add ok");
    }
    for view in ["ready", "plan"] {
        let args = || RecallArgs {
            view: Some(view.into()),
            ..of_nothing()
        };
        let whole = recall_json(&healthy, args()).await;
        assert_eq!(whole["count"], 3, "{view}: {whole}");
        let served = recall_json(&blind, args()).await;
        let mut readable = handles(&served);
        readable.sort();
        assert_eq!(
            readable,
            vec!["work:alpha".to_string(), "work:delta".to_string()],
            "{view}: {served}"
        );
        assert_eq!(
            served["unreadable"][0]["handle"], "work:gamma",
            "{view}: {served}"
        );
        assert_eq!(
            served["unreadable"][0]["fix_by"], "person",
            "{view}: {served}"
        );
    }
}

/// 🚨 **A roots question beside a key filter the index decides still asks whether
/// each parent stands.** The filter keeps the work whose status is `now`, so the
/// index hands the walk those candidates and no others; a standing project that
/// holds no status is not among them, and its child is not a root for that. The
/// positive is in the same answer: the work with no parent is.
#[tokio::test]
async fn roots_beside_an_indexed_filter_find_a_parent_outside_the_candidates() {
    use jojobot_adapters::search::{IndexedMemory, Retrieval};
    use jojobot_domain::memory::{NewEntity, NewFact};

    let indexed = Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index"));
    let jojobot = Jojobot::new(
        indexed.memory(),
        Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()])),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    );
    let day = jiff::civil::date(2026, 10, 1);
    for (handle, name, parent, status) in [
        ("project:atlas", "Atlas", None, None),
        ("work:beta", "Beta", Some("project:atlas"), Some("now")),
        ("work:gamma", "Gamma", None, Some("now")),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "user-named");
        new.parent = parent.map(|p| EntityId(p.into()));
        let added = indexed.add_entity(new).await;
        assert!(
            matches!(added, Ok(jojobot_domain::memory::Guarded::Written(_))),
            "{added:?}"
        );
        if let Some(status) = status {
            let mut note = jojobot_domain::memory::testing::contract::state(NewFact {
                fields: [("status".to_string(), status.to_string())].into(),
                ..NewFact::about(EntityId(handle.into()), "where it stands", day)
            });
            note.sets = note.fields.keys().cloned().collect();
            let written = indexed.capture(note).await;
            assert!(
                matches!(written, Ok(jojobot_domain::memory::Guarded::Written(_))),
                "{written:?}"
            );
        }
    }

    let answer = recall_json(
        &jojobot,
        RecallArgs {
            roots: Some(true),
            kind: Some("work".into()),
            fields: Some(vec![KeyFilterArgs {
                key: Some("status".into()),
                value: Some("now".into()),
                compare: None,
                scope: Some("thing".into()),
            }]),
            ..of_nothing()
        },
    )
    .await;
    let ids: Vec<&str> = answer["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        ids,
        ["work:gamma"],
        "the child of a standing project outside the candidates is not a root: {answer}"
    );
}
