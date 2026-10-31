//! The mail State with the index subscribed to it, as the binary wires them.

use super::*;
pub(super) use crate::fold::mail::FoldedMailboxes;

/// Verbs go to the State, and the index reads each change the writer publishes.
pub(super) fn mail_over(
    inner: Arc<dyn Mailboxes>,
    index: Arc<FullTextIndex>,
) -> (Arc<FoldedMailboxes>, Arc<IndexedMailboxes>) {
    let state = Arc::new(FoldedMailboxes::new(inner).with_writer(index.owner()));
    let mail = subscribed(state.clone(), index);
    (state, mail)
}

/// The index over a port whose writes the index's own writer publishes,
/// subscribed to it. A bare store as `read_from` gives a port with no count.
pub(super) fn subscribed(
    read_from: Arc<dyn Mailboxes>,
    index: Arc<FullTextIndex>,
) -> Arc<IndexedMailboxes> {
    let mail = Arc::new(IndexedMailboxes::new(read_from, index.clone()));
    mail.subscribe();
    mail
}
