//! **What is in flight on a bot's board, said once.** The door lists a bot's
//! runs when it has some, and a refusal for a key that stopped opening its run
//! lists the same ones. Both build an entry here, so a run is in flight, and
//! can be picked up or not, by one definition.
//!
//! A refusal reads the board and writes nothing. It never resumes a run, and a
//! run it offers is one the door would let a name pick up.

use jiff::Timestamp;

use super::*;

/// **One run, as the door offers it.** The public name and never a key: a key
/// is shown once, to the run it opens.
pub(crate) fn choice_json(session: &Session) -> serde_json::Value {
    let name = session
        .sid
        .as_ref()
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| session.id.as_str().to_string());
    let mut choice = serde_json::json!({
        "run": name,
        // **What it was working on is the whole point of the offer.** A bot may
        // have several runs at once, and a list of opaque handles is not a
        // choice anybody can make.
        "working_on": session.focus,
        // **Marked, never silently mixed in.** Not because a stop is worse — it
        // is not a failure — but because "this one was never wrapped up" is what
        // tells the caller which of these is still warm.
        "state": session.state.as_token(),
        "started_at": session.started_at.to_string(),
        "last_beat": session.last_beat().to_string(),
        "entry_count": session.entries.len(),
    });
    if session.state == SessionState::Abandoned
        && let Some(obj) = choice.as_object_mut()
    {
        obj.insert(
            "note".into(),
            "this run stopped without being wrapped up — a disconnect, a closed laptop, an \
             agent that moved on. Resuming it is ordinary: it reopens where it left off and \
             its chronology continues."
                .into(),
        );
    }
    choice
}

/// **Whether a run's name can pick it up, and from when if not.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pickup {
    /// A name can take it now.
    Now,
    /// It is still its sitting's own: it wrote, or was taken over, at `wrote`,
    /// and a name can take it from `free_at`.
    From {
        wrote: Timestamp,
        free_at: Timestamp,
    },
}

/// **The quiet-lease rule, in one place.** A run that has not stopped is its
/// sitting's own until it has been quiet for `quiet_after`, and a pick-up counts
/// as a write, so a run taken over a moment ago is its new sitting's. A run that
/// stopped is free to a name.
pub(crate) fn pickup_of(
    session: &Session,
    taken_at: Option<Timestamp>,
    now: Timestamp,
    quiet_after: jiff::SignedDuration,
) -> Pickup {
    if session.state.is_terminal() {
        return Pickup::Now;
    }
    let wrote = session.last_beat().max(taken_at.unwrap_or(Timestamp::MIN));
    if now.duration_since(wrote) < quiet_after {
        Pickup::From {
            wrote,
            free_at: wrote + quiet_after,
        }
    } else {
        Pickup::Now
    }
}

/// How a dead-key refusal says its key ended. Carried on the refusal's body as
/// [`ENDED_KEY`] until `finish` reads it, and never shipped.
pub(crate) const ENDED_KEY: &str = "ended_key";

/// Mark a refusal as the answer to a key that stopped opening its run: `kind` is
/// `retired` for a key a resume, a wrap or an exchange replaced, or the state a
/// closed run reached.
pub(crate) fn mark_ended_key(result: CallToolResult, kind: &str) -> CallToolResult {
    let mut result = result;
    for block in &mut result.content {
        let Some(text) = block.as_text() else {
            continue;
        };
        let Ok(serde_json::Value::Object(mut body)) =
            serde_json::from_str::<serde_json::Value>(&text.text)
        else {
            continue;
        };
        body.insert(ENDED_KEY.into(), kind.into());
        *block = ContentBlock::text(serde_json::Value::Object(body).to_string());
        break;
    }
    result
}

/// **The refusal as `finish` leaves it when it has nothing to add**: the mark
/// removed. The snapshot of every refusal shows this, because the mark is never
/// shipped.
#[cfg(test)]
pub(crate) fn unmarked(result: CallToolResult) -> CallToolResult {
    let mut result = result;
    for block in &mut result.content {
        let Some(text) = block.as_text() else {
            continue;
        };
        let Ok(serde_json::Value::Object(mut body)) =
            serde_json::from_str::<serde_json::Value>(&text.text)
        else {
            continue;
        };
        body.remove(ENDED_KEY);
        *block = ContentBlock::text(serde_json::Value::Object(body).to_string());
        break;
    }
    result
}

