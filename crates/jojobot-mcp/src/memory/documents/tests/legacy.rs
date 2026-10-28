//! Literal records from before a reader/writer spelling change must still read.

use super::*;
use jojobot_domain::memory::NewFact;

/// **Old section keys read in order, with their whitespace and continuation.**
/// The fixture names stored keys literally and never calls the document writer.
#[tokio::test]
async fn literal_legacy_section_records_keep_their_order_edges_and_continuation() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    let document = EntityId("document:alpha".into());
    let head = jojobot
        .memory
        .capture(NewFact {
            details: Some("body".into()),
            fields: [
                ("level".into(), "1".into()),
                ("order".into(), "V".into()),
                ("edges".into(), ";;\\n\\n;\\n\\n".into()),
            ]
            .into(),
            ..NewFact::about(document.clone(), "# First", jiff::civil::date(2026, 10, 1))
        })
        .await
        .unwrap()
        .written()
        .unwrap();
    let chunk = jojobot
        .memory
        .capture(NewFact {
            fields: [
                ("level".into(), "0".into()),
                ("order".into(), "a".into()),
                ("edges".into(), ";;;\\n".into()),
                ("continues".into(), head.id.0.clone()),
            ]
            .into(),
            ..NewFact::about(
                document.clone(),
                "carried paragraph",
                jiff::civil::date(2026, 10, 1),
            )
        })
        .await
        .unwrap()
        .written()
        .unwrap();
    jojobot
        .memory
        .capture(NewFact {
            fields: [
                ("level".into(), "1".into()),
                ("order".into(), "G".into()),
                ("edges".into(), ";;;\\n".into()),
            ]
            .into(),
            ..NewFact::about(document.clone(), "# Second", jiff::civil::date(2026, 10, 1))
        })
        .await
        .unwrap()
        .written()
        .unwrap();
    let read = read_part(&jojobot, "document:alpha", None).await;
    assert_eq!(
        read["markdown"], "# Second\n# First\n\nbody\n\ncarried paragraph\n",
        "{read}"
    );
    assert_eq!(
        jojobot_domain::memory::document::from_record(
            &head.content,
            head.details.as_deref(),
            &head.fields,
        )
        .unwrap()
        .1
        .level,
        1,
        "the literal heading depth is read by the section decoder",
    );
    let archived = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("archived".into()),
            details: Some("retired section".into()),
            ..update_args(&head.address().to_string())
        }))
        .await
        .unwrap();
    assert_ne!(json_of(&archived)["status"], "blocked", "{archived:?}");
    let held = jojobot.memory.recall(&document).await.unwrap();
    assert_eq!(
        held.iter().find(|fact| fact.id == head.id).unwrap().status,
        FactStatus::Archived
    );
    assert_eq!(
        held.iter().find(|fact| fact.id == chunk.id).unwrap().status,
        FactStatus::Active
    );
    let read = read_part(&jojobot, "document:alpha", None).await;
    assert_eq!(read["markdown"], "# Second\n", "{read}");
    assert_eq!(read["archived_sections_left_out"], 2, "{read}");
}
