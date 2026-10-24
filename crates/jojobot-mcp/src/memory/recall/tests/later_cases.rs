//! Recall cases added after the file of recall cases reached its size: the walk over
//! a dependency an older record wrote as its own field, and the cap a page puts on
//! the history of a run. Kept apart so that file holds its size.

use super::*;

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
