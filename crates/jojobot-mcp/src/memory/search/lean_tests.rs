use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use jojobot_domain::memory::FactId;

const QUERIES: [(&str, &str); 3] = [
    (
        "one task per Codex session fresh agent wraps when its card is reported",
        "One task per Codex session: each card gets a fresh agent, which wraps when its card is reported.",
    ),
    (
        "size ratchet unknown base",
        "The size ratchet must not pass an unknown base.",
    ),
    (
        "numbered mention collection",
        "A numbered mention resolves the collection's numbered member.",
    ),
];

#[tokio::test]
async fn search_lean_part2_counts_and_pages_distinct_session_passages() {
    use jojobot_adapters::search::{FullTextIndex, IndexedSessions, Retrieval};
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let index = Arc::new(FullTextIndex::open().expect("index"));
    let refresh = Arc::new(IndexedSessions::new(sessions.clone(), index.clone()));
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(Retrieval::new(index, vec![refresh])),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        sessions,
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let sid = writing_as(&jojobot);
    let shared = "damper ".repeat(100);
    let passage = |at| {
        let text = format!("ending {at:02} {shared}");
        jojobot_domain::memory::search::excerpt::snippet(
            &text,
            &jojobot_domain::memory::search::excerpt::literal_matches(&text, "damper"),
        )
    };
    assert_eq!(passage(0), passage(41), "the shown passage is identical");
    for at in 0..2 {
        crate::session::testing::journal_entry(&jojobot, &sid, &format!("ending {at:02} {shared}"))
            .await;
    }
    let body = answer(&jojobot, "damper", Some(sid.clone())).await;
    assert_eq!(body["results"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["total"], 1, "two matching beats are one passage");

    for at in 2..42 {
        crate::session::testing::journal_entry(&jojobot, &sid, &format!("ending {at:02} {shared}"))
            .await;
    }
    for at in 0..6 {
        crate::session::testing::journal_entry(&jojobot, &sid, &format!("damper setting {at}"))
            .await;
    }
    let first = answer(&jojobot, "damper", Some(sid.clone())).await;
    assert_eq!(first["results"].as_array().unwrap().len(), 5, "{first}");
    assert_eq!(first["total"], 7, "{first}");
    assert_eq!(first["not_shown"]["count"], 2, "{first}");
    let next = answer_args(
        &jojobot,
        SearchArgs {
            query: Some("damper".into()),
            sid: Some(sid),
            offset: Some(5),
            ..search_args()
        },
    )
    .await;
    assert_eq!(next["results"].as_array().unwrap().len(), 2, "{next}");
    assert_eq!(next["total"], 7, "{next}");
    let mut passages = std::collections::BTreeSet::new();
    for result in first["results"]
        .as_array()
        .unwrap()
        .iter()
        .chain(next["results"].as_array().unwrap().iter())
    {
        assert!(passages.insert(result["snippet"].as_str().unwrap()));
    }
    assert_eq!(passages.len(), 7);
}

fn fact(content: &str, details: Option<String>) -> Fact {
    Fact {
        app_stamp: None,
        written_by_another: false,
        sets: Default::default(),
        reaching: Default::default(),
        id: FactId("f1".into()),
        home: EntityId("person:alpha".into()),
        subject: EntityId("person:alpha".into()),
        content: content.into(),
        details,
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status: FactStatus::Active,
        recorded_at: jiff::civil::date(2026, 7, 1),
        happened_at: None,
        happened_through: None,
        edge: None,
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: Some("2026-07-01T00:00:00Z".parse().unwrap()),
        stale_after: None,
        receipt: None,
    }
}

fn hit(fact: Fact) -> Hit {
    let parent = EntityRef {
        id: fact.subject.clone(),
        kind: Some(EntityKind::PERSON),
        name: Some("Alpha".into()),
        aliases: Vec::new(),
    };
    Hit::Fact {
        fact: Box::new(fact),
        subject: parent.clone(),
        home: parent,
        source: None,
        superseded_by: None,
    }
}

fn corpus(passage: &str) -> Vec<Hit> {
    let mut answer = fact(
        "A heading",
        Some(format!(
            "{}\n{passage}\n{}",
            "Background. ".repeat(80),
            "Appendix. ".repeat(80)
        )),
    );
    answer.fields.insert("order".into(), "V".into());
    answer.fields.insert("level".into(), "2".into());
    answer.fields.insert("edges".into(), ";;".into());
    let mut hits = vec![hit(answer)];
    for at in 2..=12 {
        let mut noise = fact("A neighboring record", None);
        noise.id = FactId(format!("f{at}"));
        hits.push(hit(noise));
    }
    hits
}

async fn answer(jojobot: &Jojobot, query: &str, sid: Option<String>) -> serde_json::Value {
    answer_args(
        jojobot,
        SearchArgs {
            query: Some(query.into()),
            sid,
            ..search_args()
        },
    )
    .await
}

async fn answer_args(jojobot: &Jojobot, args: SearchArgs) -> serde_json::Value {
    let sid = args.sid.clone();
    let mut served: CallToolResponse = jojobot
        .search(Parameters(args))
        .await
        .expect("served search")
        .into();
    jojobot.finish_unshaped(&mut served, sid.as_deref()).await;
    let CallToolResponse::Complete(result) = served else {
        panic!("complete search answer")
    };
    json_of(&result)
}

fn indexed(hits: Vec<Hit>) -> Jojobot {
    use jojobot_adapters::search::{FullTextIndex, Retrieval};
    use jojobot_domain::memory::{Boot, search::DocScan};
    let entity = Entity {
        folded_names: Vec::new(),
        id: EntityId("person:alpha".into()),
        kind: EntityKind::PERSON,
        name: "Alpha".into(),
        aliases: Vec::new(),
        source: "user-named".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: None,
        archived: None,
    };
    let facts = hits
        .into_iter()
        .map(|hit| match hit {
            Hit::Fact { fact, .. } => *fact,
            _ => panic!("the fixture contains facts"),
        })
        .collect();
    let index = Arc::new(FullTextIndex::open().expect("index"));
    index
        .ingest_doc(
            &DocScan {
                doc_id: "alpha".into(),
                title: "Alpha".into(),
                prose: String::new(),
                entity: Some(entity),
                facts,
                fields: Default::default(),
                owner: None,
                unmapped: Vec::new(),
            },
            &Default::default(),
        )
        .expect("index fixture");
    Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(Retrieval::new(index, Vec::new())),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    )
}

