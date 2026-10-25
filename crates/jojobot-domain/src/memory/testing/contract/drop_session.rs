//! **A thought dropped by a capture is archived by the session that captured**,
//! proven against any [`Memory`] adapter.
//!
//! The archive is one act with the capture, so the write that archives the
//! thought carries the capture's run and bot. A write with no run reads as
//! another session's to every session that holds the thought, and the session
//! that dropped it is told that someone else changed it.

use super::support::{capture, ensure, state};
use super::*;
use crate::memory::{Edge, EdgeShape, THOUGHT_CAPACITY};

/// **The archive a drop writes names the run and the bot of the capture that
/// named it.** Paired with the thought's first write, which names the run that
/// wrote the thought, so a store that stamped every write of the thought with
/// the dropping run would not pass.
pub async fn a_dropped_thought_is_archived_by_the_session_that_captured<M: Memory>(store: &M) {
    let bot = EntityId("bot:contract-drop-session".into());
    let object = EntityId("thing:contract-drop-session-object".into());
    ensure(store, &bot).await;
    ensure(store, &object).await;
    capture(
        store,
        state(NewFact {
            fields: [(THOUGHT_CAPACITY.to_string(), "2".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(bot.clone(), "capacity is two", date(2026, 10, 8))
        }),
    )
    .await;
    let kept = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            session: Some("session-wrote".to_string()),
            written_by_bot: Some(bot.clone()),
            ..NewFact::about(bot.clone(), "the first thought", date(2026, 10, 8))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            ..NewFact::about(bot.clone(), "the second thought", date(2026, 10, 8))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            drop: Some(kept.address()),
            drop_because: Some("the third thought replaces it".into()),
            session: Some("session-dropped".to_string()),
            written_by_bot: Some(bot.clone()),
            ..NewFact::about(bot.clone(), "the third thought", date(2026, 10, 9))
        },
    )
    .await;

    let history = store
        .claim_history(&kept.address())
        .await
        .expect("the history reads");
    let sessions: Vec<Option<&str>> = history.iter().map(|w| w.session.as_deref()).collect();
    assert_eq!(
        sessions,
        vec![Some("session-wrote"), Some("session-dropped")],
        "the archive names the run that dropped the thought: {history:?}"
    );
    let bots: Vec<Option<&EntityId>> = history.iter().map(|w| w.bot.as_ref()).collect();
    assert_eq!(
        bots,
        vec![Some(&bot), Some(&bot)],
        "the archive names the bot that dropped the thought"
    );
}
