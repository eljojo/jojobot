//! `list_runs` — See your own bot's past runs, in every state — a read that
//! begins, closes and sweeps none of them.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// How many runs come back when the caller does not say — the same twenty
/// `search` and `list_sent` answer with, and for the same reason: an answer
/// nobody sized is an answer that grows until it is unreadable.
const DEFAULT_LIMIT: usize = 20;

/// Arguments to `list_runs`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListRunsArgs {
    /// How many runs to return, newest first. Defaults to twenty.
    #[serde(default)]
    pub(crate) limit: Option<u32>,
    /// **Your session id**, exactly as the boot door returned it. It is what
    /// tells jojobot which bot is asking — there is no other way to name
    /// whose runs this reads, and no way to name a different one.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// See your own bot's past runs, in every state — a read that begins,
/// closes and sweeps none of them.
#[tool_router(router = list_runs_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "See YOUR OWN bot's past runs — every one this identity has had, in \
                       whatever state it reached, what each was working on, and when. \
                       WHOSE RUNS IS NOT AN ARGUMENT: the `sid` you pass says which bot is \
                       asking, exactly as it does for journal, and there is no way to name a \
                       different identity's runs — this reads what is yours and nothing else. \
                       It is a plain read: nothing here is begun, closed or swept. A bot with no \
                       runs at all comes back with an empty list rather than a blocked answer — \
                       that is not a fault, it is a bot that has never journalled. Each run \
                       carries its own handle (`sid`, addressable through start_here's resume — \
                       `null` only for a run that predates stored handles, since a plain read mints \
                       none), \
                       what it was working on last, its state (active · wrapped · abandoned), \
                       when it began, when it last had anything to show for itself, and how many \
                       beats its chronology holds — never the chronology itself, which grows \
                       without bound and is not what this answers. Read a live or abandoned run's \
                       beats by resuming it through start_here; the most recently wrapped run's \
                       closing story arrives there too, as the handover, on every boot of this \
                       identity. Newest first. Twenty newest by default: raise `limit` for more, \
                       and whatever a cut leaves out is counted under not_shown rather than \
                       silently dropped. A call with no `sid` at all comes back status: blocked — \
                       jojobot will not guess whose runs to read."
    )]
    pub(crate) async fn list_runs(
        &self,
        Parameters(args): Parameters<ListRunsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.identified(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **The one read on this port that never begins, closes or sweeps.**
        // `sessions_of` is exactly what the sweep itself walks — never
        // `sweep_and_find`, which would close a stale run, and never
        // `session_for`, which materializes a run lazily on its first write.
        // A caller asking what it has run is not the caller starting one.
        let mut runs = self
            .sessions
            .sessions_of(&caller.bot)
            .await
            .map_err(session_error)?;
        // Already newest-start-first off the port; the cut below keeps that
        // order rather than re-deriving it.
        let held = runs.len();
        let limit = args.limit.map_or(DEFAULT_LIMIT, |l| l as usize);
        runs.truncate(limit);

        let rendered: Vec<serde_json::Value> = runs
            .iter()
            .map(|session| {
                serde_json::json!({
                    // **The handle the card was born with, read off the
                    // record — never minted.** A registry mint is a write,
                    // and this call promises to begin, close and sweep none
                    // of a bot's runs. A card written before handles were
                    // persisted carries none, and comes back `null` rather
                    // than minted on the spot: honest about what this run
                    // cannot yet be addressed by, rather than a side effect
                    // a plain read must not have.
                    "sid": session.sid.as_ref().map(|s| s.as_str()),
                    "working_on": session.focus,
                    "state": session.state.as_token(),
                    "started_at": session.started_at.to_string(),
                    "last_beat": session.last_beat().to_string(),
                    "entry_count": session.entries.len(),
                })
            })
            .collect();

        json_result(&serde_json::json!({
            "bot": caller.bot.as_str(),
            "count": rendered.len(),
            "runs_total": held,
            // **Eliding is never silent.** Absent when nothing was cut, rather
            // than present saying zero — the same convention `list_sent` uses.
            "not_shown": (held > rendered.len()).then(|| serde_json::json!({
                "count": held - rendered.len(),
                "how_to_proceed": "these are the newest; raise limit for more",
            })),
            "how_to_read": "this is which runs, in what state, and when — not their \
                            chronology, which grows without bound. Resume a live or abandoned \
                            run through start_here to read its beats; the most recently \
                            wrapped run's closing story arrives there too, as the handover.",
            "runs": rendered,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// A run of `bot`, begun directly through the port so the fixture can put
    /// it in whatever state and age the case needs without paying for a boot.
    async fn run(store: &InMemorySessions, bot: &str, nth: u32, focus: &str) -> Session {
        store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId(format!("bot:{bot}")),
                sid: fixture_sid(line!() + nth),
                focus: focus.into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok")
    }

    /// **The paired case.** A bot with a run in every state comes back with
    /// all of them, correctly stated per-state, in the same batch as a bot
    /// that has never run coming back saying so — never two unrelated tests,
    /// because the failure this guards against is a build that gets one half
    /// right and never notices the other.
    #[tokio::test]
    async fn every_state_comes_back_for_a_bot_that_ran_and_nothing_for_one_that_never_did() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let active = run(&store, "gamma", 1, "chasing the flaky test").await;
        let wrapped = run(&store, "gamma", 2, "a finished piece of work").await;
        store
            .close(&wrapped.id, SessionState::Wrapped)
            .await
            .expect("close ok");
        abandoned_run(&store, "gamma", "reading the hand-off", 30).await;

        let sid = as_run(&jojobot, "gamma", &active.id);
        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["bot"], "bot:gamma");
        assert_eq!(body["count"], 3, "all three of gamma's runs: {body}");
        let by_focus: std::collections::HashMap<&str, &str> = body["runs"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|r| {
                (
                    r["working_on"].as_str().expect("a focus"),
                    r["state"].as_str().expect("a state"),
                )
            })
            .collect();
        assert_eq!(
            by_focus,
            std::collections::HashMap::from([
                ("chasing the flaky test", "active"),
                ("a finished piece of work", "wrapped"),
                ("reading the hand-off", "abandoned"),
            ]),
            "every state, told apart, not just counted: {body}"
        );

        // **The other half, in the same batch.** A bot that has never run
        // comes back reporting so — not a blocked answer, not an error.
        let never_ran = as_bot(&jojobot, "delta");
        let empty = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(never_ran),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(empty["bot"], "bot:delta");
        assert_eq!(empty["count"], 0, "delta has never run: {empty}");
        assert_eq!(
            empty["runs"].as_array().expect("a list").len(),
            0,
            "{empty}"
        );
    }

    /// **A plain read never mints.** A card with no stored handle — the shape
    /// of one written before handles were persisted — is answered `sid: null`
    /// rather than jojobot minting one on the spot: `list_runs` promises to
    /// begin, close and sweep none of a bot's runs, and a registry mint taken
    /// under no gate is exactly the race the boot's own mint (`attach.rs`)
    /// takes a gate to avoid.
    #[tokio::test]
    async fn list_runs_answers_a_handleless_card_with_sid_null_rather_than_minting_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let legacy = run(&store, "gamma", 1, "from before handles were stored").await;
        store.forget_sid(&legacy.id);

        let sid = as_bot(&jojobot, "gamma");
        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["count"], 1, "{body}");
        assert!(
            body["runs"][0]["sid"].is_null(),
            "a card with no stored handle must not be minted one by a plain read: {body}"
        );
        assert!(
            jojobot.registry.addressing(&legacy.id).is_none(),
            "list_runs must not have minted a handle for this card"
        );
    }

    /// **A read that begins nothing.** A bot with no run at all is answered
    /// with an empty list, not an error — and the board still holds nothing
    /// to resume afterward, which is what proves this did not materialize one.
    #[tokio::test]
    async fn list_runs_begins_no_session_for_a_bot_that_has_never_run() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["count"], 0, "{body}");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "reading what has run must not be the thing that starts one"
        );
    }

    /// **A read that sweeps nothing.** A run stale enough for the sweep to
    /// close it stays exactly as it was — `active`, on the record and in the
    /// answer — because reading a bot's runs is not booting it.
    #[tokio::test]
    async fn list_runs_does_not_sweep_a_stale_active_run() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        // Past ABANDONED_AFTER, and never closed — what `start_here`'s own
        // sweep would mark abandoned on the next boot.
        let stale = store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t900".into()),
                focus: "a run nobody has touched in days".into(),
                started_at: jiff::Timestamp::now()
                    - jojobot_domain::session::ABANDONED_AFTER
                    - jiff::SignedDuration::from_hours(1),
            })
            .await
            .expect("begin ok");

        let sid = as_run(&jojobot, "gamma", &stale.id);
        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(
            body["runs"][0]["state"], "active",
            "the answer must not report a sweep this call never ran: {body}"
        );
        assert_eq!(
            store.read_session(&stale.id).await.expect("read ok").state,
            SessionState::Active,
            "…and the record itself must be untouched"
        );
    }

    /// **Own runs only.** The bot is derived from the `sid`'s own binding —
    /// there is no argument that names one, so a caller holding a different
    /// identity's runs cannot reach them by asking.
    #[tokio::test]
    async fn list_runs_never_reaches_another_identitys_runs() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let mine = run(&store, "gamma", 1, "my own work").await;
        run(&store, "delta", 2, "not mine to see").await;
        run(&store, "delta", 3, "nor this one").await;

        let sid = as_run(&jojobot, "gamma", &mine.id);
        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["count"], 1, "only gamma's one run: {body}");
        assert_eq!(body["runs"][0]["working_on"], "my own work");
    }

    /// A limit cuts the list and names what it left out, exactly as
    /// `list_sent`'s does — and a list that fits carries no marker at all.
    #[tokio::test]
    async fn a_limit_cuts_the_list_and_names_what_it_left_out() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        for n in 0..4u32 {
            run(&store, "gamma", n, &format!("run number {n}")).await;
        }
        let sid = as_bot(&jojobot, "gamma");

        let cut = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: Some(2),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(cut["count"], 2, "{cut}");
        assert_eq!(cut["runs_total"], 4);
        assert_eq!(cut["not_shown"]["count"], 2);
        assert!(
            cut["not_shown"]["how_to_proceed"]
                .as_str()
                .expect("a way on")
                .contains("limit")
        );

        let whole = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(whole["count"], 4);
        assert!(
            whole["not_shown"].is_null(),
            "nothing was cut, so nothing says it was: {whole}"
        );
    }

    /// A call with no `sid` at all is blocked with the way to get one — the
    /// same refusal every other session verb gives a connection that never
    /// booted.
    #[tokio::test]
    async fn list_runs_without_a_sid_is_blocked_with_the_way_forward() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = blocked(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    sid: None,
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(how.contains("start_here"), "{how}");
    }
}
