//! **A skill fetched by a session that names its own bot.**
//!
//! A fetch starts no session, so a call that asks for a skill and a bot at once
//! would hand back a body and no handle. That reason does not hold when the call
//! carries the `sid` of a live run of that same bot: the caller is already booted
//! and has its handle. A booted session names its bot beside the skill as a
//! matter of course, so the door serves the skill then and refuses every other
//! pairing.

use super::*;

/// **The boot arguments a fetch would drop**, as the names a caller sent them
/// under. A fetch answers before the boot and touches no run, so none of these
/// can act beside it. `sid` and `bot` are not here: `sid` is read, and `bot`
/// has its own rule.
pub(crate) fn dropped_by_a_fetch(args: &OrientArgs, what: &str) -> Vec<&'static str> {
    [
        ("resume", args.resume.is_some()),
        ("claim", args.claim.is_some()),
        ("ask_for_code", args.ask_for_code.is_some()),
        ("challenge", args.challenge.is_some()),
        ("code", args.code.is_some()),
        ("offer_code", args.offer_code.is_some()),
        ("timezone", args.timezone.is_some()),
        ("today", args.today.is_some()),
        ("brief", args.brief.is_some()),
        ("request_id", args.request_id.is_some()),
        // A skill is answered first, so a section beside it is a second fetch.
        (
            "section",
            what == "a skill"
                && args
                    .section
                    .as_deref()
                    .is_some_and(|s| !s.trim().is_empty()),
        ),
    ]
    .into_iter()
    .filter_map(|(name, sent)| sent.then_some(name))
    .collect()
}

/// **The refusal for a fetch that carries boot arguments**, naming each one.
/// The whole call is refused, because serving the body would leave the caller
/// believing the rest had happened.
pub(crate) fn dropped_by_a_fetch_refusal(args: &OrientArgs, what: &str) -> Option<CallToolResult> {
    let dropped = dropped_by_a_fetch(args, what);
    if dropped.is_empty() {
        return None;
    }
    let named = dropped
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(misused(format!(
        "reading {what} touches no run, so honouring {named} here would drop it in silence. \
         Nothing was read and nothing was done. Call start_here with {named} and no fetch to \
         boot, or drop {named} to read."
    )))
}

/// **Why a fetch and a compaction report are two calls.** A fetch answers before
/// the boot and starts nothing, so a report sent beside it would be dropped
/// without a word, and the run would go on believing the server had forgotten
/// what it was taught and shown.
pub(crate) fn compaction_beside_a_fetch(what: &str) -> String {
    format!(
        "reporting a compaction and reading {what} are two calls: a fetch touches no run, so \
         honouring `compacted` here would drop it and forget nothing. Call start_here with \
         `compacted` and your `sid` to report the compaction, then again with the fetch alone — \
         or drop `compacted` to read now."
    )
}

/// **The refusal for a section asked for beside a bot**, live run or not.
pub(crate) fn section_beside_bot_refusal() -> CallToolResult {
    misused(
        "reading a section and booting an identity are two calls: a fetch starts no \
                     session, so honouring `bot` here would hand you a body and no handle. \
                     Unlike `skill`, `section` is not served beside a live run's `bot` and \
                     `sid`. Call start_here with `section` alone to read it, then again with \
                     `bot` to boot — or drop `section` to boot now."
            .to_string(),
    )
}

