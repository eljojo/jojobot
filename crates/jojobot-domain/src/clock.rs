//! **What `now` means for this run of the server.**
//!
//! An instance may be told to act out a day: it is the 1st of June, and every
//! answer jojobot gives is the answer of a server standing in June. That is a
//! property of the RUN and not of any one call — a caller acting out a period
//! should use jojobot as it would on the day, and telling the tool the date on
//! every call is telling a notebook what day it is.
//!
//! **A caller may still name a day**, on the door or on a write, and that
//! still wins. What this adds is a second source under it, so a run that names
//! nothing is answered in the period the server is standing in rather than on
//! the wall clock.
//!
//! ⚠️ **The domain stays clock-free where it decides anything.** Every policy
//! here still takes the instant or the day it is deciding against — the sweep,
//! the due read, the guards. This is the value the EDGES stamp from, and it
//! lives in this crate because the MCP layer and the store adapters both need
//! one word for it.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

/// **The clock this server runs on.**
///
/// [`Clock::Real`] is the wall clock and is what every instance gets unless an
/// operator states a day.
#[derive(Clone, Copy, Debug)]
pub enum Clock {
    /// The wall clock.
    Real,
    /// **A day the server acts out.** Every moment jojobot stamps lands on it,
    /// and every day-grained answer it fills in for a caller is it.
    Stated {
        /// The day being acted out.
        day: Date,
        /// **When this clock was made**, on the real clock, so the stated day
        /// can advance at the real rate. See [`Clock::now`].
        began: Timestamp,
    },
}

impl Default for Clock {
    /// **The wall clock.** A server nobody told a day to runs in the present,
    /// which is where an operator who set nothing already is.
    fn default() -> Self {
        Clock::Real
    }
}

impl Clock {
    /// A server acting out `day`.
    #[must_use]
    pub fn stating(day: Date) -> Self {
        Clock::Stated {
            day,
            began: Timestamp::now(),
        }
    }

    /// **The day this server is acting out**, or `None` on the real clock.
    ///
    /// This is the question a caller-facing announcement asks, and the one the
    /// sweep asks when it wants a day to judge a run against and the caller
    /// named none. **`None` must leave every existing behaviour exactly as it
    /// was**: a fictional day leaking into an ordinary run is worse than the
    /// bug it fixes.
    #[must_use]
    pub fn stated(&self) -> Option<Date> {
        match self {
            Clock::Real => None,
            Clock::Stated { day, .. } => Some(*day),
        }
    }

    /// **The moment to stamp a record with.**
    ///
    /// A stated day starts at its own midnight, in UTC, and advances at the
    /// real rate from there. **It advances rather than standing still because
    /// two writes in one run must be readable in the order they happened**: a
    /// clock that answered one instant would stamp a whole run identically,
    /// which is the wrong reading the moment exists to end.
    ///
    /// Taking the real time of day and moving only the date would do the same
    /// job until the run crossed a real midnight, and then run backwards by a
    /// day. Elapsed-since-made has no such edge: a run longer than a day walks
    /// into the next one, which is time passing rather than time jumping.
    #[must_use]
    pub fn now(&self) -> Timestamp {
        match self {
            Clock::Real => Timestamp::now(),
            Clock::Stated { day, began } => {
                let midnight = Self::midnight(*day);
                let elapsed = Timestamp::now().as_second() - began.as_second();
                midnight
                    .checked_add(SignedDuration::from_secs(elapsed))
                    .unwrap_or(midnight)
            }
        }
    }

    /// **Which day it is here**, in the zone the caller asked in.
    ///
    /// A stated day answers every zone with itself. It is a day somebody named,
    /// exactly as a day named on a call is, and neither is a moment to be read
    /// in a frame — a server told it is the 1st of June is standing in June for
    /// a run in Madrid and for a run in New York.
    #[must_use]
    pub fn today_in(&self, zone: &TimeZone) -> Date {
        match self {
            Clock::Real => Timestamp::now().to_zoned(zone.clone()).date(),
            Clock::Stated { day, .. } => *day,
        }
    }

    /// The start of a day, in UTC. A day is always representable there, so the
    /// fallback below is unreachable rather than a choice.
    fn midnight(day: Date) -> Timestamp {
        day.at(0, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
            .map_or(Timestamp::UNIX_EPOCH, |zoned| zoned.timestamp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> Date {
        raw.parse().expect("a test date")
    }

    /// **The stamp lands on the stated day**, which is what makes *when jojobot
    /// recorded this* true inside the story rather than the day the run
    /// happened to be executed on.
    #[test]
    fn a_stated_clock_stamps_the_day_it_states() {
        let clock = Clock::stating(day("2026-06-01"));

        assert_eq!(
            clock.now().to_zoned(TimeZone::UTC).date(),
            day("2026-06-01"),
            "a stated clock stamps its own day"
        );
    }

    /// **Every zone gets the stated day.** A named day is not a moment, so
    /// there is nothing to read in a frame.
    #[test]
    fn a_stated_day_answers_every_zone_with_itself() {
        let clock = Clock::stating(day("2026-06-01"));

        for zone in ["America/New_York", "Europe/Madrid", "UTC"] {
            assert_eq!(
                clock.today_in(&TimeZone::get(zone).expect("a zone this build resolves")),
                day("2026-06-01"),
                "{zone} reads the stated day"
            );
        }
    }

    /// **The real clock is untouched**, which is the control the whole feature
    /// rests on: an instance nobody told a day to must answer exactly as it did
    /// before one could be told.
    #[test]
    fn the_real_clock_states_no_day_and_answers_the_wall_clock() {
        let clock = Clock::default();

        assert_eq!(clock.stated(), None, "the real clock states no day");
        assert_eq!(
            clock.today_in(&TimeZone::UTC),
            Timestamp::now().to_zoned(TimeZone::UTC).date(),
            "the real clock answers the wall clock"
        );
    }

    /// **A stated clock advances**, so two writes in one run are readable in
    /// the order they happened. Stated as an ordering rather than a rate: what
    /// a reader needs is that the second stamp is not before the first.
    #[test]
    fn a_stated_clock_does_not_run_backwards() {
        let clock = Clock::stating(day("2026-06-01"));

        let first = clock.now();
        let second = clock.now();

        assert!(second >= first, "{second} is not before {first}");
    }
}
