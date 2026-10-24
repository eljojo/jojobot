//! **The clock's end of a wrap window.** A wrap hands back a code, and for
//! [`WRAP_WINDOW_HOURS`] after the wrap the run it closed is a normal session:
//! the code reopens it and it takes every write a live run takes, a second wrap
//! not needed. The stores judge a window by what they hold and keep no clock, so
//! this layer ends a window the clock has run out, in the one place each verb
//! that writes to a run reaches, and says so in plain words.

use jojobot_domain::session::{Session, SessionError, SessionId, WRAP_WINDOW_HOURS};

use crate::*;

/// **The refusal of a write over a run the store cannot read**: `person`. The
/// store answered and what it holds does not parse, so the same call meets the
/// same damage. The field is named; the damaged cell is not.
pub(crate) fn run_unreadable(card: &SessionId, why: &str) -> CallToolResult {
    tracing::warn!(run = %card, why, "a run could not be read to judge a write");
    let mut body = serde_json::json!({
        "status": "blocked",
        "attempted": card.as_str(),
        "wrote": false,
    });
    crate::answer::WayForward::person(crate::boundary::damaged(
        "write",
        &format!("run {card}: {why}"),
    ))
    .write_into(&mut body);
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A bot's runs that could not be listed, as the door answers it.** Stored
/// damage in any run is a refusal for a person, naming the field and the run the
/// list read stopped at; any other failure is the outage the door has always
/// answered, carrying `note`. A damaged run is never a store that was not reached.
pub(crate) fn board_unread(
    e: &SessionError,
    bot: &EntityId,
    note: &str,
) -> Result<serde_json::Value, CallToolResult> {
    if let SessionError::Unreadable { what } = e {
        tracing::warn!(%bot, what, "a run of the bot could not be read to list its runs");
        let mut body = serde_json::json!({
            "status": "blocked",
            "attempted": bot.as_str(),
            "wrote": false,
        });
        crate::answer::WayForward::person(crate::boundary::damaged(
            "start_here",
            &format!("a run of {bot}: {what}"),
        ))
        .write_into(&mut body);
        return Err(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]));
    }
    tracing::warn!(error = %e, %bot, "the session world is not reachable");
    Ok(serde_json::json!({ "available": false, "note": note }))
}