#[tokio::test]
async fn search_lean_measure_fixed_real_queries() {
    for (query, passage) in QUERIES {
        let mut hits = corpus(passage);
        let words: Vec<_> = query.split_whitespace().collect();
        for hit in hits.iter_mut().skip(1) {
            if let Hit::Fact { fact, .. } = hit {
                fact.content = words[..words.len().div_ceil(2)].join(" ");
            }
        }
        let jojobot = indexed(hits);
        let body = answer(&jojobot, query, None).await;
        let matched = body["results"].as_array().unwrap().iter().any(|hit| {
            ["content", "snippet"]
                .iter()
                .any(|key| hit[key].as_str().is_some_and(|text| text.contains(passage)))
        });
        println!(
            "SEARCH_LEAN_MEASURE {}",
            serde_json::json!({"query":query,"matched":matched,"answer":body})
        );
    }
}

#[tokio::test]
async fn search_lean_part1_serves_matching_details_and_content_passages() {
    for (query, passage) in QUERIES {
        for in_details in [false, true] {
            let record = if in_details {
                corpus(passage).remove(0)
            } else {
                hit(fact(passage, None))
            };
            let jojobot = indexed(vec![record]);
            let body = answer(&jojobot, query, None).await;
            let result = &body["results"][0];
            assert!(
                ["content", "snippet"].iter().any(|key| result[key]
                    .as_str()
                    .is_some_and(|text| text.contains(passage))),
                "{body}"
            );
            assert!(result["address"].is_string());
        }
    }
}

#[tokio::test]
async fn search_lean_part1_heading_match_keeps_body_context() {
    let passage = QUERIES[0].1;
    let jojobot = indexed(vec![hit(fact("# Workflow", Some(passage.into())))]);
    let body = answer(&jojobot, "workflow", None).await;
    assert!(
        ["content", "snippet"]
            .iter()
            .any(|key| body["results"][0][key]
                .as_str()
                .is_some_and(|text| text.contains(passage))),
        "{body}"
    );
}

