//! "Keeping track in a clean way of what a bot is doing is important to me."
//!
//! A bot's `working_on` points at the thing it is doing now. A session declares
//! or clears it through `journal`, and moving work the bot owns to `now` fills
//! it. A reader sees the thing only while the session that wrote it is live.
//!
//! The roster names are the cast: bots are Otto and the assistant the software ships, pieces of work are
//! written in Greek letters.

use serde_json::json;

use super::dsl::{Session, Story};

/// What the bot holds under `working_on` as a reader sees it, or none.
async fn working_on(reader: &Session, bot: &str) -> Option<String> {
    let read = reader.call("recall", json!({"subject": bot})).await;
    read.json()["objects"][0]["fields"]["working_on"]
        .as_str()
        .map(str::to_string)
}

/// **A session declares what its bot is doing, and any reader sees it.** The
/// paired negative is the bot before it declares: no key, not an empty one.
#[tokio::test]
async fn a_session_declares_what_its_bot_is_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    let reader = story.as_bot("bot:assistant").await;
    assert_eq!(working_on(&reader, "bot:otto").await, None);

    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;

    assert_eq!(
        working_on(&reader, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **A session clears it with an empty `working_on`**, and the key goes away
/// rather than reading as an empty string.
#[tokio::test]
async fn a_session_clears_what_its_bot_was_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );

    otto.call(
        "journal",
        json!({"entry": "put sigma down", "working_on": ""}),
    )
    .await;

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

/// A piece of work owned by `owner`, waiting its turn.
async fn owned_work(s: &Session, handle: &str, name: &str, owner: &str) -> String {
    s.add(handle, name).await;
    let made = s
        .call(
            "capture",
            json!({"subject": handle, "content": "waits its turn",
                   "sets": {"owner": owner, "status": "next"}}),
        )
        .await;
    made.json()["address"]
        .as_str()
        .expect("an address")
        .to_string()
}

/// **Moving work the bot owns to `now` points the bot at it**, with no call to
/// `journal`. The paired negative is a move to any other status.
#[tokio::test]
async fn capturing_owned_work_as_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    otto.call(
        "capture",
        json!({"subject": "work:sigma", "content": "queued behind others",
               "provenance": "testimony", "sets": {"status": "waiting"}}),
    )
    .await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "capture",
        json!({"subject": "work:sigma", "content": "starting it",
               "provenance": "testimony", "sets": {"status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **An edit that moves owned work to `now` fills it the same way.**
#[tokio::test]
async fn editing_owned_work_to_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let address = owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "update_fact",
        json!({"address": address, "sets": {"status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **An edit of the thing that moves owned work to `now` fills it the same
/// way**, and an edit that moves it to another status fills nothing.
#[tokio::test]
async fn editing_the_owned_thing_to_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;

    otto.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "waiting"}}),
    )
    .await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **Work created at `now` by the bot that owns it fills it too**, and work
/// created at `now` for another owner fills nothing.
#[tokio::test]
async fn creating_owned_work_as_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;

    otto.call(
        "add_entity",
        json!({"handle": "work:delta", "kind": "work", "name": "Delta",
               "source": "user-named", "sets": {"owner": "bot:assistant", "status": "now"}}),
    )
    .await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "add_entity",
        json!({"handle": "work:sigma", "kind": "work", "name": "Sigma",
               "source": "user-named", "sets": {"owner": "bot:otto", "status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **Another bot moving the work fills nothing for its owner**, and nothing for
/// itself, because it does not own the work.
#[tokio::test]
async fn another_bot_moving_the_work_fills_nothing() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let address = owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    let other = story.as_bot("bot:assistant").await;

    other
        .call(
            "update_fact",
            json!({"address": address, "sets": {"status": "now"}}),
        )
        .await;

    assert_eq!(working_on(&other, "bot:otto").await, None);
    assert_eq!(working_on(&other, "bot:assistant").await, None);
}

/// **A rename of the thing keeps the link**, and the bot reads the new handle.
#[tokio::test]
async fn renaming_the_thing_keeps_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;

    otto.call(
        "rename_entity",
        json!({"handle": "work:sigma", "to": "work:zeta"}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:zeta")
    );
}

/// **After the writing session wraps, a reader sees no `working_on`**, and the
/// answer says why the key is not there. The paired positive is the same read
/// before the wrap.
#[tokio::test]
async fn a_wrapped_session_leaves_working_on_unknown() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    let reader = story.as_bot("bot:assistant").await;
    assert_eq!(
        working_on(&reader, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );

    otto.call("wrap_session", json!({"story": "done for now"}))
        .await;

    let after = reader.call("recall", json!({"subject": "bot:otto"})).await;
    assert_eq!(
        after.json()["objects"][0]["fields"]["working_on"],
        json!(null)
    );
    assert!(
        after.json()["working_on_note"].is_string(),
        "{}",
        after.json()
    );
    let backed = reader
        .call("recall", json!({"subject": "bot:otto", "backing": true}))
        .await;
    assert_eq!(
        backed.json()["objects"][0]["fields_backing"]["working_on"],
        json!(null)
    );
}

/// **The bot's own page shows the key while the writing session is live and
/// says it is unknown after the wrap**, the same answer a `recall` gives.
#[tokio::test]
async fn the_page_of_a_bot_shows_working_on_only_while_its_session_is_live() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    let live = story.page("bot:otto").await;
    live.section("fields").says("working_on").says("work:sigma");
    live.never_says("working-on-unknown");

    otto.call("wrap_session", json!({"story": "done for now"}))
        .await;

    let after = story.page("bot:otto").await;
    after.never_says("history=working_on");
    after.says("working-on-unknown");
}

/// **Only work fills it.** A project the bot owns, moved to `now`, points the
/// bot at nothing.
#[tokio::test]
async fn a_project_moved_to_now_does_not_fill_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("project:atlas", "Sigma").await;

    otto.call(
        "capture",
        json!({"subject": "project:atlas", "content": "starting it",
               "sets": {"owner": "bot:otto", "status": "now"}}),
    )
    .await;

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

/// **A handle that names nothing is refused with the entry unwritten**, so the
/// call can be sent again whole.
#[tokio::test]
async fn a_working_on_that_names_nothing_is_refused() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;

    otto.refused(
        "journal",
        json!({"entry": "picked up a ghost", "working_on": "work:ghost"}),
    )
    .await
    .says("\"wrote\":false")
    .says("work:ghost");

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

/// **Moving owned work to a word the instance maps to `now` fills `working_on`
/// as `now` does**, and a word mapped to anything else, or to nothing, fills
/// nothing.
#[tokio::test]
async fn a_word_the_instance_maps_to_now_fills_working_on_and_no_other_word_does() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    for (handle, name) in [
        ("work:sigma", "Sigma"),
        ("work:phi", "Phi"),
        ("work:kappa", "Kappa"),
    ] {
        owned_work(&otto, handle, name, "bot:otto").await;
    }
    otto.call(
        "capture",
        json!({"subject": "topic:instance", "content": "what the columns mean here",
               "sets": {"status_word/doing": "now", "status_word/later": "next"}}),
    )
    .await;
    let move_to = |thing: &'static str, word: &'static str| {
        let otto = &otto;
        async move {
            otto.call(
                "capture",
                json!({"subject": thing, "content": "moved", "sets": {"status": word}}),
            )
            .await;
        }
    };

    move_to("work:phi", "later").await;
    move_to("work:kappa", "whenever").await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    move_to("work:sigma", "doing").await;
    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
    story.finish().await;
}

/// How the session that said what a bot is working on is gone.
#[derive(Clone, Copy)]
enum Writer {
    /// Still running.
    Live,
    /// Wrapped, with its story told.
    Wrapped,
    /// Stopped without a wrap, and swept by a later boot.
    Abandoned,
}

/// A bot that said it works on `work:sigma`, and a reader of another bot. The
/// story is handed back so it outlives the reads.
async fn a_bot_on_sigma(writer: Writer) -> (Story, Session) {
    match writer {
        Writer::Live | Writer::Wrapped => {
            let story = Story::begin("bot:otto").await;
            let otto = story.session().await;
            otto.add("work:sigma", "Sigma").await;
            otto.call(
                "journal",
                json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
            )
            .await;
            if matches!(writer, Writer::Wrapped) {
                otto.call("wrap_session", json!({"story": "done for now"}))
                    .await;
            }
            let reader = story.as_bot("bot:assistant").await;
            (story, reader)
        }
        Writer::Abandoned => {
            let june = Story::begin_pretending_it_is("bot:otto", "2026-06-01").await;
            let sitting = june.session().await;
            sitting.add("work:sigma", "Sigma").await;
            sitting
                .call(
                    "journal",
                    json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
                )
                .await;
            // The sitting ends without a wrap. A boot a month later sweeps it.
            let july = june.restarted_pretending_it_is("2026-07-20").await;
            let _ = july.full_boot().await;
            let reader = july.as_bot("bot:assistant").await;
            (july, reader)
        }
    }
}

/// The handles of the bots a `recall` of kind bot returns.
async fn bots_answering(reader: &Session, asked: serde_json::Value) -> Vec<String> {
    let mut args = json!({"kind": "bot"});
    for (key, value) in asked.as_object().expect("an object") {
        args[key] = value.clone();
    }
    let answer = reader.call("recall", args).await;
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect()
}

/// **A filter on `working_on` sees the gated value.** A bot whose writer is gone
/// is not returned by a filter that asks for the thing, in either form of it, and
/// is returned by one that asks for a bot working on nothing. While the writer is
/// live it is the other way round.
async fn the_filters_see_the_gated_value(writer: Writer) {
    let (story, reader) = a_bot_on_sigma(writer).await;
    let live = matches!(writer, Writer::Live);
    let asks_for_it = json!({"where": "working_on = work:sigma"});
    let asks_by_field = json!({"fields": [{"key": "working_on", "value": "work:sigma"}]});
    let asks_for_none = json!({"where": "!working_on"});

    for asked in [asks_for_it, asks_by_field] {
        let found = bots_answering(&reader, asked.clone()).await;
        assert_eq!(
            found.iter().any(|bot| bot == "bot:otto"),
            live,
            "{asked}: {found:?}"
        );
    }
    let found = bots_answering(&reader, asks_for_none).await;
    assert_eq!(
        found.iter().any(|bot| bot == "bot:otto"),
        !live,
        "a bot working on nothing: {found:?}"
    );
    story.finish().await;
}

#[tokio::test]
async fn the_filters_return_a_bot_while_its_writer_is_live() {
    the_filters_see_the_gated_value(Writer::Live).await;
}

#[tokio::test]
async fn the_filters_do_not_return_a_bot_whose_writer_wrapped() {
    the_filters_see_the_gated_value(Writer::Wrapped).await;
}

#[tokio::test]
async fn the_filters_do_not_return_a_bot_whose_writer_was_abandoned() {
    the_filters_see_the_gated_value(Writer::Abandoned).await;
}

/// **The records of a bot do not serve a stale working-on record as current.**
/// The record that set the key says its session is gone, and while the writer is
/// live it says nothing of the kind.
async fn the_records_mark_a_stale_working_on(writer: Writer) {
    let (story, reader) = a_bot_on_sigma(writer).await;
    let live = matches!(writer, Writer::Live);

    let answer = reader
        .call("recall", json!({"subject": "bot:otto", "facts": true}))
        .await;
    let facts = answer.json()["objects"][0]["facts"].clone();
    let setting: Vec<&serde_json::Value> = facts
        .as_array()
        .expect("facts")
        .iter()
        .filter(|fact| fact["sets"]["working_on"].is_string())
        .collect();
    assert_eq!(setting.len(), 1, "one record sets it: {facts}");
    assert_eq!(
        setting[0]["working_on_stale"].as_bool().unwrap_or(false),
        !live,
        "{}",
        setting[0]
    );
    story.finish().await;
}

#[tokio::test]
async fn the_records_do_not_mark_a_live_working_on() {
    the_records_mark_a_stale_working_on(Writer::Live).await;
}

#[tokio::test]
async fn the_records_mark_a_working_on_whose_writer_wrapped() {
    the_records_mark_a_stale_working_on(Writer::Wrapped).await;
}

#[tokio::test]
async fn the_records_mark_a_working_on_whose_writer_was_abandoned() {
    the_records_mark_a_stale_working_on(Writer::Abandoned).await;
}

/// **The page of a view that selects on `working_on` answers as `recall` does.**
/// The bot is on the page while its writer is live, and is not once it wrapped.
#[tokio::test]
async fn the_page_of_a_view_selecting_on_working_on_leaves_out_a_bot_whose_writer_wrapped() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    otto.call(
        "add_entity",
        json!({"kind": "view", "handle": "on-sigma", "name": "On sigma",
               "source": "user-named",
               "sets": {"selects": "bot", "where": "working_on = work:sigma"}}),
    )
    .await;
    story.page("view:on-sigma").await.says("bot:otto");

    otto.call("wrap_session", json!({"story": "done for now"}))
        .await;

    story.page("view:on-sigma").await.never_says("bot:otto");
    story.finish().await;
}

/// **A follow that reaches a bot shows its `working_on` only while the session
/// that said it is live.** The piece of work names the bot as its owner, so a
/// walk out from the work reaches the bot by a link that does not depend on
/// `working_on` itself. The bot is reached in every case; what it holds under
/// `working_on` is the question.
async fn a_follow_reaches_a_bot(writer: Writer) {
    let (story, reader) = a_bot_on_sigma(writer).await;
    reader
        .call(
            "capture",
            json!({"subject": "work:sigma", "content": "owned by otto",
                   "sets": {"owner": "bot:otto"}}),
        )
        .await;
    let walked = reader
        .call(
            "recall",
            json!({"subject": "work:sigma", "follow": {"direction": "out"}}),
        )
        .await;
    let body = walked.json();
    let reached = body["objects"][0]["connected"]
        .as_array()
        .and_then(|all| all.iter().find(|one| one["id"] == "bot:otto"))
        .unwrap_or_else(|| panic!("the walk reaches the bot: {body}"));
    let live = matches!(writer, Writer::Live);
    assert_eq!(
        reached["fields"]["working_on"].as_str(),
        live.then_some("work:sigma"),
        "{reached}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_follow_shows_working_on_on_a_bot_whose_writer_is_live() {
    a_follow_reaches_a_bot(Writer::Live).await;
}

#[tokio::test]
async fn a_follow_shows_no_working_on_on_a_bot_whose_writer_wrapped() {
    a_follow_reaches_a_bot(Writer::Wrapped).await;
}

#[tokio::test]
async fn a_follow_shows_no_working_on_on_a_bot_whose_writer_was_abandoned() {
    a_follow_reaches_a_bot(Writer::Abandoned).await;
}

/// **The page of a bot answers as `recall` does for every way the writer is
/// gone.** While the writer is live the page shows the key; once the sitting was
/// abandoned it says the key is unknown, as it does after a wrap.
async fn the_page_of_a_bot(writer: Writer) {
    let (story, _reader) = a_bot_on_sigma(writer).await;
    let page = story.page("bot:otto").await;
    if matches!(writer, Writer::Live) {
        page.section("fields").says("working_on").says("work:sigma");
        page.never_says("working-on-unknown");
    } else {
        page.never_says("history=working_on");
        page.says("working-on-unknown");
    }
    story.finish().await;
}

#[tokio::test]
async fn the_page_of_a_bot_shows_working_on_while_its_writer_is_live() {
    the_page_of_a_bot(Writer::Live).await;
}

#[tokio::test]
async fn the_page_of_a_bot_says_working_on_is_unknown_after_its_writer_was_abandoned() {
    the_page_of_a_bot(Writer::Abandoned).await;
}
