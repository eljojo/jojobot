//! The mail scope of a search costs the boxes it reads, not the mail it holds.
//!
//! The query used to carry one clause per readable message, so a bot high on
//! the chart paid for every message ever sent on each search with mail.

use std::collections::BTreeSet;

use jojobot_domain::mailbox::{Mailbox, MailboxName, MessageState, visibility::visible_boxes};
use jojobot_domain::memory::{EntityId, EntityKind, search::SearchQuery};

use super::tests::{index_of, message};
use super::*;

/// Alpha manages the boxes `pm` and `inbox`; `beside` belongs to a bot that is
/// not on alpha's chart.
fn alpha_asking(text: &str) -> SearchQuery {
    let caller = EntityId("bot:alpha".into());
    let chart = ["pm", "inbox"]
        .map(|name| (EntityId::new(EntityKind::BOT, name), caller.to_string()))
        .into();
    let board = ["pm", "inbox", "beside"].map(|name| Mailbox {
        name: MailboxName(name.into()),
        owner: EntityId::new(EntityKind::BOT, name),
        counts: Default::default(),
        quarantined: Vec::new(),
    });
    SearchQuery {
        include_mail: true,
        mail_visibility: visible_boxes(Some(&caller), &chart, &board),
        ..SearchQuery::text(text)
    }
}

fn mail_in(boxes: &[&str], per_box: usize) -> Vec<Message> {
    boxes
        .iter()
        .flat_map(|name| {
            (0..per_box).map(move |n| {
                message(
                    &format!("{name}-{n}"),
                    name,
                    "bot:marge",
                    None,
                    "scopeneedle",
                    MessageState::New,
                )
            })
        })
        .collect()
}

/// How many terms the query for this search holds.
fn terms_in_query(index: &FullTextIndex, query: &SearchQuery) -> usize {
    let whole = BooleanQuery::new(index.other_clauses(query));
    let mut terms = 0;
    whole.query_terms(&mut |_, _| terms += 1);
    terms
}

/// **The query holds as many terms for 2,000 messages as for 6.** The messages
/// sit in three boxes either way, so what the query names is the boxes.
#[test]
fn the_query_for_mail_does_not_grow_with_the_messages() {
    let few = index_of(vec![]);
    few.ingest_mail(&mail_in(&["pm", "inbox", "beside"], 2))
        .expect("ingest");
    let many = index_of(vec![]);
    many.ingest_mail(&mail_in(&["pm", "inbox", "beside"], 667))
        .expect("ingest");
    let query = alpha_asking("scopeneedle");
    assert_eq!(
        terms_in_query(&many, &query),
        terms_in_query(&few, &query),
        "2,001 messages in three boxes ask for the same terms as 6"
    );
}

/// **Admitting by box still admits what the rule admits, and only that**: the
/// mail in a box alpha manages, and alpha's own mail in a box beside it.
#[test]
fn the_readable_mail_is_found_and_the_unreadable_is_not_in_the_same_read() {
    let index = index_of(vec![]);
    let mut mail = mail_in(&["pm", "inbox", "beside"], 1);
    mail.push(message(
        "authored",
        "beside",
        "bot:alpha",
        None,
        "scopeneedle",
        MessageState::New,
    ));
    index.ingest_mail(&mail).expect("ingest");
    let hits = index
        .search(&alpha_asking("scopeneedle"))
        .expect("search ok");
    let found: BTreeSet<String> = hits
        .iter()
        .filter_map(|hit| match hit {
            Hit::Message { message, .. } => Some(message.id.as_str().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        found,
        BTreeSet::from(["pm-0".into(), "inbox-0".into(), "authored".into()])
    );
}
