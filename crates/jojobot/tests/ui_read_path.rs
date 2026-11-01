//! Declared type fit is retired from the node page under rule 742, while the
//! folded fields remain readable.

mod support;

use support::ui_board::{CLIENT_ID, READER, read, spawn_jojobot};
use support::{browser, log_in, spawn_idp};

use jojobot_domain::memory::{EntityId, Memory};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicUsize, Ordering},
};

struct UnreadableParts {
    inner: Arc<dyn Memory>,
    failed: AtomicU8,
    history_reads: AtomicUsize,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for UnreadableParts {
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        if address.home.as_str() == "topic:widgets" {
            self.history_reads.fetch_add(1, Ordering::SeqCst);
            if self.failed.load(Ordering::SeqCst) & 4 != 0 {
                return Err(MemoryError::Store("history unavailable".into()));
            }
        }
        self.inner.claim_history(address).await
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        if entity.as_str() == "topic:widgets" && self.failed.load(Ordering::SeqCst) & 1 != 0 {
            return Err(MemoryError::Store("ages unavailable".into()));
        }
        self.inner.field_ages(entity).await
    }

    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<jojobot_domain::memory::search::DocScan>, MemoryError> {
        if entity.as_str() == "topic:widgets" && self.failed.load(Ordering::SeqCst) & 2 != 0 {
            return Err(MemoryError::Store("scan unavailable".into()));
        }
        self.inner.scan_entity(entity).await
    }
}

fn question() -> jojobot_query_api::Question {
    let mut question = jojobot_query_api::Question::new(
        jojobot_query_api::Selection {
            subject: Some(EntityId("topic:widgets".into())),
            ..Default::default()
        },
        jojobot_query_api::Include {
            facts: true,
            prose: true,
            stood_for: true,
            status: None,
        },
        None,
        None,
    );
    question.asks.backing = true;
    question
}