#[tokio::test]
async fn search_lean_part2_pages_five_and_counts_every_remaining_match() {
    let query = QUERIES[1].0;
    let mut hits = Vec::new();
    for at in (1..=37).rev() {
        let mut record = fact(query, None);
        record.id = FactId(format!("f{at}"));
        hits.push(hit(record));
    }
    let jojobot = indexed(hits);
    let first = answer(&jojobot, query, None).await;
    assert_eq!(first["count"], 5, "{first}");
    assert_eq!(first["total"], 37, "{first}");
    assert_eq!(first["not_shown"]["count"], 32, "{first}");
    assert_eq!(first["not_shown"]["offset"], 5, "{first}");
    let next = answer_args(
        &jojobot,
        SearchArgs {
            query: Some(query.into()),
            offset: Some(5),
            ..search_args()
        },
    )
    .await;
    assert_eq!(next["count"], 5, "{next}");
    assert_eq!(next["total"], 37, "{next}");
    assert_eq!(next["not_shown"]["count"], 27, "{next}");
    for result in next["results"].as_array().unwrap() {
        assert!(
            !first["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|before| { before["address"] == result["address"] })
        );
    }
    let empty = answer(&jojobot, "penicillin", None).await;
    assert_eq!(empty["count"], 0);
    assert_eq!(empty["total"], 0);
    assert!(empty.get("not_shown").is_none(), "{empty}");
}

#[tokio::test]
async fn search_lean_part2_more_terms_then_closer_terms_outrank_a_newer_record() {
    let mut dense = fact(
        &format!(
            "{} size ratchet unknown base {}",
            "background ".repeat(120),
            "background ".repeat(120)
        ),
        None,
    );
    dense.id = FactId("f9".into());
    let mut scattered = fact(
        &format!(
            "size {} ratchet {} unknown {} base {}",
            "background ".repeat(60),
            "background ".repeat(60),
            "background ".repeat(60),
            "background ".repeat(60)
        ),
        None,
    );
    scattered.recorded_at = jiff::civil::date(2026, 10, 1);
    let mut sparse = fact(&format!("size ratchet {}", "background ".repeat(240)), None);
    sparse.id = FactId("f2".into());
    sparse.recorded_at = scattered.recorded_at;
    let jojobot = indexed(vec![hit(scattered), hit(sparse), hit(dense)]);
    let body = answer(&jojobot, QUERIES[1].0, None).await;
    let addresses: Vec<_> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["address"].as_str().unwrap())
        .collect();
    assert_eq!(
        addresses,
        ["person:alpha#f9", "person:alpha#f1", "person:alpha#f2"],
        "{body}"
    );
}

#[tokio::test]
async fn search_lean_part3_serves_one_parent_and_no_empty_or_internal_data() {
    let mut record = fact("A heading", Some("the damper stays open".into()));
    record.fields.extend([
        ("edges".into(), ";;".into()),
        ("order".into(), "V".into()),
        ("level".into(), "2".into()),
        ("color".into(), "blue".into()),
    ]);
    record.refs.push(EntityId("person:beta".into()));
    record.edge = Some(Edge::new(
        EdgeShape::Membership,
        EntityId("org:contract-guild".into()),
    ));
    let jojobot = indexed(vec![hit(record)]);
    let control = answer_args(
        &jojobot,
        serde_json::from_value(serde_json::json!({
            "query": "damper", "include_links": true, "details": true
        }))
        .unwrap(),
    )
    .await;
    let result = &control["results"][0];
    assert_eq!(result["content"], "A heading");
    assert_eq!(result["details"], "the damper stays open");
    assert_eq!(result["refs"][0], "person:beta");
    assert_eq!(result["edge"]["object"], "org:contract-guild");
    let body = answer(&jojobot, "damper", None).await;
    let result = &body["results"][0];
    assert_eq!(result["parent"]["id"], "person:alpha", "{body}");
    assert_eq!(result["address"], "person:alpha#f1");
    assert!(result["content"].as_str().unwrap().contains("damper"));
    for key in [
        "about",
        "home",
        "subject",
        "snippet",
        "inserted_at",
        "refs",
        "edge",
    ] {
        assert!(result.get(key).is_none(), "{key}: {body}");
    }
    assert_eq!(result["fields"]["color"], "blue");
    for key in ["edges", "order", "level"] {
        assert!(result["fields"].get(key).is_none(), "{key}: {body}");
    }
    fn no_empty(value: &serde_json::Value) {
        match value {
            serde_json::Value::Array(items) => {
                assert!(!items.is_empty());
                items.iter().for_each(no_empty);
            }
            serde_json::Value::Object(fields) => {
                assert!(!fields.is_empty());
                fields.values().for_each(no_empty);
            }
            serde_json::Value::String(text) => assert!(!text.is_empty()),
            serde_json::Value::Null => panic!("null result field"),
            _ => {}
        }
    }
    no_empty(result);

    let mut foreign = fact("damper", None);
    foreign.subject = EntityId("person:beta".into());
    let foreign = indexed(vec![hit(foreign)]);
    let body = answer(&foreign, "damper", None).await;
    assert_eq!(body["results"][0]["parent"]["id"], "person:alpha");
    assert_eq!(body["results"][0]["subject"]["id"], "person:beta");
    assert_eq!(body["results"][0]["address"], "person:alpha#f1");
}
