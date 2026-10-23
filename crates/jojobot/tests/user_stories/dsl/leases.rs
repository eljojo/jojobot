//! **A lease lapses on a clock the story moves, not on the wall clock.** A server
//! restarted over the same stores reads a clock of its own, so a story that must
//! let a lease run out restarts on a clock that reads later than the real one
//! instead of sleeping for the lease.

use super::{Story, Timing};

impl Story {
    /// **The same world served again on a lease of its own length, on a clock that
    /// reads `ahead` later than the wall clock** and runs at the real rate from
    /// there. A run that last wrote less than `ahead - lease` ago reads as quiet
    /// to it, so the story does not wait the lease out. With no `ahead` it reads
    /// the wall clock, and it is the restart on a lease of its own length.
    ///
    /// The clock states today's day, at midnight UTC, and is made to read that
    /// much later: a stated clock reads its day's midnight plus the time since it
    /// began, so it begins `ahead` before midnight. A run written in the last
    /// minutes of the day on the wall clock is not swept by it, because the sweep
    /// of a stated day counts days.
    pub async fn restarted_on_a_lease_of_a_clock_ahead_by(
        &self,
        lease: std::time::Duration,
        ahead: std::time::Duration,
    ) -> Self {
        let clock = if ahead.is_zero() {
            jojobot_domain::clock::Clock::default()
        } else {
            let tz = jiff::tz::TimeZone::UTC;
            let day = jiff::Timestamp::now().to_zoned(tz.clone()).date();
            let midnight = day.to_zoned(tz).expect("a day has a midnight").timestamp();
            let ahead = jiff::SignedDuration::try_from(ahead).expect("a distance is a duration");
            jojobot_domain::clock::Clock::Stated {
                day,
                began: midnight
                    .checked_sub(ahead)
                    .expect("a moment before midnight"),
            }
        };
        Self::spawn_with(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            Timing {
                clock,
                quiet_after: Some(
                    jiff::SignedDuration::try_from(lease).expect("a lease is a duration"),
                ),
            },
            None,
        )
        .await
    }
}
