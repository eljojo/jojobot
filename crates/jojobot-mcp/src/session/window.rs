//! **The clock's end of a wrap window.** A wrap hands back a code, and for
//! [`WRAP_WINDOW_HOURS`] after the wrap the run it closed is a normal session:
//! the code reopens it and it takes every write a live run takes, a second wrap
//! not needed. The stores judge a window by what they hold and keep no clock, so
//! this layer ends a window the clock has run out, in the one place each verb
//! that writes to a run reaches, and says so in plain words.

use jojobot_domain::session::{Session, SessionId, WRAP_WINDOW_HOURS};

use crate::*;

impl Jojobot {
    /// **End the window of a wrapped run the clock has run out**, so the store's
    /// own check agrees with the clock. Returns whether it was ended. A read or a
    /// write the store refuses leaves the window as it was and says so in the log:
    /// the next call tries again.
    pub(crate) async fn end_a_lapsed_window(&self, card: &SessionId) -> bool {
        let Ok(session) = self.sessions.read_session(card).await else {
            return false;
        };
        if !session.window_lapsed_at(self.clock().now()) {
            return false;
        }
        match self.sessions.set_wrap_window(card, None).await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!(error = %e, run = %card, "a lapsed wrap window could not be ended");
                false
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
