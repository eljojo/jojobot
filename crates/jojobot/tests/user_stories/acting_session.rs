//! **A write that acts for a run names the run.**
//!
//! Every claim records the session that made it, and a claim's rewrite is judged
//! by the session of its first write. Some acts wrote their claim with no session
//! although the caller had one: a retraction, a fold, a restore, a role's claim,
//! renewal and release, and the taking-off of a stored due moment. Each case
//! below runs the act as a named run and reads who the store says made the write.

use std::collections::BTreeMap;

use jojobot_domain::memory::EntityId;
use serde_json::json;

use super::dsl::Story;

/// Every claim on `entity` with how many writes it has and the session of the
/// newest, read from the store beneath the surface.
async fn writes_on(story: &Story, entity: &str) -> BTreeMap<String, (usize, Option<String>)> {
    let memory = story.memory_store();
    let mut out = BTreeMap::new();
    // A role nobody has claimed is not an entity yet, and holds no claim.
    let facts = match memory.recall(&EntityId(entity.to_string())).await {
        Ok(facts) => facts,
        Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }) => Vec::new(),
        Err(e) => panic!("the claims of {entity} did not read: {e:?}"),
    };
    for fact in facts {
        let writes = memory
            .claim_history(&fact.address())
            .await
            .expect("the history reads");
        out.insert(
            fact.address().to_string(),
            (
                writes.len(),
                writes.last().and_then(|write| write.session.clone()),
            ),
        );
    }
    out
}

/// **The claims an act wrote to**: new, or with more writes than before. The
/// act has to have written something, or the case would pass over nothing.
fn written_since(
    before: &BTreeMap<String, (usize, Option<String>)>,
    after: &BTreeMap<String, (usize, Option<String>)>,
) -> Vec<(String, Option<String>)> {
    let moved: Vec<(String, Option<String>)> = after
        .iter()
        .filter(|(address, (count, _))| before.get(*address).is_none_or(|(was, _)| count > was))
        .map(|(address, (_, session))| (address.clone(), session.clone()))
        .collect();
    assert!(
        !moved.is_empty(),
        "the act wrote no claim: {before:?} {after:?}"
    );
    moved
}

/// Every claim the act wrote names `run`, and the act wrote at least `at_least`.
fn all_name(moved: &[(String, Option<String>)], run: &str, at_least: usize) {
    assert!(
        moved.len() >= at_least,
        "the act wrote {} claims, not {at_least}: {moved:?}",
        moved.len()
    );
    for (address, session) in moved {
        assert_eq!(
            session.as_deref(),
            Some(run),
            "{address} was written by the run and does not say so: {moved:?}"
        );
    }
}

/// **A retraction names the run that took the claim back**, on the claim and on
/// the account it leaves. The ordinary capture before it is the positive: it
/// names its run as it always did.
#[tokio::test]
async fn a_retraction_names_the_run_that_made_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:delta", "Delta").await;
    let gamma = story.as_bot("bot:gamma").await;
    let claim = gamma
        .call(
            "capture",
            json!({"subject": "thing:delta", "content": "the stall opens on Thursdays",
                   "provenance": "inference"}),
        )
        .await;
    let before = writes_on(&story, "thing:delta").await;
    all_name(&written_since(&BTreeMap::new(), &before), gamma.run(), 1);

    gamma
        .call(
            "retract",
            json!({"address": claim.field("address"), "reason": "written in error"}),
        )
        .await;
    let after = writes_on(&story, "thing:delta").await;
    all_name(&written_since(&before, &after), gamma.run(), 2);
    story.finish().await;
}

/// **A fold names the run that made it**, on the account the survivor keeps.
#[tokio::test]
async fn a_merge_names_the_run_that_made_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:epsilon", "Epsilon").await;
    s.add("thing:sigma", "Sigma").await;
    let gamma = story.as_bot("bot:gamma").await;
    let before = writes_on(&story, "thing:sigma").await;

    gamma
        .call(
            "merge_entities",
            json!({"duplicate": "thing:epsilon", "survivor": "thing:sigma",
                   "reason": "one thing under two names"}),
        )
        .await;
    let after = writes_on(&story, "thing:sigma").await;
    all_name(&written_since(&before, &after), gamma.run(), 1);
    story.finish().await;
}

/// **A restore names the run that put the thing back**, on the claim that
/// records both acts.
#[tokio::test]
async fn a_restore_names_the_run_that_made_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:omega", "Omega").await;
    let gamma = story.as_bot("bot:gamma").await;
    gamma
        .call(
            "archive_entity",
            json!({"handle": "thing:omega", "reason": "a mistaken filing"}),
        )
        .await;
    let before = writes_on(&story, "thing:omega").await;

    gamma
        .call(
            "archive_entity",
            json!({"handle": "thing:omega", "reason": "it was fine", "restore": true}),
        )
        .await;
    let after = writes_on(&story, "thing:omega").await;
    all_name(&written_since(&before, &after), gamma.run(), 1);
    story.finish().await;
}

/// **A role's claim and its release name the run that held it.**
#[tokio::test]
async fn a_role_claim_and_a_release_name_the_run() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let before = writes_on(&story, "role:gamma").await;
    let (_, held) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "gamma"}),
        )
        .await;
    let held = held.expect("the boot hands back the run");
    let claimed = writes_on(&story, "role:gamma").await;
    all_name(&written_since(&before, &claimed), held.run(), 1);

    // **The release at the wrap.** A renewal is not asked here: a lease is
    // renewed only once it is old enough to need it, and none is.
    held.call("wrap_session", json!({"story": "done for now"}))
        .await;
    let released = writes_on(&story, "role:gamma").await;
    all_name(&written_since(&claimed, &released), held.run(), 1);
    story.finish().await;
}

/// **Taking the stored due moment off a thing names the run that did.**
#[tokio::test]
async fn clearing_a_stored_due_moment_names_the_run_that_did() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:tau", "Tau").await;
    let gamma = story.as_bot("bot:gamma").await;
    let covered = gamma
        .call(
            "capture",
            json!({"subject": "thing:tau", "content": "the warranty runs out",
                   "provenance": "testimony", "sets": {"runs_out": "2026-06-01"}}),
        )
        .await;
    let before = writes_on(&story, "thing:tau").await;

    gamma
        .call(
            "update_fact",
            json!({"address": covered.field("address"), "clear_fields": ["runs_out"]}),
        )
        .await;
    let after = writes_on(&story, "thing:tau").await;
    all_name(&written_since(&before, &after), gamma.run(), 1);
    story.finish().await;
}