#[tokio::test]
async fn node_read_path_keeps_unreadable_parts_in_the_shared_answer_and_each_renderer() {
    use support::ui_board::{seeded_board_over, seeded_memory, spawn_jojobot_over};
    let memory = Arc::new(UnreadableParts {
        inner: seeded_memory().await,
        failed: AtomicU8::new(0),
        history_reads: AtomicUsize::new(0),
    });
    use jojobot_domain::memory::filter::{Direction, Order};
    use jojobot_domain::memory::{EntityKind, NewEntity, NewFact, graph};
    for (handle, key, value) in [
        ("topic:beta", "pitch", "15"),
        ("view:bare-listing", "selects", "topic"),
    ] {
        let id = EntityId(handle.into());
        memory
            .inner
            .add_entity(NewEntity::new(id.clone(), handle, "the fixture roster"))
            .await
            .unwrap();
        let mut fact = NewFact::about(
            id,
            "the question's fields",
            jiff::civil::Date::constant(2026, 3, 10),
        );
        fact.fields = [(key.into(), value.into())].into();
        fact.sets = [key.into()].into();
        memory.inner.capture(fact).await.unwrap();
    }
    memory
        .inner
        .set_prose(&EntityId("topic:widgets".into()), "alpha", None)
        .await
        .unwrap();
    let board = seeded_board_over(memory.clone()).await;
    let queries = jojobot_mcp::memory::view_run::handle(
        memory.clone(),
        board.sessions.clone(),
        support::ui_board::search_over(memory.clone()),
    );
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    for failed in 0..8 {
        memory.failed.store(failed, Ordering::SeqCst);
        let shared = queries
            .ask(&jojobot_domain::actor::Actor::Operator, question())
            .await
            .unwrap_or_else(|_| panic!("the shared answer keeps unreadability in row {failed}"));
        let scan_unreadable = shared
            .selected
            .unreadable_scans
            .contains_key(&EntityId("topic:widgets".into()));
        let object = shared
            .selected
            .subject_object(&EntityId("topic:widgets".into()))
            .unwrap();
        assert_eq!(object.fields.get("pitch").map(String::as_str), Some("14"));
        assert_eq!(object.ages.is_some(), failed & 1 == 0);
        assert_eq!(object.ages_unreadable.is_some(), failed & 1 != 0);
        assert_eq!(scan_unreadable, failed & 2 != 0);
        assert_eq!(object.revisions_unreadable.is_some(), failed & 4 != 0);
        assert!(!object.facts.is_empty());
        if failed & 4 == 0 {
            for fact in &object.facts {
                assert_eq!(object.fact_revisions[&fact.id], 1);
            }
        }
        assert_eq!(
            object.prose.as_deref(),
            Some(if failed & 2 == 0 { "alpha" } else { "" })
        );
        let page = read(&client, addr, "/person:alpha/topic:widgets/", &cookie).await;
        assert_eq!(
            page.status().as_u16(),
            if failed & 1 != 0 { 502 } else { 200 }
        );
        if failed & 1 == 0 {
            let body = page.text().await.unwrap();
            assert_eq!(body.contains("<pre>alpha</pre>"), failed & 2 == 0);
            // **The prose that could not be read is said so**, never left out as if
            // the thing had none.
            assert_eq!(
                body.contains("id=\"unreadable-the-prose-of-this-thing\""),
                failed & 2 != 0,
                "the prose is marked unreadable exactly when its scan failed, row {failed}"
            );
        }
        // **The view page is a reader too.** A scan that cannot be read is said on
        // the page in its own section, never drawn as an empty answer. Revisions are
        // read only for the facts a question asks, which this view does not.
        let listing = read(&client, addr, "/view:bare-listing/", &cookie).await;
        assert_eq!(listing.status().as_u16(), 200, "view page in row {failed}");
        let listing_body = listing.text().await.unwrap();
        assert_eq!(
            listing_body.contains("id=\"unreadable-the-records-it-selects\""),
            failed & 2 != 0,
            "view page, row {failed}: {listing_body}"
        );
        assert_eq!(
            listing_body.contains("topic:widgets"),
            failed & 2 == 0,
            "the view's answer lists the topic only when its records read, row {failed}"
        );
        for view in [None, Some("bare-listing")] {
            let recall = recall_body(addr, view).await;
            if failed & 6 != 0 {
                // **The verb names the thing it cannot read and serves the rest**: the
                // subject is the one unreadable thing, so no object comes back and the
                // damage is named with `fix_by: person`, never an error for the call.
                assert!(recall.get("code").is_none(), "{recall}");
                assert_eq!(
                    recall["objects"].as_array().map(Vec::len),
                    Some(0),
                    "{recall}"
                );
                assert_eq!(
                    recall["unreadable"][0]["handle"], "topic:widgets",
                    "{recall}"
                );
                assert_eq!(recall["unreadable"][0]["fix_by"], "person", "{recall}");
            } else {
                assert_eq!(recall["objects"][0]["fields"]["pitch"], "14", "{recall}");
                assert!(recall.get("unreadable").is_none(), "{recall}");
            }
        }
        assert_eq!(
            graph::walk(memory.as_ref(), &[], &question().query)
                .await
                .is_err(),
            failed & 6 != 0
        );
        let mut browse = question().query;
        browse.select.subject = None;
        browse.select.kind = Some(EntityKind::TOPIC);
        browse.select.order = Some(Order::new("pitch", Direction::Ascending));
        let head = memory
            .inner
            .ordered_first_k(
                "pitch",
                jojobot_domain::memory::types::ValueType::Number,
                Direction::Ascending,
                Some(EntityKind::TOPIC),
                None,
                1,
            )
            .await
            .unwrap();
        assert!(head.cut);
        assert_eq!(head.head[0].0.as_str(), "topic:widgets");
        browse.select.ordered_head = Some(head);
        for budget in [0, usize::MAX] {
            let before = memory.history_reads.load(Ordering::SeqCst);
            let selected = jojobot_mcp::memory::view_run::select(
                memory.as_ref(),
                &[],
                &browse,
                Some(graph::Window {
                    offset: 0,
                    budget,
                    size_of: &|_| 1,
                }),
            )
            .await
            .unwrap();
            assert_eq!(selected.left_over, usize::from(budget == 0));
            let widget = selected
                .subject_object(&EntityId("topic:widgets".into()))
                .unwrap();
            assert_eq!(widget.revisions_unreadable.is_some(), failed & 4 != 0);
            assert_eq!(
                memory.history_reads.load(Ordering::SeqCst) - before,
                widget.facts.len(),
                "the cached object is not read twice"
            );
            assert_eq!(
                widget.fact_revisions.len(),
                if failed & 4 == 0 {
                    widget.facts.len()
                } else {
                    0
                }
            );
        }
    }
    ct.cancel();
}