impl Jojobot {
    /// **The refusal for a skill asked for beside a bot, or `None` when the
    /// fetch may go on.** It goes on only when `sid` opens a live run of `bot`.
    /// The run is resolved by [`Jojobot::caller`], the one place a key becomes a
    /// run (rule 498). A key that stopped opening its run gets that key's own
    /// refusal, a run that stopped without wrapping is named, and the rest
    /// (no key, another bot's run) get the two-calls refusal. `carried` is
    /// [`Jojobot::standing`] of the same key.
    pub(crate) fn skill_beside_bot_refusal(
        &self,
        bot: &EntityId,
        sid: Option<&str>,
        carried: &serde_json::Value,
    ) -> Option<CallToolResult> {
        match self.caller(sid) {
            Err(refusal) => return Some(refusal),
            Ok(Some(caller)) if caller.bot == *bot => {
                if carried == "held" {
                    return None;
                }
                return Some(misused(format!(
                    "that run is {}, not live, so a skill is not served beside it. Resume it \
                     with start_here, `bot` and `resume` set to {}, then fetch.",
                    carried.as_str().unwrap_or("not live"),
                    caller.sid.as_str(),
                )));
            }
            Ok(_) => {}
        }
        Some(misused(
            "reading a skill and booting an identity are two calls: a fetch starts no \
             session, so honouring `bot` here would hand you a body and no handle. Call \
             start_here with `skill` alone to read the procedure, then again with `bot` \
             to boot — or drop `skill` to boot now. A booted session can send `skill`, \
             `bot` and its own `sid` together."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::{InMemorySessions, with_sessions};
    use jojobot_domain::session::Sessions;
    use std::sync::Arc;

    async fn fetch(jojobot: &Jojobot, bot: &str, sid: Option<String>) -> serde_json::Value {
        fetch_with(jojobot, Some(bot), sid, |_| {}).await
    }

    /// A skill fetch whose other arguments the case sets.
    async fn fetch_with(
        jojobot: &Jojobot,
        bot: Option<&str>,
        sid: Option<String>,
        set: impl FnOnce(&mut OrientArgs),
    ) -> serde_json::Value {
        let mut args = OrientArgs {
            request_id: None,
            compacted: None,
            claim: None,
            timezone: None,
            bot: bot.map(Into::into),
            brief: None,
            skill: Some("evidence".into()),
            section: None,
            resume: None,
            sid,
            today: None,
            ask_for_code: None,
            challenge: None,
            code: None,
            offer_code: None,
        };
        set(&mut args);
        json_of(
            &jojobot
                .start_here(Parameters(args))
                .await
                .expect("start_here ok"),
        )
    }

    fn assert_refused(body: &serde_json::Value) {
        assert_eq!(body["status"], "blocked", "{body}");
        assert!(body.get("skill").is_none(), "no body is served: {body}");
    }

    /// **The booted session gets the skill** when it names its own bot beside
    /// its own key, and the fetch begins nothing.
    #[tokio::test]
    async fn a_live_session_fetches_a_skill_beside_its_own_bot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let live = booted(&jojobot, "gamma").await;

        let fetched = fetch(&jojobot, "gamma", Some(live)).await;

        assert!(
            fetched["skill"]["body"]
                .as_str()
                .is_some_and(|body| !body.is_empty()),
            "{fetched}"
        );
        assert_eq!(fetched["carried_session"], "held", "{fetched}");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a fetch is a read: it begins nothing"
        );
    }

    /// A handler with two booted bots, after proving the positive the refusals
    /// rest on: the first bot's own key fetches its skill.
    async fn two_bots() -> (Jojobot, String, String) {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let gamma = booted(&jojobot, "gamma").await;
        let delta = booted(&jojobot, "delta").await;
        assert!(
            fetch(&jojobot, "gamma", Some(gamma.clone())).await["skill"]["body"].is_string(),
            "the positive the refusals rest on"
        );
        (jojobot, gamma, delta)
    }

    #[tokio::test]
    async fn a_skill_beside_a_bot_with_no_key_is_refused() {
        let (jojobot, _, _) = two_bots().await;
        assert_refused(&fetch(&jojobot, "gamma", None).await);
    }

    #[tokio::test]
    async fn a_skill_beside_a_bot_with_the_key_of_another_bots_run_is_refused() {
        let (jojobot, _, delta) = two_bots().await;
        assert_refused(&fetch(&jojobot, "gamma", Some(delta)).await);
    }

    #[tokio::test]
    async fn a_skill_beside_a_bot_with_the_key_of_a_wrapped_run_is_refused() {
        let (jojobot, _, _) = two_bots().await;
        let finished = booted(&jojobot, "gamma").await;
        jojobot
            .wrap_session(Parameters(crate::session::wrap_session::WrapSessionArgs {
                story: "done".into(),
                sid: finished.clone(),
            }))
            .await
            .expect("wrap ok");
        assert_refused(&fetch(&jojobot, "gamma", Some(finished)).await);
    }

    #[tokio::test]
    async fn a_skill_beside_a_bot_with_a_key_jojobot_never_held_is_refused() {
        let (jojobot, _, _) = two_bots().await;
        let unknown = format!("sk-{}", "a".repeat(26));
        assert_refused(&fetch(&jojobot, "gamma", Some(unknown)).await);
    }

    /// One boot argument per class, each set to a value that would act if the
    /// call booted.
    type Set = Box<dyn Fn(&mut OrientArgs)>;

    fn dropped_classes() -> Vec<(&'static str, Set)> {
        vec![
            ("claim", Box::new(|a| a.claim = Some("role-x".into()))),
            ("resume", Box::new(|a| a.resume = Some("new".into()))),
            ("ask_for_code", Box::new(|a| a.ask_for_code = Some(true))),
            ("challenge", Box::new(|a| a.challenge = Some("ch-x".into()))),
            ("code", Box::new(|a| a.code = Some("123456".into()))),
            (
                "offer_code",
                Box::new(|a| a.offer_code = Some("oc-x".into())),
            ),
            (
                "timezone",
                Box::new(|a| a.timezone = Some("Europe/Paris".into())),
            ),
            ("today", Box::new(|a| a.today = Some("2026-01-02".into()))),
            ("brief", Box::new(|a| a.brief = Some(true))),
            (
                "request_id",
                Box::new(|a| a.request_id = Some("req-x".into())),
            ),
            ("section", Box::new(|a| a.section = Some("opening".into()))),
        ]
    }

    /// **A call never has an argument silently ignored.** A skill beside a live
    /// run's own key serves the skill, and any other boot argument beside it
    /// refuses the whole call and names that argument.
    #[tokio::test]
    async fn a_boot_argument_beside_a_skill_and_a_live_run_is_refused_by_name() {
        let (jojobot, gamma, _) = two_bots().await;
        for (name, set) in dropped_classes() {
            let refused =
                fetch_with(&jojobot, Some("gamma"), Some(gamma.clone()), |a| set(a)).await;
            assert_refused(&refused);
            assert!(
                refused["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains(&format!("`{name}`"))),
                "{name} is named: {refused}"
            );
        }
    }

    /// The same rule with no bot: the argument is dropped just the same.
    #[tokio::test]
    async fn a_boot_argument_beside_a_skill_with_no_bot_is_refused_by_name() {
        let (jojobot, _, _) = two_bots().await;
        for (name, set) in dropped_classes() {
            if name == "resume" {
                continue; // refused earlier as a resume with no bot
            }
            let refused = fetch_with(&jojobot, None, None, |a| set(a)).await;
            assert_refused(&refused);
            assert!(
                refused["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains(&format!("`{name}`"))),
                "{name} is named: {refused}"
            );
        }
    }

    /// The same rule for a section, which also touches no run.
    #[tokio::test]
    async fn a_boot_argument_beside_a_section_is_refused_by_name() {
        let (jojobot, _, _) = two_bots().await;
        for (name, set) in dropped_classes() {
            if name == "resume" || name == "section" {
                continue;
            }
            let refused = fetch_with(&jojobot, None, None, |a| {
                a.skill = None;
                a.section = Some("opening".into());
                set(a)
            })
            .await;
            assert_refused(&refused);
            assert!(
                refused["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains(&format!("`{name}`"))),
                "{name} is named: {refused}"
            );
        }
    }

    /// **A key that stopped opening its run is told why**, as every other verb
    /// tells it, and not sent to "two calls".
    #[tokio::test]
    async fn a_skill_beside_a_bot_with_a_retired_key_gets_the_keys_own_refusal() {
        let (jojobot, _, _) = two_bots().await;
        let finished = booted(&jojobot, "gamma").await;
        jojobot
            .wrap_session(Parameters(crate::session::wrap_session::WrapSessionArgs {
                story: "done".into(),
                sid: finished.clone(),
            }))
            .await
            .expect("wrap ok");
        let own = match jojobot.caller(Some(&finished)) {
            Err(refusal) => json_of(&refusal),
            Ok(_) => panic!("a wrapped run's key opens nothing"),
        };
        let refused = fetch(&jojobot, "gamma", Some(finished)).await;
        assert_eq!(refused, own);
    }

    /// **An abandoned run is not a live run.** Its key still opens it, but the
    /// door that serves a skill beside a bot serves a live one only.
    #[tokio::test]
    async fn a_skill_beside_a_bot_with_the_key_of_an_abandoned_run_is_refused() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let live = booted(&jojobot, "gamma").await;
        crate::session::testing::journal_entry(&jojobot, &live, "a beat").await;
        assert!(
            fetch(&jojobot, "gamma", Some(live.clone())).await["skill"]["body"].is_string(),
            "the positive the refusal rests on"
        );
        let card = jojobot
            .registry
            .lookup_key(&live)
            .and_then(|held| held.card)
            .expect("the run has a card");
        store
            .close(&card, jojobot_domain::session::SessionState::Abandoned)
            .await
            .expect("close ok");
        assert_refused(&fetch(&jojobot, "gamma", Some(live)).await);
    }
}
