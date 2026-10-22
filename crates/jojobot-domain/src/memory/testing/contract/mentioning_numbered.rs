use super::numbered::ordinances;
use super::support::{capture, ensure};
use super::*;

/// 🚨 **A mention written as a numbered member's address links the member, or is
/// refused, and never links the collection with `/N` left as text.**
///
/// The address is the collection's handle, a slash and the number, which `recall`
/// already takes as a subject (rule 639 cites rules this way). The slug ends at a
/// slash, so the parser read the collection and left `/2` as words: a link to the
/// wrong thing that looked right in the sentence.
///
/// **Both halves**: the member is linked and served as its own handle, and a number
/// nobody holds is refused naming the collection and the number. A mention of the
/// collection alone still links the collection, so the fix did not swallow it.
pub async fn a_mention_written_as_a_numbered_address_links_the_member<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let author = EntityId::person("person:contract-mention-numberer");
    ensure(mentioning, &author).await;
    let o = ordinances(
        mentioning,
        "Mention ordinances",
        [
            "topic:contract-ordinances-mention",
            "decision:contract-ordinance-mention-curfew",
            "decision:contract-ordinance-mention-parking",
            "decision:contract-ordinance-mention-leaves",
            "decision:contract-ordinance-mention-fireworks",
        ],
    )
    .await;

    let written = capture(
        mentioning,
        NewFact::about(
            author.clone(),
            format!(
                "the rule is in @{}/2 and the list in @{}",
                o.collection, o.collection
            ),
            date(2026, 4, 18),
        ),
    )
    .await;
    // The member is served as its own handle; the collection, mentioned alone, as itself.
    assert!(
        written.content.contains(o.parking.as_str()),
        "the numbered address was not followed to the member: {}",
        written.content
    );
    assert!(
        !written.content.contains("/2"),
        "a '/2' stayed behind as text: {}",
        written.content
    );
    assert!(
        written.content.contains(o.collection.as_str()),
        "the collection mentioned alone no longer reads as itself: {}",
        written.content
    );
    let stored = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;
    assert!(
        !stored.contains(o.parking.as_str()) && !stored.contains("/2"),
        "the store kept a handle or the number, not a link: {stored}"
    );

    // A number nobody holds is refused, naming the collection and the number.
    let missing = mentioning
        .capture(NewFact::about(
            author.clone(),
            format!("see @{}/999", o.collection),
            date(2026, 4, 18),
        ))
        .await;
    match missing {
        Ok(Guarded::Blocked { attempted, .. }) => {
            assert!(
                attempted.as_str().contains(o.collection.as_str())
                    && attempted.as_str().contains("999"),
                "the refusal does not name the collection and the number: {attempted}"
            );
        }
        Err(MemoryError::UnknownEntity { attempted, .. }) => {
            assert!(
                attempted.contains(o.collection.as_str()) && attempted.contains("999"),
                "the refusal does not name the collection and the number: {attempted}"
            );
        }
        other => panic!("a number nobody holds must be refused, got {other:?}"),
    }
}
