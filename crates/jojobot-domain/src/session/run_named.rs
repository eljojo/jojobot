//! **The run a stored record names** (rule 498). A write keeps the run's public
//! name in its session. A run the boot minted has its name for its card id, so
//! a lookup by id finds it. A run from before the boot minted names has a card id
//! that is not its name, so the same lookup misses it and the record would name
//! no run. [`find`] is the one lookup every caller uses: by id, else by name.

use super::{SessionError, SessionId, SessionSummary, Sessions};

/// **The run `stored` names**, by its card id first and by its public name when no
/// card carries that id. `None` when neither finds a run. When several runs wear one name, the one that began last
/// answers, so the answer does not depend on the order a store lists them in.
pub(super) async fn find<S: Sessions + ?Sized>(
    store: &S,
    stored: &str,
) -> Result<Option<SessionSummary>, SessionError> {
    if let Some(summary) = store.summary_of(&SessionId(stored.to_string())).await? {
        return Ok(Some(summary));
    }
    // **No shape test on the name**: a run of the older kind wears a shorter name
    // than a drawn handle, and the name is only compared here, never used to
    // address a card.
    Ok(named_among(&store.all_summaries().await?, stored).cloned())
}

/// **The run `stored` names among summaries already read**, by [`find`]'s rule:
/// a card id first, then a public name. A reader that holds a bot's runs asks
/// them this, so the rule has one owner.
pub fn find_among<'a>(summaries: &'a [SessionSummary], stored: &str) -> Option<&'a SessionSummary> {
    summaries
        .iter()
        .find(|summary| summary.id.as_str() == stored)
        .or_else(|| named_among(summaries, stored))
}

/// The run wearing the public name `stored`; the one that began last when several do.
fn named_among<'a>(summaries: &'a [SessionSummary], stored: &str) -> Option<&'a SessionSummary> {
    summaries
        .iter()
        .filter(|summary| {
            summary
                .sid
                .as_ref()
                .is_some_and(|sid| sid.as_str() == stored)
        })
        .max_by_key(|summary| summary.started_at)
}