async fn recall_body(addr: std::net::SocketAddr, view: Option<&str>) -> serde_json::Value {
    let http = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");
    let (session, _) = support::open_with(&http, &url, "2025-06-18").await;
    let mut request = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(
            &serde_json::json!({"jsonrpc":"2.0", "id":2, "method":"tools/call",
            "params":{"name":"recall", "arguments":{"subject":"topic:widgets", "facts":true, "view":view}}}),
        );
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let body = request.send().await.unwrap().text().await.unwrap();
    support::tool_body(&support::event_stream_result(&body))
}

#[tokio::test]
async fn node_read_path_backing_follows_rewrites_omits_sum_winners_and_keeps_cleared_fields_absent()
{
    use jojobot_domain::memory::types::{DeclaredType, Field};
    use jojobot_domain::memory::{FactPatch, NewFact, Provenance};
    use support::ui_board::{seeded_board_over, seeded_memory};
    let memory = seeded_memory().await;
    let id = EntityId("topic:widgets".into());
    memory
        .declare_type(
            DeclaredType::new("donut-eaten", vec![Field::summing("donuts")]),
            None,
        )
        .await
        .unwrap();
    let mut addresses = Vec::new();
    for (value, count, provenance) in [
        ("5", "1", Provenance::Testimony),
        ("7", "2", Provenance::Inference),
    ] {
        let mut claim = NewFact::about(
            id.clone(),
            "the price and donut count",
            jiff::civil::Date::constant(2026, 3, 10),
        );
        claim.session = Some("alpha-run".into());
        claim.provenance = provenance;
        claim.fields = [
            ("price".into(), value.into()),
            ("donuts".into(), count.into()),
            ("cleared".into(), "value".into()),
        ]
        .into();
        claim.sets = claim.fields.keys().cloned().collect();
        addresses.push(
            memory
                .capture(claim)
                .await
                .unwrap()
                .written()
                .unwrap()
                .address(),
        );
    }
    let board = seeded_board_over(memory.clone()).await;
    let queries = jojobot_mcp::memory::view_run::handle(
        memory.clone(),
        board.sessions.clone(),
        support::ui_board::search_over(memory.clone()),
    );
    let read = queries
        .ask(&jojobot_domain::actor::Actor::Operator, question())
        .await
        .unwrap_or_else(|_| panic!("the read succeeds"));
    assert_eq!(
        read.backing[&id].as_ref().unwrap()["price"].fact,
        addresses[1].local
    );
    memory
        .update_fact(
            &addresses[0],
            FactPatch {
                fields: [("price".into(), "6".into())].into(),
                sets: ["price".into()].into(),
                session: Some("alpha-run".into()),
                ..Default::default()
            },
            &EntityId("person:alpha".into()),
        )
        .await
        .unwrap();
    memory
        .update_fact(
            &addresses[1],
            FactPatch {
                clear_fields: vec!["cleared".into()],
                session: Some("alpha-run".into()),
                ..Default::default()
            },
            &EntityId("person:alpha".into()),
        )
        .await
        .unwrap();
    let read = queries
        .ask(&jojobot_domain::actor::Actor::Operator, question())
        .await
        .unwrap_or_else(|_| panic!("the read succeeds"));
    assert_eq!(
        read.backing[&id].as_ref().unwrap()["price"].fact,
        addresses[0].local
    );
    assert_eq!(
        read.backing[&id].as_ref().unwrap()["price"].provenance,
        Provenance::Testimony
    );
    assert!(!read.backing[&id].as_ref().unwrap().contains_key("donuts"));
    assert!(!read.backing[&id].as_ref().unwrap().contains_key("cleared"));
    let object = read.selected.subject_object(&id).unwrap();
    assert_eq!(object.fields.get("price").map(String::as_str), Some("6"));
    assert_eq!(object.fields.get("donuts").map(String::as_str), Some("3"));
    assert!(!object.fields.contains_key("cleared"));
}

