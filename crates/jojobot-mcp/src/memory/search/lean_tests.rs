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
    let mut served: CallToolResponse = jojobot
        .search(Parameters(SearchArgs {
            query: Some(query.into()),
            sid: sid.clone(),
            ..search_args()
        }))
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
