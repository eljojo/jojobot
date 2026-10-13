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

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

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

    /// **This clock, made to continue after the newest stamp the store already
    /// holds.** A stated clock restarts at its day's midnight in every process,
    /// so a process started over a store another process filled would stamp its
    /// first writes before that process's last ones.
    ///
    /// **It moves when the clock began, not what a stamp reads.** `began` is set
    /// back by the distance from midnight to the newest stamp, so the clock
    /// reads that stamp now and advances at the real rate from it. A stamp that
    /// is before the stated day, and a wall clock, leave the clock as it was.
    #[must_use]
    pub fn continuing_after(self, newest: Option<Timestamp>) -> Self {
        let Clock::Stated { day, began } = self else {
            return self;
        };
        let midnight = Self::midnight(day);
        let Some(newest) = newest.filter(|stamp| *stamp >= midnight) else {
            return self;
        };
        let held_back = newest.duration_since(midnight);
        match began.checked_sub(held_back) {
            Ok(began) => Clock::Stated { day, began },
            Err(_) => self,
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
                // **The elapsed time keeps its fraction of a second.** Counted in
                // whole seconds, every write inside one second would carry the
                // same stamp, and a reader that sorts or takes the oldest by stamp
                // would pick among them by whatever order it holds them in.
                let elapsed = Timestamp::now().duration_since(*began);
                midnight.checked_add(elapsed).unwrap_or(midnight)
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

    /// **Which day a moment falls on**, in the same frame [`Clock::today_in`]
    /// answers in, so a day read off a moment and a day read off the clock can sit
    /// in one sentence. On the wall clock that is the moment in the zone the
    /// caller asked in. A stated clock stamps its moments on its stated day, so a
    /// moment it stamped belongs to that day whatever the zone: it is read in UTC,
    /// which is where the stated day starts.
    pub fn day_of(&self, moment: Timestamp, zone: &TimeZone) -> Date {
        match self {
            Clock::Real => moment.to_zoned(zone.clone()).date(),
            Clock::Stated { .. } => moment.to_zoned(TimeZone::UTC).date(),
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
    /// **Two writes inside one second do not share a stamp.** A stated clock
    /// advances so that writes are readable in the order they happened, and a
    /// reader that sorts by stamp, or takes the oldest, is told a lie by two
    /// writes with the same one: their order is then whatever order the reader
    /// happens to hold them in. A pair taken a few milliseconds apart has to
    /// differ, and the later has to be the greater.
    ///
    /// Many pairs, not one: a pair that straddles a whole second would differ
    /// on a clock that counts whole seconds, so a single pair could pass for the
    /// wrong reason. Twenty pairs of three milliseconds do not all straddle one.
    #[test]
    fn two_stamps_taken_milliseconds_apart_differ_and_are_in_order() {
        let clock = Clock::stating(day("2026-06-01"));

        for pair in 0..20 {
            let first = clock.now();
            std::thread::sleep(std::time::Duration::from_millis(3));
            let second = clock.now();
            assert!(
                second > first,
                "pair {pair}: {second} does not come after {first}, so two writes made \
                 milliseconds apart read as simultaneous",
            );
        }
    }

    /// **A clock made after a stamp stamps after it.** The stamp is late in the
    /// stated day, as one left by an earlier process that ran for hours would be.
    #[test]
    fn a_stated_clock_continuing_after_a_stamp_stamps_after_it() {
        let late: Timestamp = "2026-06-01T23:15:00.123456789Z".parse().expect("a moment");
        let clock = Clock::stating(day("2026-06-01")).continuing_after(Some(late));

        let first = clock.now();
        assert!(first > late, "{first} is not after {late}");
        let second = clock.now();
        assert!(second > first, "{second} is not after {first}");
    }

    /// **The control for the case above**: with no stamp to continue from, or one
    /// that is not on the stated day, the clock starts at its day's midnight as
    /// it always did. A clock that continued from anything would pass the case
    /// above for the wrong reason.
    #[test]
    fn a_stated_clock_with_nothing_on_its_day_to_continue_from_starts_at_midnight() {
        let midnight: Timestamp = "2026-06-01T00:00:00Z".parse().expect("a moment");
        let a_minute = std::time::Duration::from_secs(60);
        let before: Timestamp = "2026-05-31T23:59:00Z".parse().expect("a moment");

        for newest in [None, Some(before)] {
            let clock = Clock::stating(day("2026-06-01")).continuing_after(newest);
            let now = clock.now();
            assert!(now >= midnight, "{now} is before the stated day began");
            assert!(
                now.duration_since(midnight).unsigned_abs() < a_minute,
                "{now} did not start at midnight after {newest:?}"
            );
        }
    }

    /// **The wall clock has nothing to continue.** It stays the wall clock
    /// whatever stamp it is handed.
    #[test]
    fn the_real_clock_does_not_continue_from_anything() {
        let late: Timestamp = "2026-06-01T23:15:00Z".parse().expect("a moment");
        let clock = Clock::Real.continuing_after(Some(late));

        assert_eq!(clock.stated(), None);
        assert!(clock.now().duration_since(late).as_secs() > 0);
    }

    /// **A moment is read in the zone asked for on the wall clock, and on the
    /// day it was stamped on a stated clock.** The same moment is the 8th in
    /// Tokyo and the 7th in UTC; a stated clock stamps the 7th and the 7th it
    /// stays, in any zone, because a stated day is a day somebody named and not a
    /// moment to be read in a frame.
    #[test]
    fn a_moment_is_read_in_the_asked_zone_and_a_stated_one_on_its_stated_day() {
        let moment: Timestamp = "2026-10-07T23:30:00Z".parse().expect("a moment");
        let tokyo = TimeZone::get("Asia/Tokyo").expect("a zone");
        assert_eq!(Clock::Real.day_of(moment, &tokyo), day("2026-10-08"));
        assert_eq!(
            Clock::Real.day_of(moment, &TimeZone::UTC),
            day("2026-10-07")
        );

        let stated = Clock::stating(day("2026-10-07"));
        let stamped = stated.now();
        let honolulu = TimeZone::get("Pacific/Honolulu").expect("a zone");
        assert_eq!(stated.day_of(stamped, &tokyo), day("2026-10-07"));
        assert_eq!(stated.day_of(stamped, &honolulu), day("2026-10-07"));
    }
}