#[tokio::test]
async fn node_read_path_keeps_folded_fields_without_type_fit() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let response = read(&client, addr, "/person:alpha/topic:widgets/", &cookie).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.unwrap();
    let fields = body
        .split_once("id=\"fields\"")
        .expect("the folded fields are present")
        .1
        .split_once("</table>")
        .expect("the fields table closes")
        .0;
    assert!(fields.contains("opens") && fields.contains("2026-03-05"));
    assert!(fields.contains("pitch") && fields.contains(">14<"));
    assert!(!body.contains("id=\"conforms\""), "{body}");
    ct.cancel();
}

#[tokio::test]
async fn node_key_history_shows_every_write_oldest_first_and_names_no_bot_verb() {
    use jojobot_domain::memory::{NewFact, graph};
    use support::ui_board::{seeded_board_over, seeded_memory, spawn_jojobot_over};
    let memory = seeded_memory().await;
    let id = EntityId("topic:widgets".into());
    let writes = graph::WRITES_SHOWN + 5;
    for n in 0..writes {
        let mut claim = NewFact::about(
            id.clone(),
            "a gate write",
            jiff::civil::Date::constant(2026, 3, 10),
        );
        claim.fields = [("gate_count".into(), format!("{n}"))].into();
        claim.sets = ["gate_count".into()].into();
        memory.capture(claim).await.unwrap();
    }
    let board = seeded_board_over(memory.clone()).await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = read(
        &client,
        addr,
        "/person:alpha/topic:widgets/?history=gate_count",
        &cookie,
    )
    .await
    .text()
    .await
    .unwrap();
    let table = page
        .split_once("id=\"history\"")
        .expect("the history table is present")
        .1
        .split_once("</table>")
        .expect("the history table closes")
        .0;
    assert_eq!(table.matches("<tr><td>").count(), writes);
    let first = table.split_once("<tr><td>").unwrap().1;
    assert!(first.starts_with("0<"), "oldest first: {first}");
    let last = table.rsplit_once("<tr><td>").unwrap().1;
    assert!(last.starts_with(&format!("{}<", writes - 1)), "{last}");
    assert!(
        !page.contains("history-left-out") && !page.contains("history_most"),
        "the page cuts nothing and names no bot verb: {page}"
    );
    ct.cancel();
}

#[tokio::test]
async fn node_key_history_shows_a_clear_as_a_clear() {
    use jojobot_domain::memory::{FactPatch, NewFact};
    use support::ui_board::{seeded_board_over, seeded_memory, spawn_jojobot_over};
    let memory = seeded_memory().await;
    let id = EntityId("topic:widgets".into());
    let mut claim = NewFact::about(
        id.clone(),
        "a gate count",
        jiff::civil::Date::constant(2026, 3, 10),
    );
    claim.fields = [("gate_count".into(), "7".into())].into();
    claim.sets = ["gate_count".into()].into();
    let address = memory
        .capture(claim)
        .await
        .unwrap()
        .written()
        .unwrap()
        .address();
    memory
        .update_fact(
            &address,
            FactPatch {
                clear_fields: vec!["gate_count".into()],
                ..Default::default()
            },
            &EntityId("person:alpha".into()),
        )
        .await
        .unwrap();
    let board = seeded_board_over(memory.clone()).await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = read(
        &client,
        addr,
        "/person:alpha/topic:widgets/?history=gate_count",
        &cookie,
    )
    .await
    .text()
    .await
    .unwrap();
    let table = page
        .split_once("id=\"history\"")
        .expect("the history table is present")
        .1
        .split_once("</table>")
        .expect("the history table closes")
        .0;
    assert!(table.contains("<em>taken off</em>"), "{table}");
    ct.cancel();
}

