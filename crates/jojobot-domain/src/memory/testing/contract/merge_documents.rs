//! **Merging two documents leaves one document holding both** (card 2396),
//! proven against any [`Memory`] adapter. The folded document's sections follow
//! the survivor's last section in their own order, every long section stays
//! whole, and no chunk continues a heading that is not on the survivor. A
//! document merged into a thing, or a thing into a document, is refused with the
//! route. Each refusal is read beside the merge that lands.

use super::support::{add, capture, other_caller};
use super::*;
use crate::memory::document::{self, Section};

/// The text of the survivor, three sections.
const ALPHA: &str = "# Alpha one\n\nthe first body\n\n# Alpha two\n\nthe second body\n\n\
                     # Alpha three\n\nthe third body\n";

/// The text of the folded document: a long section, cut into a heading and
/// chunks when it is stored, and a short one.
const BETA: &str = "# Beta long\n\nfirst paragraph of the long one\n\nsecond paragraph of the \
                    long one\n\nthird paragraph of the long one\n\n# Beta short\n\nshort body\n";

/// **Store a document a section per claim.** A section too long for `room`
/// characters is cut into a heading and chunks, each chunk naming the claim that
/// holds its heading. Returns how many records were written.
async fn put_text<M: Memory>(store: &M, subject: &EntityId, text: &str, room: usize) -> usize {
    let sections = document::split(text).expect("the text splits");
    let mut pieces: Vec<Vec<Section>> = Vec::new();
    for section in sections {
        pieces.push(
            document::fit(section, &mut |s| {
                document::join(std::slice::from_ref(s)).len() <= room
            })
            .expect("the section can be cut small enough"),
        );
    }
    let records: usize = pieces.iter().map(Vec::len).sum();
    let mut orders = crate::memory::order::spread(None, None, records)
        .expect("room")
        .into_iter();
    for chunks in pieces {
        let mut head: Option<FactId> = None;
        for chunk in chunks {
            let mut fields = document::record_fields(&chunk, &orders.next().expect("an order"));
            if let Some(head) = &head {
                fields.insert(document::CONTINUES.to_string(), head.0.clone());
            }
            let written = capture(
                store,
                NewFact {
                    details: (!chunk.body.is_empty()).then(|| chunk.body.clone()),
                    fields,
                    session: Some("session-document".to_string()),
                    ..NewFact::about(
                        subject.clone(),
                        chunk.first_line.clone(),
                        date(2026, 10, 14),
                    )
                },
            )
            .await;
            head.get_or_insert(written.id);
        }
    }
    records
}

/// **A document's live sections in reading order**, with the record each is
/// and the heading a chunk carries on, as a reader of the served claims finds
/// them.
fn read_sections(facts: &[Fact]) -> Vec<(Fact, Section, Option<String>)> {
    let mut found: Vec<(String, Fact, Section, Option<String>)> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter_map(|fact| {
            let (order, section) =
                document::from_record(&fact.content, fact.details.as_deref(), &fact.fields)?;
            let head = fact
                .fields
                .get(document::CONTINUES)
                .filter(|value| !value.is_empty() && value.as_str() != "false")
                .cloned();
            Some((order, fact.clone(), section, head))
        })
        .collect();
    found.sort_by(|a, b| (&a.0, &a.1.id.0).cmp(&(&b.0, &b.1.id.0)));
    found
        .into_iter()
        .map(|(_, fact, section, head)| (fact, section, head))
        .collect()
}

/// The text of the sections read, joined in reading order.
fn text_of(sections: &[(Fact, Section, Option<String>)]) -> String {
    let parts: Vec<Section> = sections.iter().map(|(_, s, _)| s.clone()).collect();
    document::join(&parts)
}

