//! **The clock's end of a wrap window.** A wrap hands back a code, and for
//! [`WRAP_WINDOW_HOURS`] after the wrap the run it closed is a normal session:
//! the code reopens it and it takes every write a live run takes, a second wrap
//! not needed. The stores judge a window by what they hold and keep no clock, so
//! this layer ends a window the clock has run out, in the one place each verb
//! that writes to a run reaches, and says so in plain words.

use jojobot_domain::session::{Session, SessionError, SessionId, WRAP_WINDOW_HOURS};

use crate::*;

impl Jojobot {
    /// **End the window of a wrapped run the clock has run out**, so the store's
    /// own check agrees with the clock. Returns whether it was ended.
    ///
    /// **It fails closed.** A run the store cannot read, or a lapsed window it
    /// will not end, refuses the write that asked, because a write let through
    /// either way lands after the window closed. The refusal is `retry`: the
    /// next call reads and clears again.
    pub(crate) async fn end_a_lapsed_window(
        &self,
        card: &SessionId,
    ) -> Result<bool, CallToolResult> {
        let refused = |what: &str| {
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
        };
        let session = match self.sessions.read_session(card).await {
            Ok(session) => session,
            // A run the store holds no record of has no window to judge.
            Err(SessionError::UnknownSession { .. }) => return Ok(false),
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a run could not be read to judge its wrap window");
                return Err(refused("read this run"));
            }
        };
        if !session.window_lapsed_at(self.clock().now()) {
            return Ok(false);
        }
        match self.sessions.set_wrap_window(card, None).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a lapsed wrap window could not be ended");
                Err(refused("end the wrap window the clock has run out"))
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
                if !word.starts_with("hours") || at == 0 {
                    continue;
                }
                let Ok(count) = words[at - 1].parse::<i64>() else {
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
            mentions >= 4,
            "the served texts name the window's length: {mentions}"
        );
    }
}