/// **A scan that cannot be read is said in the Around section, never drawn as a
/// cell with fewer neighbours.** Both halves in one test through the real route:
/// the readable scan shows the neighbour the stall points at, and the failing
/// scan shows the unreadable section and no neighbour table.
#[tokio::test]
async fn node_around_says_so_when_the_scan_cannot_be_read() {
    use support::ui_board::{seeded_board_over, seeded_memory, spawn_jojobot_over};
    let memory = Arc::new(UnreadableParts {
        inner: seeded_memory().await,
        failed: AtomicU8::new(0),
        history_reads: AtomicUsize::new(0),
    });
    let board = seeded_board_over(memory.clone()).await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    for failed in [0, 2] {
        memory.failed.store(failed, Ordering::SeqCst);
        let page = read(&client, addr, "/person:alpha/topic:widgets/", &cookie).await;
        assert_eq!(page.status().as_u16(), 200, "row {failed}");
        let body = page.text().await.unwrap();
        // The town is linked from the facts as well, so only the Around table counts.
        let around = body
            .split_once("id=\"around\"")
            .and_then(|(_, rest)| rest.split_once("</table>"))
            .map_or("", |(table, _)| table);
        assert_eq!(
            around.contains("href=\"/place:shelbyville/\""),
            failed == 0,
            "the neighbour shows in Around only when the scan reads, row {failed}: {body}"
        );
        assert_eq!(
            body.contains("id=\"unreadable-what-is-around-this\""),
            failed != 0,
            "row {failed}: {body}"
        );
    }
    ct.cancel();
}

