//! `list_runs` — See your own bot's past runs, in every state — a read that
//! begins, closes and sweeps none of them.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;
use crate::answer::STATUS_BAR_ROOM;
use jojobot_domain::text::Capped;

/// How many runs come back when the caller does not say — the same twenty
/// `search` and `list_sent` answer with, and for the same reason: an answer
/// nobody sized is an answer that grows until it is unreadable.
const DEFAULT_LIMIT: usize = 20;

/// Arguments to `list_runs`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListRunsArgs {
    /// How many runs to return, newest first. Defaults to twenty.
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub(crate) limit: Option<u32>,
    /// **How many of the newest runs you have already read.** The answer names
    /// the `offset` that returns the next part whenever it left runs out;
    /// repeat the same call with it. Defaults to 0. An offset past the last run
    /// answers no runs and a `past_the_end` note naming it, so an empty page is
    /// never mistaken for a bot that has no runs.
    #[serde(default)]
    pub(crate) offset: Option<u32>,
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
                       different identity's runs. \
                       It is a plain read: nothing here is begun, closed or swept. A bot with no \
                       runs comes back with an empty list. A run counted since the counts began \
                       carries `calls` and `refused`; recall of one run adds \
                       `calls_<verb>`, `refused_<verb>` and `refused_<verb>_<word>`. Each run \
                       carries its name (`run`, the name start_here takes to resume the run once it has gone quiet; a wrapped run reopens only through its wrap_code — \
                       never its key, which a list does not show), \
                       what it was working on last, its state (active · wrapped · abandoned), \
                       when it began, when it last had anything to show for itself, and how many \
                       beats its chronology holds — never the chronology itself. For a run's chronology \
                       without resuming it, read recall with kind: session and prose: true. Read a live or abandoned run's \
                       beats by resuming it through start_here; the most recently wrapped run's \
                       closing story arrives there too, as the handover. Each run also carries `served_since_compaction` and `served_lifetime`, which \
                       replace `served` (the characters answered since its last compaction, and \
                       over its whole life, each with a token estimate, never a real \
                       tokenisation), and `compactions`, how many it reported. A compaction \
                       resets served_since_compaction; the weekly totals are kind usage (view: \
                       usage). An unreadable run carries `unreadable`, `fix_by: person` and no moments. Newest first. Twenty newest by default: \
                       `limit` is the size of a page. An answer stops at the answer ceiling, never \
                       mid-run, and whatever it leaves out is counted under not_shown with the \
                       `offset` that reads the next part: repeat the same call with it. A call \
                       with no `sid` at all comes back status: blocked — \
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
        // **A page of no runs is refused**, as `search` and `list_sent` refuse
        // it. It cut every run and named the offset it was given as the one
        // that reads the rest, so following that pointer repeated the same call.
        if args.limit == Some(0) {
            return Ok(crate::caller::handle_declined(
                "",
                "limit must be at least 1: it is the size of a page. Leave it off for the \
                 twenty newest, or send a larger limit."
                    .to_string(),
            ));
        }
        // **The one read on this port that never begins, closes or sweeps.**
        // `summaries_of` is what this verb actually needs: which runs, in
        // what state, and two numbers about their beats — never the
        // sweep's own walk, which reads full sessions because it has to
        // decide whether one has gone quiet, and never `session_for`, which
        // materializes a run lazily on its first write. A caller asking
        // what it has run is not the caller starting one, and is not the
        // caller paying for the text of every beat either.
        let runs = self
            .sessions
            .summaries_of(&caller.bot)
            .await
            .map_err(session_error)?;
        // Already newest-start-first off the port; the cut below keeps that
        // order rather than re-deriving it. `limit` is the size of a page and
        // `offset` is how many runs the caller has already read, so a page is
        // a window of the ordered list and the ceiling can make it shorter.
        let held = runs.len();
        let limit = args.limit.map_or(DEFAULT_LIMIT, |l| l as usize);
        let offset = args.offset.map_or(0, |o| o as usize);
        let page: Vec<_> = runs.into_iter().skip(offset).take(limit).collect();

        // **What each run on the page called and was refused**, in one read of the
        // runs on it.
        let names: Vec<String> = page
            .iter()
            .map(|session| session.run_name().to_string())
            .collect();
        let calls = self
            .sessions
            .run_calls(Some(&names))
            .await
            .map_err(session_error)?;

        // **Rendered first, so the page can be filled against the ceiling.**
        let rendered: Vec<(serde_json::Value, usize)> = page
            .iter()
            .map(|session| {
                if let Some(json) = named_unreadable(session) {
                    let size = json.to_string().chars().count() + 1;
                    return (json, size);
                }
                let mut json = serde_json::json!({
                    // **The run's public name, read off the record — never
                    // minted, and never a key.** A key is shown once, to the
                    // run it opens. A card written before names were kept goes
                    // by its own id.
                    "run": session.run_name(),
                    "working_on": session.focus,
                    "state": session.state.as_token(),
                    "started_at": session.started_at.to_string(),
                    "last_beat": session.last_beat.to_string(),
                    "entry_count": session.entry_count,
                    "served_since_compaction": served_json(session.served_chars),
                    "served_lifetime": served_json(session.served_lifetime()),
                    "compactions": session.compactions,
                });
                // **A run with no count kept has no totals, and says nothing**
                // rather than saying none: the counts began after the first runs.
                let verbs = jojobot_domain::usage::run_verbs(&calls, session.run_name());
                if !verbs.is_empty() {
                    let (made, refused) = jojobot_domain::usage::call_totals(&verbs);
                    json["calls"] = made.into();
                    json["refused"] = refused.into();
                }
                let size = json.to_string().chars().count() + 1;
                (json, size)
            })
            .collect();

        // The envelope with its run list empty, and the block that names what
        // was left out at its widest, so what ships is under the ceiling.
        let envelope = |count: usize| {
            serde_json::json!({
                "bot": caller.bot.as_str(),
                "count": count,
                "runs_total": held,
                "how_to_read": "this is which runs, in what state, and when — not their \
                                chronology, which grows without bound. Resume a live or \
                                abandoned run through start_here to read its beats; the most \
                                recently wrapped run's closing story arrives there too, as the \
                                handover.",
                "runs": [],
            })
        };
        // **The block is counted under its key**, as it rides the answer: the
        // key is characters the answer spends too.
        let rest = envelope(page.len()).to_string().chars().count()
            + crate::answer::under_key("not_shown", &crate::answer::not_shown(held, held, "runs"))
            + STATUS_BAR_ROOM;
        let kept = Capped::beside(rest).head(&rendered, |(_, size)| *size);
        let mut answer = envelope(kept.kept().len());
        answer["runs"] = kept
            .kept()
            .iter()
            .map(|(json, _)| json.clone())
            .collect::<Vec<_>>()
            .into();
        // **Eliding is never silent.** This says how many runs there are after
        // the ones above and which call reads them; it is absent when nothing
        // was cut rather than saying "0 left out".
        if held > offset + kept.kept().len() {
            answer["not_shown"] = crate::answer::not_shown(
                held - offset - kept.kept().len(),
                offset + kept.kept().len(),
                "runs",
            );
        }
        // **An empty page past the last run says so**, as `list_sent`, `search`,
        // `recall` and `list_entities` do: an empty page with no note reads as
        // "this bot has no runs".
        if page.is_empty() && offset > 0 && offset >= held {
            answer["past_the_end"] = format!(
                "offset {offset} is past the last of the {held} runs this read lists, so \
                 nothing is left to read"
            )
            .into();
        }
        json_result(&answer)
    }
}