/// **Beta folded into alpha: alpha's three sections, then beta's two, the long
/// one whole, and every chunk continues a heading that is on alpha.**
///
/// The two documents are written apart, each with its own record ids, so the
/// merge gives each folded record a new id under the survivor. The text read
/// back is the survivor's followed by the folded one's, byte for byte, which
/// holds only if the folded sections sort after the survivor's and every chunk
/// still finds its heading. **Paired**: before the merge each document reads as
/// its own text, so what is read after is the merge's doing.
pub async fn two_documents_merged_leave_one_document_holding_both_in_order<M: Memory>(store: &M) {
    let alpha = EntityId("document:contract-merged-into".into());
    let beta = EntityId("document:contract-merged-away".into());
    // Named by hand: the guard takes a document whose slug is a plain word for the
    // same name as a thing of another kind that bears it, and another case in
    // this shared store gives a person the name `Beta`.
    for (id, name) in [
        (&alpha, "The Survivor Document"),
        (&beta, "The Folded Document"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let alpha_records = put_text(store, &alpha, ALPHA, 1_000).await;
    let beta_records = put_text(store, &beta, BETA, 60).await;
    assert!(
        beta_records > 2,
        "the long section was cut into a heading and chunks: {beta_records} records"
    );

    let before_alpha = read_sections(&store.recall(&alpha).await.expect("alpha reads"));
    let before_beta = read_sections(&store.recall(&beta).await.expect("beta reads"));
    assert_eq!(text_of(&before_alpha), ALPHA, "alpha reads as its text");
    assert_eq!(text_of(&before_beta), BETA, "beta reads as its text");
    let alpha_writes_before = store
        .key_trails(&alpha)
        .await
        .expect("alpha's trails read")
        .record_writes;

    let done = store
        .merge(
            &beta,
            &alpha,
            Some("one document under two names"),
            date(2026, 10, 14),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("the documents merge");
    assert_eq!(done.rehomed, beta_records, "every folded record moved");

    let facts = store.recall(&alpha).await.expect("alpha reads after");
    let after = read_sections(&facts);
    assert_eq!(
        after.len(),
        alpha_records + beta_records,
        "alpha holds its own sections and beta's"
    );
    assert_eq!(
        text_of(&after),
        format!("{ALPHA}{BETA}"),
        "alpha's sections, then beta's in their own order, the long one whole"
    );
    // The survivor's own sections stand where they were, in front.
    let own: Vec<&str> = after[..alpha_records]
        .iter()
        .map(|(_, s, _)| s.first_line.as_str())
        .collect();
    assert_eq!(own, ["# Alpha one", "# Alpha two", "# Alpha three"]);

    // **The writes behind a section wear the id its record has on the survivor.**
    // A read of the document as of a day rebuilds each record from them, so a
    // folded record's writes under the id it had before the merge would land on
    // whichever survivor record holds that id. Three things hold: no write names
    // an id the survivor does not serve, the survivor's own writes are all still
    // there, and the newest write of each section key is what the record holds.
    let trails = store
        .key_trails(&alpha)
        .await
        .expect("alpha's trails read after")
        .record_writes;
    let served: std::collections::BTreeSet<&FactId> = facts.iter().map(|f| &f.id).collect();
    for write in &trails {
        assert!(
            served.contains(&write.fact),
            "a write of {} names a record alpha does not serve: {trails:?}",
            write.key
        );
    }
    for write in &alpha_writes_before {
        assert!(
            trails
                .iter()
                .any(|w| w.fact == write.fact && w.key == write.key && w.value == write.value),
            "alpha's own write of {} on {} is gone: {trails:?}",
            write.key,
            write.fact.0
        );
    }
    for (fact, _, _) in &after {
        for key in [
            document::ORDER,
            document::LEVEL,
            document::EDGES,
            document::CONTINUES,
        ] {
            let newest = trails
                .iter()
                .filter(|w| w.fact == fact.id && w.key == key)
                .max_by_key(|w| w.ordinal)
                .and_then(|w| w.value.clone());
            assert_eq!(
                newest.as_deref(),
                fact.fields.get(key).map(String::as_str),
                "the newest write of {key} on {} is what the record holds: {:?}",
                fact.id.0,
                trails
                    .iter()
                    .filter(|w| w.fact == fact.id && w.key == key)
                    .collect::<Vec<_>>()
            );
        }
    }

    // **No chunk continues an id outside the document**: each names a heading
    // among alpha's served records, and a heading is no chunk itself.
    let headings: std::collections::BTreeSet<&str> = after
        .iter()
        .filter(|(_, _, head)| head.is_none())
        .map(|(fact, _, _)| fact.id.0.as_str())
        .collect();
    let chunks: Vec<&(Fact, Section, Option<String>)> =
        after.iter().filter(|(_, _, head)| head.is_some()).collect();
    assert!(
        chunks.len() >= 2,
        "the long section's chunks are on alpha: {chunks:?}"
    );
    for (fact, _, head) in &chunks {
        let head = head.as_deref().expect("a chunk names its heading");
        assert!(
            headings.contains(head),
            "{} continues {head}, which is no heading on alpha: {headings:?}",
            fact.id.0
        );
    }
    // **And it names its OWN heading**: a stale id can name some other heading
    // that happens to wear it on the survivor, so each chunk continues the
    // heading nearest in front of it.
    let mut heading: Option<&str> = None;
    for (fact, _, head) in &after {
        match head {
            None => heading = Some(fact.id.0.as_str()),
            Some(head) => assert_eq!(
                Some(head.as_str()),
                heading,
                "{} continues a heading other than the one in front of it",
                fact.id.0
            ),
        }
    }
}

/// The text of the first link of a merge chain: a long section, cut into a
/// heading and chunks when it is stored, and a short one.
const LINK_FIRST: &str = "# First long\n\nfirst link opening paragraph\n\nfirst link middle \
                          paragraph\n\nfirst link closing paragraph\n\n# First short\n\nshort \
                          body of the first\n";

/// The text of the second link, cut the same way.
const LINK_SECOND: &str = "# Second long\n\nsecond link opening paragraph\n\nsecond link \
                           middle paragraph\n\nsecond link closing paragraph\n\n# Second \
                           short\n\nshort body of the second\n";

/// The text of the document the chain ends in.
const LINK_THIRD: &str = "# Third one\n\nbody of the third\n\n# Third two\n\nmore of the third\n";

/// **Three documents merged in a chain serve each section once, in order, and
/// every chunk continues the heading in front of it.** The first folds into the
/// second, then the second into the third, so the second's served records are
/// partly its own and partly the first's, still stored under the first's key. The
/// text read back is the third's, then the second's, then the first's, byte for
/// byte. **Paired**: after the first merge the second reads its own text and the
/// first's, so what the second merge did is read against a chain that was whole.
pub async fn a_chain_of_three_document_merges_serves_each_section_once_in_order<M: Memory>(
    store: &M,
) {
    let first = EntityId("document:contract-chain-first".into());
    let second = EntityId("document:contract-chain-second".into());
    let third = EntityId("document:contract-chain-third".into());
    for (id, name) in [
        (&first, "The Chain Opener"),
        (&second, "The Chain Middle"),
        (&third, "The Chain Closer"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let first_records = put_text(store, &first, LINK_FIRST, 60).await;
    let second_records = put_text(store, &second, LINK_SECOND, 60).await;
    let third_records = put_text(store, &third, LINK_THIRD, 1_000).await;
    assert!(
        first_records > 2 && second_records > 2,
        "the long sections were cut into chunks: {first_records} and {second_records}"
    );

    store
        .merge(
            &first,
            &second,
            Some("one document under two names"),
            date(2026, 10, 14),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("the first merge lands");
    let middle = read_sections(&store.recall(&second).await.expect("second reads"));
    assert_eq!(
        text_of(&middle),
        format!("{LINK_SECOND}{LINK_FIRST}"),
        "after the first merge the second holds its own sections, then the first's"
    );

    store
        .merge(
            &second,
            &third,
            Some("one document under two names"),
            date(2026, 10, 14),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("the second merge lands");

    let facts = store.recall(&third).await.expect("third reads after");
    let after = read_sections(&facts);
    assert_eq!(
        after.len(),
        third_records + second_records + first_records,
        "each section is served once"
    );
    assert_eq!(
        text_of(&after),
        format!("{LINK_THIRD}{LINK_SECOND}{LINK_FIRST}"),
        "the third's sections, then the second's, then the first's"
    );
    let mut heading: Option<&str> = None;
    for (fact, _, head) in &after {
        match head {
            None => heading = Some(fact.id.0.as_str()),
            Some(head) => assert_eq!(
                Some(head.as_str()),
                heading,
                "{} continues a heading other than the one in front of it",
                fact.id.0
            ),
        }
    }
    let trails = store
        .key_trails(&third)
        .await
        .expect("third's trails read after")
        .record_writes;
    let served: std::collections::BTreeSet<&FactId> = facts.iter().map(|f| &f.id).collect();
    for write in &trails {
        assert!(
            served.contains(&write.fact),
            "a write of {} names a record the third does not serve: {trails:?}",
            write.key
        );
    }
}

/// **A document merged into a thing, or a thing into a document, is refused
/// with the route, and nothing moves.** A merge of a document into a thing would
/// leave sections that belong to no document; a thing into a document would
/// leave claims no reader of the document would take for sections. **Paired**:
/// two things merge, so the refusal is about documents and not about merging.
pub async fn a_merge_of_a_document_and_a_thing_is_refused_with_the_route<M: Memory>(store: &M) {
    let document = EntityId("document:contract-docmerge".into());
    let thing = EntityId("topic:contract-xylophone".into());
    let twin = EntityId("topic:contract-zeppelin".into());
    put_text(store, &document, "# A heading\n\nsome body\n", 1_000).await;
    capture(
        store,
        NewFact::about(thing.clone(), "a claim on the thing", date(2026, 10, 14)),
    )
    .await;
    capture(
        store,
        NewFact::about(twin.clone(), "a claim on the twin", date(2026, 10, 14)),
    )
    .await;

    for (folded, survivor) in [(&document, &thing), (&thing, &document)] {
        let refused = store
            .merge(
                folded,
                survivor,
                None,
                date(2026, 10, 14),
                &other_caller(),
                Some("session-merge"),
            )
            .await
            .expect_err("a document and a thing do not merge");
        let said = refused.to_string();
        assert!(
            said.contains("document"),
            "the refusal says it is about documents: {said}"
        );
    }
    // Nothing moved: the document still reads as its own text, and neither
    // handle was folded.
    let kept = read_sections(&store.recall(&document).await.expect("reads"));
    assert_eq!(text_of(&kept), "# A heading\n\nsome body\n");
    let held = store.list_entities(None).await.expect("lists");
    for side in [&document, &thing] {
        assert!(
            held.iter()
                .any(|e| &e.id == side && e.merged_into.is_none()),
            "{side} was not folded"
        );
    }

    // The positive the refusals depend on: two things do merge.
    store
        .merge(
            &twin,
            &thing,
            None,
            date(2026, 10, 14),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("two things merge");
}
