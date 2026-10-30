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
    let queries = jojobot_mcp::memory::view_run::handle(memory.clone(), board.sessions.clone());
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    for failed in 0..8 {
        memory.failed.store(failed, Ordering::SeqCst);
        let shared = queries
            .ask(question())
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
        }
        for view in [None, Some("bare-listing")] {
            let recall = recall_body(addr, failed & 6 != 0, view).await;
            if failed & 6 != 0 {
                assert_eq!(recall["code"], -32603, "{recall}");
            } else {
                assert_eq!(recall["objects"][0]["fields"]["pitch"], "14", "{recall}");
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

async fn recall_body(
    addr: std::net::SocketAddr,
    unreadable: bool,
    view: Option<&str>,
) -> serde_json::Value {
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
    if unreadable {
        support::event_stream_error(&body)
    } else {
        support::tool_body(&support::event_stream_result(&body))
    }
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
    let queries = jojobot_mcp::memory::view_run::handle(memory.clone(), board.sessions.clone());
    let read = queries
        .ask(question())
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
        .ask(question())
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
