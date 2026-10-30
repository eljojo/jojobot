//! The mail scope of a search costs the boxes it reads, not the mail it holds.
//!
//! The query used to carry one clause per readable message, so a bot high on
//! the chart paid for every message ever sent on each search with mail.

use std::collections::{BTreeMap, BTreeSet};

use jojobot_domain::mailbox::{
    Mailbox, MailboxName, MessageState,
    visibility::{every_bot_box, visible_boxes},
};
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
    assert!(
        terms_in_query(&few, &query) > 0,
        "the query for mail names something: a mail clause that was always None would pass the equality below"
    );
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

/// The ids of the messages a query finds in an index.
fn found_by(index: &FullTextIndex, query: &SearchQuery) -> BTreeSet<String> {
    index
        .search(query)
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Message { message, .. } => Some(message.id.as_str().to_string()),
            _ => None,
        })
        .collect()
}

/// Two boxes on a board: a bot's `pm` and the person's `person-milhouse`.
fn pm_and_a_persons_box() -> Vec<Mailbox> {
    vec![
        Mailbox {
            name: MailboxName("pm".into()),
            owner: EntityId("bot:homer".into()),
            counts: Default::default(),
            quarantined: Vec::new(),
        },
        Mailbox {
            name: MailboxName("person-milhouse".into()),
            owner: EntityId("person:milhouse".into()),
            counts: Default::default(),
            quarantined: Vec::new(),
        },
    ]
}

/// **The grader's scope reads every message but those in a private box.** The
/// index here holds a message in each box (written straight in, as no verb
/// would), so the exclusion the query makes is what keeps the person's out.
/// Paired with the bot box's message, which the same read finds.
#[test]
fn the_grader_scope_finds_a_bots_mail_and_leaves_out_a_persons() {
    let index = index_of(vec![]);
    index
        .ingest_mail(&mail_in(&["pm", "person-milhouse"], 1))
        .expect("ingest");
    let grader = SearchQuery {
        include_mail: true,
        mail_visibility: every_bot_box(&pm_and_a_persons_box()),
        ..SearchQuery::text("scopeneedle")
    };
    assert_eq!(found_by(&index, &grader), BTreeSet::from(["pm-0".into()]));
}

/// **A caller who reads no box finds no mail, however much the index holds.**
/// The anonymous caller has no chart place, and a bot with no box on the board
/// reads nothing of anybody's, though it wrote the message it asks for. Each is
/// paired with the grader's read of the same index, which finds the mail.
#[test]
fn a_caller_who_reads_no_box_finds_no_mail_where_the_grader_finds_it() {
    let index = index_of(vec![]);
    let mut mail = mail_in(&["pm"], 1);
    mail.push(message(
        "alphas",
        "pm",
        "bot:alpha",
        None,
        "scopeneedle",
        MessageState::New,
    ));
    index.ingest_mail(&mail).expect("ingest");
    let board = pm_and_a_persons_box();
    let grader = SearchQuery {
        include_mail: true,
        mail_visibility: every_bot_box(&board),
        ..SearchQuery::text("scopeneedle")
    };
    assert_eq!(
        found_by(&index, &grader),
        BTreeSet::from(["pm-0".into(), "alphas".into()]),
        "the index holds the mail the two callers below are asking for"
    );
    let anonymous = SearchQuery {
        include_mail: true,
        mail_visibility: visible_boxes(None, &BTreeMap::new(), &board),
        ..SearchQuery::text("scopeneedle")
    };
    assert!(found_by(&index, &anonymous).is_empty());
    let no_box = SearchQuery {
        include_mail: true,
        mail_visibility: visible_boxes(Some(&EntityId("bot:alpha".into())), &BTreeMap::new(), &[]),
        ..SearchQuery::text("scopeneedle")
    };
    assert!(found_by(&index, &no_box).is_empty());
}