/// A run of `bot` that began `n` minutes after the epoch, with `beats` beats.
async fn begin_run(
    sessions: &jojobot_domain::session::testing::InMemorySessions,
    bot: &str,
    n: i64,
    beats: usize,
) {
    use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};
    let at = |minute: i64| jiff::Timestamp::from_second(1_700_000_000 + minute * 60).unwrap();
    let run = sessions
        .begin(NewSession {
            timezone: None,
            bot: EntityId(bot.into()),
            sid: Sid(format!("r{n:03}")),
            focus: format!("focus-{n:02}"),
            started_at: at(n),
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("the run begins");
    for beat in 0..beats {
        sessions
            .append(
                &run.id,
                NewEntry::manual(format!("beat-{n:02}-{beat}"), at(n), None),
            )
            .await
            .expect("the beat is recorded");
    }
}

/// **Runs asked of the shared boundary** as the asker, the way the page asks.
async fn runs_asked(
    queries: &jojobot_query_api::QueryHandle,
    asker: &jojobot_domain::actor::Actor,
    bot: &EntityId,
    chronologies: usize,
) -> Result<jojobot_query_api::Runs, jojobot_query_api::RunsFailed> {
    let asked = jojobot_query_api::Question::of_runs(jojobot_query_api::RunsAsk {
        bot: Some(bot.clone()),
        chronologies,
        ..Default::default()
    });
    queries
        .ask(asker, asked)
        .await
        .unwrap_or_else(|_| panic!("a runs question walks nothing and cannot fail there"))
        .runs
        .expect("the answer carries the runs asked for")
}

/// **The operator reads a bot's runs through the shared boundary: every run,
/// newest first, and the chronologies of the newest few that wrote any.** A run
/// with no beat is in the list and has no chronology, so the count of
/// chronologies is of runs with beats and not of runs.
#[tokio::test]
async fn the_operator_reads_a_bots_runs_newest_first_with_the_chronologies_of_the_newest_few() {
    use jojobot_domain::actor::Actor;
    let board = support::ui_board::seeded_board().await;
    for n in 1..=5 {
        begin_run(&board.sessions, "bot:lambda", n, 2).await;
    }
    // The newest run wrote nothing: it is in the table and has no chronology.
    begin_run(&board.sessions, "bot:lambda", 6, 0).await;
    let queries = jojobot_mcp::memory::view_run::handle(
        board.memory.clone(),
        board.sessions.clone(),
        support::ui_board::search_over(board.memory.clone()),
    );

    let read = runs_asked(
        &queries,
        &Actor::Operator,
        &EntityId("bot:lambda".into()),
        2,
    )
    .await
    .expect("the operator reads a bot's runs");
    let focuses: Vec<&str> = read
        .summaries
        .iter()
        .map(|run| run.focus.as_str())
        .collect();
    assert_eq!(
        focuses,
        [
            "focus-06", "focus-05", "focus-04", "focus-03", "focus-02", "focus-01"
        ],
        "every run, newest first"
    );
    let written: Vec<String> = read
        .chronologies
        .iter()
        .map(|run| run.entries[0].text.clone())
        .collect();
    assert_eq!(
        written,
        ["beat-05-0", "beat-04-0"],
        "the chronologies of the two newest runs that wrote a beat, and none for the empty one"
    );
}

/// **A bot reads its own runs and no other bot's, and an app reads none.** Beside
/// the refusals the same asker reads what it may, so a refusal is not an
/// empty read.
#[tokio::test]
async fn a_bot_reads_its_own_runs_not_another_bots_and_an_app_reads_none() {
    use jojobot_domain::actor::Actor;
    use jojobot_query_api::RunsFailed;
    let board = support::ui_board::seeded_board().await;
    begin_run(&board.sessions, "bot:lambda", 1, 1).await;
    begin_run(&board.sessions, "bot:omega", 2, 1).await;
    let queries = jojobot_mcp::memory::view_run::handle(
        board.memory.clone(),
        board.sessions.clone(),
        support::ui_board::search_over(board.memory.clone()),
    );
    let lambda = EntityId("bot:lambda".into());

    let own = runs_asked(&queries, &Actor::Bot(lambda.clone()), &lambda, 5)
        .await
        .expect("a bot reads its own runs");
    assert_eq!(own.summaries.len(), 1);

    let other = runs_asked(
        &queries,
        &Actor::Bot(EntityId("bot:omega".into())),
        &lambda,
        5,
    )
    .await;
    assert!(
        matches!(other, Err(RunsFailed::NotYours)),
        "another bot's runs are not a bot's to read: {other:?}"
    );
    let app = runs_asked(
        &queries,
        &Actor::App(jojobot_domain::app::AppId::parse("springfield-watch").unwrap()),
        &lambda,
        5,
    )
    .await;
    assert!(
        matches!(app, Err(RunsFailed::NotYours)),
        "an app reads no run: {app:?}"
    );
}

/// **A read that names no bot reads no run and still counts them, and what a bot
/// is served is the days of the runs it is given, not of every bot.** The counts
/// are what a browse is told it was not shown.
#[tokio::test]
async fn a_read_with_no_bot_counts_and_the_served_days_are_those_of_the_runs_returned() {
    use jojobot_domain::actor::Actor;
    use jojobot_domain::session::Sessions;
    let board = support::ui_board::seeded_board().await;
    begin_run(&board.sessions, "bot:lambda", 1, 1).await;
    begin_run(&board.sessions, "bot:omega", 2, 1).await;
    let queries = jojobot_mcp::memory::view_run::handle(
        board.memory.clone(),
        board.sessions.clone(),
        support::ui_board::search_over(board.memory.clone()),
    );
    let lambda = EntityId("bot:lambda".into());
    let day = jiff::civil::date(2026, 9, 2);
    for bot in ["bot:lambda", "bot:omega"] {
        let run = board
            .sessions
            .summaries_of(&EntityId(bot.into()))
            .await
            .expect("the runs are read")
            .remove(0);
        board.sessions.add_served(&run.id, 100, day).await.unwrap();
    }

    let asked = |bot: Option<EntityId>| {
        jojobot_query_api::Question::of_runs(jojobot_query_api::RunsAsk {
            bot,
            counts: true,
            served: true,
            ..Default::default()
        })
    };
    let counted = queries
        .ask(&Actor::System("probe".into()), asked(None))
        .await
        .unwrap_or_else(|_| panic!("a runs question cannot fail in the walk"))
        .runs
        .expect("the answer carries the runs")
        .unwrap_or_else(|_| panic!("a read that names no bot is not refused"));
    assert!(counted.summaries.is_empty(), "no bot named, no run read");
    assert_eq!(counted.counts.get(&lambda), Some(&1));
    assert_eq!(counted.counts.get(&EntityId("bot:omega".into())), Some(&1));

    let own = queries
        .ask(&Actor::Bot(lambda.clone()), asked(Some(lambda.clone())))
        .await
        .unwrap_or_else(|_| panic!("a runs question cannot fail in the walk"))
        .runs
        .expect("the answer carries the runs")
        .unwrap_or_else(|_| panic!("a bot reads its own runs"));
    let ids: Vec<_> = own.summaries.iter().map(|run| &run.id).collect();
    assert_eq!(ids.len(), 1, "only the bot's own run");
    assert_eq!(
        own.served.iter().map(|held| &held.run).collect::<Vec<_>>(),
        ids,
        "the days served are those of the run returned, not of another bot's"
    );
}
