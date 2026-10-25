use super::*;

/// **A claim the thing cannot serve is named in its object, with the reason and
/// the address that reads it**, and the object of a thing with none says
/// nothing of the kind. **Paired with the claims it does serve**, which stay in
/// the same object, so a rendering that dropped the listing to make room for
/// the note cannot pass.
#[tokio::test]
async fn an_object_names_the_claims_it_cannot_serve_beside_the_claims_it_can() {
    use jojobot_domain::memory::search::DocScan;
    use jojobot_domain::memory::unmapped::UnmappedClaim;
    let memory = shared_memory();
    let jojobot = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    let sid = writing_as(&jojobot);
    ensure(&jojobot, "person:milhouse").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            ..capture_args("person:milhouse", "plays the cornet")
        },
    )
    .await;
    let milhouse = captured(&memory, "person:milhouse").await;
    let facts = memory.recall(&milhouse.id).await.expect("recall reads");
    let doc = |unmapped: Vec<UnmappedClaim>| DocScan {
        doc_id: "milhouse".into(),
        title: milhouse.name.clone(),
        prose: String::new(),
        entity: Some(milhouse.clone()),
        facts: facts.clone(),
        fields: Default::default(),
        owner: None,
        field_links: Default::default(),
        unmapped,
    };
    let stray = UnmappedClaim {
        address: jojobot_domain::memory::FactAddress::new(
            jojobot_domain::memory::EntityId("person:bart".into()),
            jojobot_domain::memory::FactId("f9".into()),
        ),
        merge_no: 3,
    };
    let query = graph::GraphQuery {
        select: graph::Selection {
            subject: Some(milhouse.id.clone()),
            ..graph::Selection::default()
        },
        include: graph::Include {
            facts: true,
            prose: false,
            stood_for: false,
            status: None,
        },
        follow: None,
        history: None,
        masked: Default::default(),
    };
    let zone = jiff::tz::TimeZone::UTC;
    let frame = DayFrame {
        clock: jojobot_domain::clock::Clock::stating(jiff::civil::date(2026, 10, 14)),
        zone: &zone,
    };
    let render = |unmapped: Vec<UnmappedClaim>| {
        let answer = graph::resolve(&[doc(unmapped)], &[], &query).expect("a subject selects");
        object_json(
            &answer.objects[0],
            query.include,
            jiff::civil::date(2026, 10, 14),
            None,
            frame,
            false,
            None,
        )
    };

    let named = render(vec![stray]);
    let left_out = &named["claims_left_out"];
    assert_eq!(left_out["count"], 1, "{named}");
    assert_eq!(left_out["claims"][0]["address"], "person:bart#f9");
    assert_eq!(left_out["claims"][0]["reason"], "unmapped by merge event 3");
    assert_eq!(
        named["facts"].as_array().map(Vec::len),
        Some(1),
        "the claim it serves is still listed: {named}"
    );
    let plain = render(Vec::new());
    assert!(
        plain.get("claims_left_out").is_none(),
        "nothing is left out of a thing nobody merged into: {plain}"
    );
}
