//! **What this server says about the clock it is running on.**
//!
//! An instance may be told to act out a day (see [`jojobot_domain::clock`]).
//! ⛔️ **One that did not say so would be the silently-wrong-state class**: a
//! deployment handed a stated day looks exactly like a healthy one, and every
//! date it fills in and every moment it stamps would be fiction nobody could
//! see from the outside. So the announcement ships with the capability rather
//! than after it.
//!
//! It rides on the two doors a caller reaches for to find out what they are
//! talking to: `start_here`, which every session calls, and `ping`, which is
//! the call that says which server this is.

use super::*;

impl Jojobot {
    /// **The clock this server runs on** — the real one unless an operator
    /// stated a day.
    pub(crate) fn clock(&self) -> &jojobot_domain::clock::Clock {
        &self.clock
    }

    /// **What a door says about a stated day**, or `None` on the real clock.
    ///
    /// Absent is the ordinary answer, so nothing is added to what a real
    /// deployment's callers read. What is present is the exception, and it says
    /// what the day changes rather than only naming it: a reader who learns the
    /// day but not that the stamps moved with it would take the record's own
    /// moments as real.
    pub(crate) fn stated_clock(&self) -> Option<serde_json::Value> {
        let day = self.clock.stated()?;
        Some(serde_json::json!({
            "stated_day": day.to_string(),
            "note": format!(
                "THIS SERVER IS ACTING OUT {day}. It is not running on the real clock. A date \
                 you do not name is {day}, whether something recurring has come due is asked of \
                 {day}, a run is judged stale against {day}, and every moment jojobot stamps a \
                 record with lands on {day}. Nothing here is a fault and nothing needs working \
                 around: the operator set the day, and jojobot never moves it on its own. You \
                 may still name a date on any call, and yours still wins."
            ),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::PostMessageArgs;
    use crate::session::{JournalArgs, WrapSessionArgs};
    use jojobot_domain::clock::Clock;
    use rmcp::handler::server::wrapper::Parameters;

    /// The day these cases act out.
    const JUNE: &str = "2026-06-01";

    fn acting() -> Jojobot {
        handler().on_clock(Clock::stating(JUNE.parse().expect("a day")))
    }

    /// **Everything this layer stamps lands on the stated day.**
    ///
    /// Three verbs, because each binds its own moment in its own file and a
    /// case over one of them says nothing about the others: a beat, the entry
    /// a wrap closes with, and a message's `sent_at`. **When jojobot recorded
    /// it** is the reading these columns give, and on a server acting out June
    /// the real answer is June.
    ///
    /// The moment is read back through the surface a caller uses, so a build
    /// where the stamp never reaches storage fails this too.
    #[tokio::test]
    async fn a_run_on_a_stated_day_stamps_what_it_writes_with_that_day() {
        let jojobot = acting();
        // The box a post lands in opens with the bot that owns it.
        make_bot(&jojobot, "gamma").await;
        let sid = as_bot(&jojobot, "gamma");

        let beat = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert!(
            beat["entry"]["at"]
                .as_str()
                .is_some_and(|at| at.starts_with(JUNE)),
            "a beat is stamped with the day the server is acting out: {beat}"
        );

        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "gamma".into(),
                    sid: sid.clone(),
                    body: "the kiln reached temperature".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        assert!(
            posted["sent_at"]
                .as_str()
                .is_some_and(|at| at.starts_with(JUNE)),
            "a message is sent on the day the server is acting out: {posted}"
        );

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "the slice is built and the bar is green".into(),
                    sid,
                }))
                .await
                .expect("wrap ok"),
        );
        // **The wrap's own entry, by its field.** A containment check over the
        // whole answer passes on the chronology the answer carries beside it,
        // which holds June-stamped entries this case wrote earlier.
        assert!(
            wrapped["entry"]["at"]
                .as_str()
                .is_some_and(|at| at.starts_with(JUNE)),
            "the entry a wrap closes with is stamped with the stated day: {wrapped}"
        );
    }

    /// **The control.** A server nobody told a day to stamps on the wall clock,
    /// which is what every instance does and must keep doing.
    #[tokio::test]
    async fn a_run_on_the_real_clock_stamps_today() {
        let jojobot = handler();
        let sid = as_bot(&jojobot, "gamma");

        let beat = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: None,
                    sid,
                }))
                .await
                .expect("journal ok"),
        );
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            .to_string();
        assert!(
            beat["entry"]["at"]
                .as_str()
                .is_some_and(|at| at.starts_with(&today)),
            "a beat on an ordinary server is stamped with today: {beat}"
        );
    }
}