impl Jojobot {
    /// **Say what is in flight on a refusal for a key that stopped opening its
    /// run**, and remove the mark that asked for it.
    ///
    /// Only a key whose run is known can be answered: a retired key (the
    /// registry remembers the bot) and a live key whose run closed. A key
    /// jojobot never held names no bot, so it carries no mark and is left
    /// exactly as the verb wrote it.
    pub(crate) async fn add_in_flight(&self, answered: &mut CallToolResponse, sid: Option<&str>) {
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        for block in &mut result.content {
            let Some(text) = block.as_text() else {
                continue;
            };
            let Ok(serde_json::Value::Object(mut body)) =
                serde_json::from_str::<serde_json::Value>(&text.text)
            else {
                continue;
            };
            let Some(ended) = body.remove(ENDED_KEY) else {
                continue;
            };
            let ended = ended.as_str().unwrap_or_default().to_string();
            if let Some(raw) = sid.map(str::trim)
                && let Some((bot, wrapped)) = self.bot_of_ended_key(raw, &ended)
                && let Some(listed) = self.in_flight_of(&bot, wrapped).await
            {
                self.say_how_to_go_on(&mut body, &bot, &listed);
                body.insert("in_flight".into(), listed);
            }
            *block = ContentBlock::text(serde_json::Value::Object(body).to_string());
            return;
        }
    }

    /// The bot a dead key belonged to, and whether its run was wrapped.
    fn bot_of_ended_key(&self, raw: &str, ended: &str) -> Option<(EntityId, bool)> {
        if ended == "retired" {
            let retired = self.registry.retired(raw)?;
            return Some((retired.bot, retired.by == sid::RetiredBy::Wrapped));
        }
        let held = self.registry.lookup_key(raw)?;
        Some((held.bot, ended == SessionState::Wrapped.as_token()))
    }

    /// **The bot's runs as the door would offer them, read without a sweep**,
    /// each with the moment a name can take it when it is still its sitting's
    /// own. `None` when the board cannot be read: the refusal then stands as it
    /// was.
    async fn in_flight_of(&self, bot: &EntityId, wrapped: bool) -> Option<serde_json::Value> {
        let now = self.clock().now();
        let board = match jojobot_domain::session::find_without_sweeping(
            self.sessions.as_ref(),
            bot,
            now,
            None,
        )
        .await
        {
            Ok(board) => board,
            Err(e) => {
                tracing::warn!(error = %e, %bot, "the board could not be read for a refusal");
                return None;
            }
        };
        let mut runs = Vec::new();
        let mut next: Option<String> = None;
        for session in board.live.iter().chain(board.offerable.iter()) {
            let mut run = choice_json(session);
            let name = run["run"].as_str().unwrap_or_default().to_string();
            let taken_at = self.registry.lookup_name(&name).and_then(|h| h.taken_at);
            match pickup_of(session, taken_at, now, self.quiet_after) {
                Pickup::Now => {
                    next.get_or_insert(name);
                }
                Pickup::From { free_at, .. } => {
                    run["free_at"] = free_at.to_string().into();
                }
            }
            runs.push(run);
        }
        let resume = next.unwrap_or_else(|| sid::NEW.to_string());
        let mut listed = serde_json::json!({
            "bot": bot.as_str(),
            "runs": runs,
            "next_call": {
                "tool": "start_here",
                "arguments": { "bot": bot.as_str(), "resume": resume },
            },
        });
        if wrapped {
            if let Some(handover) = board.handover.as_ref() {
                listed["handover"] = Self::handover(Some(handover));
            }
            listed["wrap_code"] =
                "If you hold the wrap_code its wrap handed back, call start_here \
                with this bot and resume set to that code: for 2 hours after the wrap the run is \
                a normal session and stays wrapped, and a newer run of the bot ends that window early."
                    .into();
        }
        Some(listed)
    }

    /// The sentence a refusal gains: the one call that goes on, and what it costs.
    fn say_how_to_go_on(
        &self,
        body: &mut serde_json::Map<String, serde_json::Value>,
        bot: &EntityId,
        listed: &serde_json::Value,
    ) {
        let resume = listed["next_call"]["arguments"]["resume"]
            .as_str()
            .unwrap_or(sid::NEW);
        let sentence = if resume == sid::NEW {
            format!(
                "Nothing here can be picked up by its name. To go on, call start_here with bot \
                 {bot} and resume set to \"new\": that starts a run of your own and touches no \
                 other run."
            )
        } else {
            format!(
                "The runs of {bot} in flight are in in_flight. To go on, call start_here with bot \
                 {bot} and resume set to the name of the run you are picking up, {resume} for \
                 one: that replaces the run's key, so the key you hold stops working, and voids \
                 the vouchers the run holds. Or set resume to \"new\" for a run of your own."
            )
        };
        let how = body
            .get("how_to_proceed")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        body.insert("how_to_proceed".into(), format!("{how} {sentence}").into());
    }
}
