//! The two bot boxes the mail-index fixtures ask to read.

use jojobot_domain::mailbox::{Mailbox, MailboxName, visibility::visible_boxes};
use jojobot_domain::memory::{EntityId, EntityKind, search::SearchQuery};

pub(super) fn asking_for_mail(text: &str) -> SearchQuery {
    let caller = EntityId("bot:alpha".into());
    let chart = ["pm", "inbox"]
        .map(|name| (EntityId::new(EntityKind::BOT, name), caller.to_string()))
        .into();
    let board = ["pm", "inbox"].map(|name| Mailbox {
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
