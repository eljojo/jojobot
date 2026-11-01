//! **The words and small decisions the operator's window and the verbs share**
//! (rule 498: one wording, never two copies). Each takes plain values and returns
//! text or a decision, so a page can call it without the store and without the MCP
//! crate; the MCP crate re-exports them from where they used to live.

use jojobot_domain::memory::document::{self, Section};
use jojobot_domain::memory::{EntityId, Fact, FactStatus};

/// The record that holds the instance's own settings.
const INSTANCE_RECORD: &str = jojobot_domain::memory::INSTANCE_RECORD;

/// The key on the instance record that holds the operator, as the handle of a
/// person.
const INSTANCE_OPERATOR_KEY: &str = jojobot_domain::memory::OPERATOR;

/// The key on the instance record that holds the zone, as an IANA name.
pub const ZONE_KEY: &str = "timezone";

/// **The frame an unzoned read is answered in, from the instance's folded
/// fields** a reader already holds: UTC when they hold no zone or one this build
/// cannot resolve. The same frame [`unzoned_frame`] answers.
pub fn frame_of_fields(fields: &std::collections::BTreeMap<String, String>) -> jiff::tz::TimeZone {
    fields
        .get(ZONE_KEY)
        .map(|raw| raw.trim())
        .filter(|raw| !raw.is_empty())
        .and_then(|raw| jiff::tz::TimeZone::get(raw).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC)
}

/// **The sentence for a record that names something other than a person as the
/// operator.** The operator is a person, so the handle the record holds does not
/// count, and the sentence says what it holds, its kind, and where a person is
/// written. The entity exists, so it never says there is none. The boot and a
/// post to that handle both carry this one sentence, so they cannot disagree.
pub fn operator_is_not_a_person(held: &EntityId) -> String {
    let kind = held.kind().map_or("unknown kind", |kind| kind.as_token());
    format!(
        "the instance's record holds '{held}' as its operator, which is a {kind} and not a \
         person, so there is no operator yet: capture {INSTANCE_OPERATOR_KEY} in sets with a \
         person's handle on {INSTANCE_RECORD}"
    )
}

/// **The sentence for an operator who owns more than one mailbox.** One person has
/// exactly one, so several is damage and not a choice: no caller picks one of them,
/// and a person has to repair it. The verbs' refusal and the operator's page both
/// say this sentence, so they cannot tell a caller different things (rule 498).
pub fn operator_owns_several_boxes(names: &[String]) -> String {
    format!(
        "The operator owns more than one mailbox ({}), and one person has exactly one. This is \
         stored damage rather than a mistake in any call: nothing sent through a verb or this \
         page changes it, and a person who runs this instance has to repair it. A session that \
         meets it reports it.",
        names.join(", ")
    )
}

/// **What a bot has for a mailbox.** One box per bot is settled, so the other
/// two answers are damage rather than variety — and they are apart from each
/// other because a bot with no box and a bot with two are different repairs.
pub enum OwnBox {
    /// The one box it owns.
    The(jojobot_domain::mailbox::MailboxName),
    /// It owns none. A bot's box opens with the bot, so this is a creation
    /// that was interrupted or a record predating the rule.
    None,
    /// It owns more than one, which nothing should be able to produce.
    Several(Vec<jojobot_domain::mailbox::MailboxName>),
    /// The board could not be read, so this says nothing about the bot.
    Unreadable,
}

/// **The ownership question, over a board already read.** Apart from the fetch
/// so that a caller holding the listing can answer it — and anything else the
/// same listing answers — without asking the store for the table again.
pub fn owned_box(boxes: &[jojobot_domain::mailbox::Mailbox], bot: &EntityId) -> OwnBox {
    let mut owned: Vec<jojobot_domain::mailbox::MailboxName> = boxes
        .iter()
        .filter(|held| &held.owner == bot)
        .map(|held| held.name.clone())
        .collect();
    match owned.len() {
        1 => OwnBox::The(owned.pop().expect("one box")),
        0 => OwnBox::None,
        _ => OwnBox::Several(owned),
    }
}

/// One live section of a document: the record, its order key and the section it
/// holds.
pub struct Held<'a> {
    pub order: String,
    pub section: Section,
    pub fact: &'a Fact,
    /// For a chunk that carries a section on: the local id of the record that
    /// holds that section's heading.
    pub head: Option<&'a str>,
}

/// **The live sections among a document's records, in reading order.** A section
/// that was archived or taken back is not live: it stays readable by its address
/// and is left out of the page. **A chunk whose heading is not live is not live
/// either**: archiving a section takes the whole of it out. Ties in the order
/// key, which only a hand-written key can make, fall back to the record's id so
/// the page is still the same page every time it is read.
pub fn live_sections(facts: &[Fact]) -> Vec<Held<'_>> {
    let mut held: Vec<Held<'_>> = document::placed_in_order(facts)
        .into_iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter_map(|fact| {
            let (order, section) =
                document::from_record(&fact.content, fact.details.as_deref(), &fact.fields)?;
            Some(Held {
                order,
                section,
                fact,
                head: fact
                    .fields
                    .get(document::CONTINUES)
                    .map(String::as_str)
                    .filter(|_| facts_continue(fact)),
            })
        })
        .collect();
    let heads: std::collections::BTreeSet<&str> = held
        .iter()
        .filter(|h| h.head.is_none())
        .map(|h| h.fact.id.0.as_str())
        .collect();
    held.retain(|h| h.head.is_none_or(|head| heads.contains(head)));
    held
}

/// **The markdown of one live section**, joined from its parts. This is the one
/// join of a section's text: `read_document` pages over it and the web page
/// concatenates it, so the two cannot disagree about what a document says.
pub fn section_text(held: &Held<'_>) -> String {
    document::join(std::slice::from_ref(&held.section))
}

/// **A document's whole live text**, as `read_document` returns it when it
/// fits in one answer: the live sections in reading order, each joined by
/// [`section_text`]. For a reader that is not a verb, such as the web page.
pub fn live_text(facts: &[Fact]) -> String {
    live_sections(facts).iter().map(section_text).collect()
}

fn facts_continue(fact: &Fact) -> bool {
    fact.fields
        .get(document::CONTINUES)
        .is_some_and(|value| !value.is_empty() && value != "false")
}

/// **What a reader that lists only entities and claims says of the hits it holds and does
/// not list**: how many, and where they are read. The count leads, so a reader can take it
/// off the line. The reader asks for no mail and no asker, so the only hits it holds and
/// does not list are in prose, and prose is the one source the line names. The way to read
/// them is the search verb, whose answer lists every hit kind, so the page and the verb name
/// one place.
pub fn not_listed_note(count: usize) -> String {
    format!(
        "{count} found in prose, which this page does not list. The search verb returns each \
         of them as a hit."
    )
}