/// The refusal of a write whose run's window could not be judged: `retry`.
pub(crate) fn window_unjudged(card: &SessionId, what: &str) -> CallToolResult {
    let mut body = serde_json::json!({
        "status": "blocked",
        "attempted": card.as_str(),
        "wrote": false,
    });
    crate::answer::WayForward::retry(format!(
        "Nothing was written. jojobot could not {what}, so it cannot tell whether \
         this run's wrap window is still open. Send the same call again."
    ))
    .write_into(&mut body);
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

impl Jojobot {
    /// **End the window of a wrapped run the clock has run out**, so the store's
    /// own check agrees with the clock. Returns whether it was ended.
    ///
    /// **It fails closed.** A run the store cannot read, or a lapsed window it
    /// will not end, refuses the write that asked, because a write let through
    /// either way lands after the window closed. Stored damage needs a person;
    /// a read or clear that fails can be retried.
    pub(crate) async fn end_a_lapsed_window(
        &self,
        card: &SessionId,
    ) -> Result<bool, CallToolResult> {
        // A run the store holds no record of has no window to judge.
        Ok(self
            .wrapped_run_for_write(card)
            .await?
            .is_some_and(|(_, lapsed)| lapsed))
    }

    /// **The run a write is judged against, read once.** A run that is not
    /// wrapped is judged by its state alone, which the one cheap summary read
    /// answers, so a write on a live run reads no chronology. A wrapped run is
    /// read in full once, and that read judges both the clock's end of its window
    /// and whether it still takes writes. `None` is a run the write is not
    /// refused over. An unreadable summary refuses the write with `person`,
    /// because a write let through then could land after the window. A read
    /// that fails is retried.
    pub(crate) async fn wrapped_run_for_write(
        &self,
        card: &SessionId,
    ) -> Result<Option<(Session, bool)>, CallToolResult> {
        match self.sessions.summary_of(card).await {
            // **A run the store names as unreadable is a run it cannot read**, whatever
            // state the store filed it under: a damaged wrapped run is filed as
            // abandoned, and taken at that word it would take the write.
            Ok(Some(summary)) if summary.unreadable.is_some() => {
                let why = summary.unreadable.as_deref().expect("unreadable summary");
                return Err(run_unreadable(card, why));
            }
            Ok(Some(summary))
                if summary.state == jojobot_domain::session::SessionState::Wrapped => {}
            Ok(_) => return Ok(None),
            // **Damage the store names as a typed error is a person's** whichever
            // read met it; only a read that failed is retried.
            Err(SessionError::Unreadable { what }) => return Err(run_unreadable(card, &what)),
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a run could not be read to judge its wrap window");
                return Err(window_unjudged(card, "read this run"));
            }
        }
        let session = match self.sessions.read_session(card).await {
            Ok(session) => session,
            Err(SessionError::UnknownSession { .. }) => return Ok(None),
            // **Damage the summary did not name** is a person's, as the summary's
            // own is; only a read that failed is retried.
            Err(SessionError::Unreadable { what }) => return Err(run_unreadable(card, &what)),
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a run could not be read to judge its wrap window");
                return Err(window_unjudged(card, "read this run"));
            }
        };
        let lapsed = self.end_the_window_if_lapsed(card, &session).await?;
        Ok(Some((session, lapsed)))
    }

    /// [`Self::end_a_lapsed_window`] over a run the caller has already read, so a
    /// write that needs the run for more than this reads it once.
    pub(crate) async fn end_the_window_if_lapsed(
        &self,
        card: &SessionId,
        session: &Session,
    ) -> Result<bool, CallToolResult> {
        if !session.window_lapsed_at(self.clock().now()) {
            return Ok(false);
        }
        match self.sessions.set_wrap_window(card, None).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a lapsed wrap window could not be ended");
                Err(window_unjudged(
                    card,
                    "end the wrap window the clock has run out",
                ))
            }
        }
    }

    /// **When a window ends, in the run's own zone** (the instance's when the run
    /// stated none, UTC when the instance holds none), with the zone's name.
    pub(crate) async fn window_end_in_words(
        &self,
        timezone: Option<&str>,
        ends: jiff::Timestamp,
    ) -> String {
        let zone = match timezone.and_then(|name| jiff::tz::TimeZone::get(name).ok()) {
            Some(zone) => zone,
            None => self.unzoned_frame().await,
        };
        format!(
            "{} {}",
            ends.to_zoned(zone.clone()).strftime("%Y-%m-%d %H:%M"),
            zone.iana_name().unwrap_or("UTC")
        )
    }

    /// **The one sentence that says what a window is**, for every answer that
    /// hands a code back, takes one or refuses one: until when, what the run may
    /// do, and what ends it sooner.
    pub(crate) async fn window_sentence(&self, session: &Session, ends: jiff::Timestamp) -> String {
        let until = self
            .window_end_in_words(session.timezone.as_deref(), ends)
            .await;
        format!(
            "Until {until}, {WRAP_WINDOW_HOURS} hours after the wrap, this run is a normal \
             session: it takes every write a live run takes, the closing story included, and \
             it need not wrap again. Wrapping again, or a newer run of this bot, closes the \
             window early."
        )
    }
}

#[cfg(test)]
mod tests {
    use jojobot_domain::session::WRAP_WINDOW_HOURS;

    /// **Every served mention of the window's length is the constant.** The
    /// tool descriptions are attributes and doc comments, which cannot read
    /// [`WRAP_WINDOW_HOURS`], so the served texts are read here: every count
    /// of hours written beside a wrap is the constant, and there are some.
    #[test]
    fn every_served_mention_of_the_wrap_window_is_the_constant() {
        let served = [
            include_str!("../../served-texts/tools.md"),
            include_str!("../../served-texts/refusals.md"),
        ];
        let mut mentions = 0;
        for text in served {
            let words: Vec<&str> = text.split_whitespace().collect();
            for (at, word) in words.iter().enumerate() {
                // **Both spellings of a count**: "2 hours" is a number then the
                // word, and "2-hour" is one word.
                let count = if let Some((number, unit)) = word.split_once('-')
                    && unit.starts_with("hour")
                {
                    number.parse::<i64>()
                } else if word.starts_with("hours") && at > 0 {
                    words[at - 1].parse::<i64>()
                } else {
                    continue;
                };
                let Ok(count) = count else {
                    continue;
                };
                let near = words[at.saturating_sub(40)..(at + 40).min(words.len())].join(" ");
                if !near.contains("wrap") {
                    continue;
                }
                mentions += 1;
                assert_eq!(count, WRAP_WINDOW_HOURS, "a served window length: {near}");
            }
        }
        assert!(
            mentions >= 5,
            "the served texts name the window's length: {mentions}"
        );
        // **The one place the number is written down as a literal**: the
        // tool descriptions are attributes that cannot read the constant, so
        // they spell it, and the scan above holds each spelling to the value.
        assert_eq!(WRAP_WINDOW_HOURS, 2);
    }
}
