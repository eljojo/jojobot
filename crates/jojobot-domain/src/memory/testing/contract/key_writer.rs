//! **A key's history names who made each write and in which run**, proven
//! against any [`Memory`] adapter.
//!
//! The run is kept per write of a claim, so the run of a key write is the run of
//! the claim write that set that value, not of the claim's first write.

use super::support::{capture, edit, ensure};
use super::*;
use crate::actor::Actor;
use crate::app::{AppId, AppStamp};
use std::collections::BTreeMap;

fn stamp() -> AppStamp {
    AppStamp {
        app: "springfield-ci".to_string(),
        grant: "record_deploy_event".to_string(),
        received_at: "2026-10-18T12:00:00Z".to_string(),
    }
}

fn staged(value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("stage".to_string(), value.to_string())])
}

/// **Each write of a key names the writer and the run of THE WRITE that set the
/// value.** A bot sets the key in run alpha, the same claim is edited in run
/// beta to set a new value, then an app sets it on a claim of its own. The
/// middle write is the one a join on the claim alone gets wrong: it would name
/// alpha, the run of the claim's first write.
pub async fn a_keys_history_names_the_writer_and_run_of_each_write<M: Memory>(store: &M) {
    let thing = EntityId("thing:contract-stage-edict".into());
    let bot = EntityId("bot:contract-key-writer".into());
    ensure(store, &thing).await;
    ensure(store, &bot).await;
    let claim = capture(
        store,
        NewFact {
            fields: staged("draft"),
            sets: ["stage".to_string()].into(),
            session: Some("run-alpha".to_string()),
            written_by_bot: Some(bot.clone()),
            ..NewFact::about(thing.clone(), "the stage of the edict", date(2026, 10, 1))
        },
    )
    .await;
    edit(
        store,
        &claim.address(),
        FactPatch {
            fields: staged("final"),
            sets: ["stage".to_string()].into(),
            session: Some("run-beta".to_string()),
            ..FactPatch::default()
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: BTreeMap::from([
                ("stage".to_string(), "deployed".to_string()),
                ("read_from".to_string(), "springfield-ci".to_string()),
            ]),
            sets: ["stage".to_string()].into(),
            app_stamp: Some(stamp()),
            provenance: Provenance::Observation,
            ..NewFact::about(thing.clone(), "the edict was deployed", date(2026, 10, 2))
        },
    )
    .await;

    let history = store.history(&thing, "stage").await.expect("history reads");
    let seen: Vec<(Option<&str>, Option<Actor>, Option<&str>)> = history
        .iter()
        .map(|w| (w.value.as_deref(), w.writer.clone(), w.run.as_deref()))
        .collect();
    let app = Actor::App(AppId::parse("springfield-ci").expect("a name"));
    assert_eq!(
        seen,
        vec![
            (
                Some("draft"),
                Some(Actor::Bot(bot.clone())),
                Some("run-alpha")
            ),
            // The bot of an edit is the bot behind its run; no run row answers
            // to this one, so it names a run and no writer.
            (Some("final"), None, Some("run-beta")),
            (Some("deployed"), Some(app), None),
        ],
        "each write names the run that set its value"
    );
    assert_eq!(
        history[0].recorded_by, None,
        "a bot write has no app receipt"
    );
    assert_eq!(
        history[2].recorded_by,
        Some(stamp()),
        "the app write keeps its receipt beside the writer"
    );
}