/// **A run whose row cannot be read, named with why and with no moment**: its
/// stored times are not to be trusted (rule 130). Nothing for a run that read.
/// **It carries `fix_by: person`**, because a row that cannot be read is stored
/// damage and no call a session makes repairs it.
fn named_unreadable(
    session: &jojobot_domain::session::SessionSummary,
) -> Option<serde_json::Value> {
    session.unreadable.as_ref().map(|why| {
        serde_json::json!({
            "run": session.run_name(),
            "working_on": session.focus,
            "unreadable": why,
            "fix_by": crate::answer::FixBy::Person.as_token(),
        })
    })
}

#[cfg(test)]
mod unreadable_tests {
    use super::*;
    use crate::session::testing::*;

    /// **A run that could not be read is named with its reason and shows no
    /// moment; a run that read shows nothing of the kind.**
    #[tokio::test]
    async fn an_unreadable_run_is_named_and_a_sound_one_is_not() {
        let store = InMemorySessions::new();
        let run = store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: fixture_sid(line!()),
                focus: "shelving".into(),
                started_at: jiff::Timestamp::now(),
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        let mut summary = jojobot_domain::session::SessionSummary::of(&run, &[]);
        assert!(
            named_unreadable(&summary).is_none(),
            "a sound run is not named"
        );
        summary.unreadable = Some("its start time is no time".into());
        let named = named_unreadable(&summary).expect("named");
        assert_eq!(named["unreadable"], "its start time is no time", "{named}");
        assert_eq!(named["working_on"], "shelving", "{named}");
        assert_eq!(
            named["fix_by"],
            crate::answer::FixBy::Person.as_token(),
            "{named}"
        );
        assert!(
            named.get("started_at").is_none(),
            "no moment is shown: {named}"
        );

        // **The recall of a run says it too**, and shows no moment.
        let doc = jojobot_domain::session::projected_summary(&summary);
        assert_eq!(
            doc.fields.get("unreadable").map(String::as_str),
            Some("its start time is no time"),
            "{doc:?}"
        );
        assert!(!doc.fields.contains_key("started_at"), "{doc:?}");
        assert!(
            jojobot_domain::session::projected_summary(
                &jojobot_domain::session::SessionSummary::of(&run, &[])
            )
            .fields
            .contains_key("started_at"),
            "a sound run shows its start"
        );

        // The moments the runs began leave the unreadable one out: it has none to
        // count.
        let sound = jojobot_domain::session::SessionSummary::of(&run, &[]);
        assert_eq!(
            jojobot_domain::session::history::started_ats(&[summary, sound.clone()]),
            vec![sound.started_at]
        );
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;

    /// **`list_runs` names a run its store could not read, beside the runs it can,
    /// through the verb a caller uses.** The unreadable run shows its reason and no
    /// moment; the sound run beside it shows its moments; the count includes both.
    #[tokio::test]
    async fn list_runs_names_an_unreadable_run_beside_the_runs_that_read() {
        let counting = Arc::new(CountingSessions::new(InMemorySessions::new()));
        let (jojobot, _memory) = with_sessions_port_and_memory(counting.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let summary =
            |id: &str, unreadable: Option<&str>| jojobot_domain::session::SessionSummary {
                id: jojobot_domain::session::SessionId(id.into()),
                sid: None,
                bot: EntityId("bot:gamma".into()),
                focus: "shelving".into(),
                started_at: match unreadable {
                    Some(_) => jiff::Timestamp::UNIX_EPOCH,
                    None => jiff::Timestamp::now(),
                },
                state: jojobot_domain::session::SessionState::Abandoned,
                entry_count: 0,
                last_beat: jiff::Timestamp::UNIX_EPOCH,
                served_chars: 0,
                started_on: None,
                compactions: 0,
                served_set_back: 0,
                unreadable: unreadable.map(str::to_string),
            };
        counting.name_an_unreadable_run(summary("damaged-run", Some("its start time is no time")));
        counting.name_an_unreadable_run(summary("sound-run", None));

        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    offset: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        let runs = body["runs"].as_array().expect("a list of runs");
        assert_eq!(runs.len(), 2, "{body}");
        let damaged = runs
            .iter()
            .find(|run| run["run"] == "damaged-run")
            .unwrap_or_else(|| panic!("the damaged run is named: {body}"));
        assert_eq!(damaged["unreadable"], "its start time is no time", "{body}");
        assert!(damaged.get("started_at").is_none(), "{body}");
        let sound = runs
            .iter()
            .find(|run| run["run"] != "damaged-run")
            .expect("the sound run");
        assert!(sound.get("unreadable").is_none(), "{body}");
        assert!(sound["started_at"].is_string(), "{body}");
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
                id: None,
                key_hash: None,
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
                    offset: None,
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
                    offset: None,
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
                    offset: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["count"], 1, "{body}");
        assert_eq!(
            body["runs"][0]["run"],
            legacy.id.as_str(),
            "a card with no stored name goes by its own id: {body}"
        );
        assert!(
            jojobot.registry.name_of_card(&legacy.id).is_none(),
            "list_runs must not have registered a name for this card"
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
                    offset: None,
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
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        let sid = as_run(&jojobot, "gamma", &stale.id);
        let body = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    offset: None,
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
                    offset: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(body["count"], 1, "only gamma's one run: {body}");
        assert_eq!(body["runs"][0]["working_on"], "my own work");
    }

    /// **A page of no runs is refused**, as `search` and `list_sent` refuse it.
    /// It cut every run and named the offset it was given as the one that reads
    /// the rest, so following that pointer repeated the same call. A page of one
    /// run beside it still reads, so the refusal is about the size and not about
    /// the verb.
    #[tokio::test]
    async fn a_limit_of_no_runs_is_refused_and_a_limit_of_one_reads_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        for n in 0..3u32 {
            run(&store, "gamma", n, &format!("run number {n}")).await;
        }
        let sid = as_bot(&jojobot, "gamma");

        let refused = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: Some(0),
                    offset: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("list_runs answers"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused.get("runs").is_none() && refused.get("not_shown").is_none(),
            "a refused page carries no runs and no pointer to follow: {refused}"
        );

        let one = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: Some(1),
                    offset: None,
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(one["count"], 1, "{one}");
        assert_eq!(one["not_shown"]["offset"], 1, "{one}");
    }

    /// **An empty page past the last run says so**, as `list_sent`, `search`,
    /// `recall` and `list_entities` do: an empty page with no note reads as "this
    /// bot has no runs". Paired in one case: an offset inside the list reads its
    /// last runs and carries no note, and a bot with no runs at offset 0 answers
    /// its ordinary empty answer.
    #[tokio::test]
    async fn an_offset_past_the_last_run_says_it_is_past_the_end() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        for n in 0..12u32 {
            run(&store, "gamma", n, &format!("run number {n}")).await;
        }
        let list = |bot: &'static str, offset: u32| {
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .list_runs(Parameters(ListRunsArgs {
                            limit: None,
                            offset: Some(offset),
                            sid: Some(as_bot(jojobot, bot)),
                        }))
                        .await
                        .expect("list_runs ok"),
                )
            }
        };

        let past = list("gamma", 40).await;
        assert_eq!(past["count"], 0, "{past}");
        let note = past["past_the_end"]
            .as_str()
            .unwrap_or_else(|| panic!("an empty page past the last run says so: {past}"));
        assert!(
            note.contains("40") && note.contains("12"),
            "the note names the offset and the number of runs: {note}"
        );

        let at_end = list("gamma", 12).await;
        assert!(
            at_end["past_the_end"].is_string(),
            "an offset equal to the number of runs is past the last one: {at_end}"
        );

        let inside = list("gamma", 10).await;
        assert_eq!(inside["count"], 2, "the last two runs: {inside}");
        assert!(inside.get("past_the_end").is_none(), "{inside}");

        let none = list("delta", 0).await;
        assert_eq!(none["count"], 0, "{none}");
        assert!(
            none.get("past_the_end").is_none(),
            "a bot with no runs has nothing to be past: {none}"
        );
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
                    offset: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("list_runs ok"),
        );
        assert_eq!(cut["count"], 2, "{cut}");
        assert_eq!(cut["runs_total"], 4);
        assert_eq!(cut["not_shown"]["count"], 2);
        assert_eq!(
            cut["not_shown"]["offset"], 2,
            "the part that reads the runs left out starts after the two shown: {cut}"
        );

        let whole = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: None,
                    offset: None,
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

    /// The size of an answer as the surface serves it: the body, one newline
    /// and the status bar's room, counted in characters as the ceiling is.
    fn served_size(body: &serde_json::Value) -> usize {
        body.to_string().chars().count() + 1 + crate::answer::STATUS_BAR_ROOM
    }

    /// **A bot with more runs than one answer can hold is read in parts.** Every
    /// part is under the ceiling, the offset each names reads the next, and the
    /// parts together are exactly the runs that were begun, newest first, none
    /// twice and none skipped.
    #[tokio::test]
    async fn a_long_history_is_read_in_parts_under_the_ceiling_with_none_twice_and_none_skipped() {
        const RUNS: u32 = 150;
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        for n in 0..RUNS {
            // A focus may hold 200 characters; this one is 176.
            let focus = format!(
                "run {n:03} {}",
                "weighing the kiln ledger against the flue ".repeat(4)
            );
            run(&store, "gamma", n, &focus).await;
        }
        let sid = as_bot(&jojobot, "gamma");

        let mut seen: Vec<String> = Vec::new();
        let mut offset = 0u32;
        let mut parts = 0;
        loop {
            let part = json_of(
                &jojobot
                    .list_runs(Parameters(ListRunsArgs {
                        limit: Some(1000),
                        offset: Some(offset),
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("list_runs ok"),
            );
            parts += 1;
            assert!(
                served_size(&part) <= jojobot_domain::text::ANSWER_CEILING,
                "part {parts} passes the ceiling at {}",
                served_size(&part)
            );
            assert_eq!(part["runs_total"], RUNS, "{}", part["not_shown"]);
            let runs = part["runs"].as_array().expect("a list");
            assert!(
                !runs.is_empty(),
                "a part with no run cannot be walked on from"
            );
            seen.extend(
                runs.iter()
                    .map(|r| r["working_on"].as_str().expect("a focus")[..7].to_string()),
            );
            match part["not_shown"]["offset"].as_u64() {
                Some(next) => {
                    assert_eq!(
                        next as usize,
                        seen.len(),
                        "the offset named is the count of runs already read"
                    );
                    offset = next as u32;
                }
                None => break,
            }
        }
        assert!(
            parts >= 3,
            "a hundred and fifty long runs take three parts at least: {parts}"
        );
        let expected: Vec<String> = (0..RUNS).rev().map(|n| format!("run {n:03}")).collect();
        assert_eq!(seen, expected, "every run once, newest first");
    }

    /// An `offset` skips the runs already read even when nothing needs the
    /// ceiling, so a caller paging by `limit` reads the whole history.
    #[tokio::test]
    async fn an_offset_skips_the_runs_already_read() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        for n in 0..5u32 {
            run(&store, "gamma", n, &format!("run number {n}")).await;
        }
        let sid = as_bot(&jojobot, "gamma");
        let page = json_of(
            &jojobot
                .list_runs(Parameters(ListRunsArgs {
                    limit: Some(2),
                    offset: Some(2),
                    sid: Some(sid),
                }))
                .await
                .expect("list_runs ok"),
        );
        let focus: Vec<&str> = page["runs"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|r| r["working_on"].as_str().expect("a focus"))
            .collect();
        assert_eq!(focus, vec!["run number 2", "run number 1"], "{page}");
        assert_eq!(page["not_shown"]["offset"], 4, "{page}");
        assert_eq!(page["not_shown"]["count"], 1, "{page}");
    }

    /// A caller learns the argument exists from the tool's own description.
    #[test]
    fn list_runs_description_names_offset() {
        let tools = Jojobot::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|t| t.name == "list_runs")
            .expect("list_runs is a tool");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("offset"), "{description}");
    }

    /// **The served figure has no path but reading the diff, and that is no
    /// path at all.** `list_runs` renders `served_since_compaction`,
    /// `served_lifetime` and `compactions` on every run (see the map above), but
    /// a caller who has only ever read the tool's own description has no way to
    /// learn the fields exist or that the token counts are estimates rather than
    /// a real tokenisation.
    #[test]
    fn list_runs_description_names_the_served_figure_and_its_estimate() {
        let tools = Jojobot::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|t| t.name == "list_runs")
            .expect("list_runs is a tool");
        let description = tool.description.as_deref().unwrap_or_default();
        for name in ["served_since_compaction", "served_lifetime", "compactions"] {
            assert!(
                description.contains(name),
                "must name {name}, which each run carries: {description}"
            );
        }
        assert!(
            description.contains("estimate"),
            "must say the token figure is an estimate derived from characters: {description}"
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
                    offset: None,
                    sid: None,
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(how.contains("start_here"), "{how}");
    }
}
