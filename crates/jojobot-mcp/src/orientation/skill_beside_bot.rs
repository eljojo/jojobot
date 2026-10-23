//! **A skill fetched by a session that names its own bot.**
//!
//! A fetch starts no session, so a call that asks for a skill and a bot at once
//! would hand back a body and no handle. That reason does not hold when the call
//! carries the `sid` of a live run of that same bot: the caller is already booted
//! and has its handle. A booted session names its bot beside the skill as a
//! matter of course, so the door serves the skill then and refuses every other
//! pairing.

use super::*;

impl Jojobot {
    /// **The refusal for a skill asked for beside a bot, or `None` when the
    /// fetch may go on.** It goes on only when `sid` opens a live run of `bot`.
    /// The run is resolved by [`Jojobot::caller`], the one place a key becomes a
    /// run (rule 498), so a key that opens nothing (no key, an unknown one, a
    /// wrapped run's) and a key that opens another bot's run all get the
    /// refusal.
    pub(crate) fn skill_beside_bot_refusal(
        &self,
        bot: &EntityId,
        sid: Option<&str>,
    ) -> Option<CallToolResult> {
        if matches!(self.caller(sid), Ok(Some(caller)) if caller.bot == *bot) {
            return None;
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
        json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some(bot.into()),
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
                }))
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
}
