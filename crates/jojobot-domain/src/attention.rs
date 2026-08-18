//! Attention — what deserves the user's eyes: watches, decay items, rhythms,
//! quiet plants, the focus surface. Consumes the graph; decides what surfaces
//! at boot and in shapes.
//!
//! # A late check-in names its own date, and there is no default
//!
//! When a check-in is recorded after the moment it was due, the caller says
//! which date the rhythm advances from: the date it was due, or the date the
//! check-in happened. Neither is the default. Omitting the choice is refused
//! with a way forward, rather than resolved quietly.
//!
//! This is a deliberate exception to convention over configuration, which
//! otherwise requires every default to work unconfigured. The operator set it
//! **for now**, and it may become a default in a later version — so a default
//! added here without the operator saying so is a bug rather than a
//! convenience.
//!
//! Why it cannot be guessed: pick wrong, and a backdated check-in never
//! clears the reminder. It fails silently and keeps failing, because each
//! late check-in re-arms the thing it was meant to settle. The reducer and
//! the procedure text take the answer from one place, or they disagree about
//! which date a rhythm counts from.
//!
//! # What this module is, and what it is not
//!
//! **Arithmetic on a declared cadence and a recorded date, and no judgement.**
//! It says which rhythms have gone quiet as of a date somebody names; it never
//! decides that one should run, never runs one, and never reads a clock. The
//! date is an argument, so *what is overdue as of next Friday* is a question
//! that can be asked, and every answer here is a function of what it was
//! handed.

use std::collections::BTreeMap;

use jiff::civil::Date;

/// **The cadence, in days.** A cadence is always TIME: the time prompts the
/// check, and what the check measures — a distance, a reading, a count — is a
/// field on the check-in rather than a unit of the schedule.
pub const CADENCE_DAYS: &str = "cadence_days";

/// **Which date the next cycle counts from**, as a policy: one of
/// [`AdvancesFrom`]'s two tokens. Paired with [`COUNTS_FROM`], which holds the
/// date that policy chose.
pub const ADVANCES_FROM: &str = "advances_from";

/// **The date this cycle counts from.** The rhythm is next due
/// [`CADENCE_DAYS`] after it, which is the whole of the overdue arithmetic.
///
/// It is a stored date rather than a derived one because deriving it means
/// reading every check-in ever recorded and folding them in order — the scan
/// the projection exists to replace. A consuming check-in moves it; a snooze
/// leaves it exactly where it was, which is how a snoozed rhythm returns at
/// its own date instead of at a later one.
pub const COUNTS_FROM: &str = "counts_from";

/// **The day of the last check-in** — what *when did I last do this* reads,
/// and it is a different question from [`COUNTS_FROM`]. Every check-in writes
/// it, a snooze included: a snooze is a check-in that recorded a decision, and
/// a rhythm whose last contact was a snooze has not gone unattended.
pub const LAST_CHECK_IN: &str = "last_check_in";

/// **What the last check-in found** — one of [`Outcome`]'s three tokens. Its
/// history is the record the operator asked for: a skipped cycle and a
/// completed one advance the schedule identically, and this is what tells them
/// apart afterwards.
pub const OUTCOME: &str = "outcome";

/// **What a check-in found, and whether the cycle is consumed.**
///
/// Three rather than two, because *it happened* and *it did not happen and the
/// cycle moves on anyway* are different facts about the same schedule, and the
/// difference is only readable later if it was recorded at the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It happened. The cycle advances.
    Ran,
    /// It did not happen, and the cycle advances as if it had. The record says
    /// plainly that it did not, which is what makes a skipped cycle
    /// distinguishable from a completed one in the history.
    Skipped,
    /// **The cycle is not consumed.** The rhythm comes back at its own date and
    /// is acted on again, so nothing about the schedule moves.
    Snoozed,
}

impl Outcome {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Outcome::Ran => "ran",
            Outcome::Skipped => "skipped",
            Outcome::Snoozed => "snoozed",
        }
    }

    /// The outcome a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Outcome> {
        match token.trim() {
            "ran" => Some(Outcome::Ran),
            "skipped" => Some(Outcome::Skipped),
            "snoozed" => Some(Outcome::Snoozed),
            _ => None,
        }
    }

    /// **Does this outcome consume the cycle** — the one property that
    /// separates the three.
    pub fn consumes_the_cycle(self) -> bool {
        match self {
            Outcome::Ran | Outcome::Skipped => true,
            Outcome::Snoozed => false,
        }
    }

    /// Every outcome there is, for a refusal that has to offer them.
    pub const ALL: [Outcome; 3] = [Outcome::Ran, Outcome::Skipped, Outcome::Snoozed];
}

/// **Which date the next cycle counts from**, when a check-in lands after the
/// day it was due.
///
/// There is **no default**, and that is the deliberate exception this module's
/// header describes. The two answers diverge exactly when a check-in is late,
/// and picking the wrong one leaves a rhythm that re-arms itself for ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancesFrom {
    /// From the date it fell due. A check-in a week late still puts the next
    /// one a cadence after the ORIGINAL date — which keeps a weekly loop on
    /// its day of the week, and which can leave the next one already in the
    /// past.
    DueDate,
    /// From the date the check-in happened. The schedule drifts with the
    /// doing, and the next one is always a whole cadence away.
    CheckInDate,
}

impl AdvancesFrom {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            AdvancesFrom::DueDate => "due_date",
            AdvancesFrom::CheckInDate => "check_in_date",
        }
    }

    /// The choice a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<AdvancesFrom> {
        match token.trim() {
            "due_date" => Some(AdvancesFrom::DueDate),
            "check_in_date" => Some(AdvancesFrom::CheckInDate),
            _ => None,
        }
    }

    /// Both choices, for a refusal that has to offer them.
    pub const ALL: [AdvancesFrom; 2] = [AdvancesFrom::DueDate, AdvancesFrom::CheckInDate];
}

/// **Why a rhythm cannot take a check-in.** Each one is a key the rhythm does
/// not hold, or holds as something this vocabulary does not contain — never a
/// failure and never a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotSchedulable {
    /// The key at fault.
    pub key: &'static str,
    /// What the rhythm holds under it, when it holds anything. `None` is the
    /// key nobody wrote, and the two need different repairs: one is a value to
    /// correct, the other is a value to add.
    pub held: Option<String>,
    /// The values this key accepts, when it is a vocabulary. Empty for a key
    /// whose value is a number or a date.
    pub accepts: Vec<&'static str>,
}

impl std::fmt::Display for NotSchedulable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let accepts = if self.accepts.is_empty() {
            String::new()
        } else {
            format!(" ({})", self.accepts.join(" or "))
        };
        match &self.held {
            None => write!(
                f,
                "this rhythm holds no '{}'{accepts}, so nothing here can say when the next cycle \
                 falls due",
                self.key
            ),
            Some(held) => write!(
                f,
                "this rhythm's '{}' holds '{held}', which is not a value this reads{accepts}",
                self.key
            ),
        }
    }
}

/// A rhythm's schedule, read off the fields it holds.
///
/// It is built from the FOLDED fields — every write on the thing, projected to
/// one value per key — because a rhythm is described a record at a time: the
/// record that set it up carries the cadence, and each check-in since carries
/// what it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// How many days a cycle lasts.
    pub cadence_days: i32,
    /// Which date the next cycle counts from, when a check-in is late.
    pub advances_from: AdvancesFrom,
    /// The date this cycle counts from.
    pub counts_from: Date,
}

impl Schedule {
    /// **The date this rhythm is next due**: a cadence after the date it counts
    /// from.
    pub fn due_on(&self) -> Date {
        self.counts_from
            .checked_add(jiff::Span::new().days(self.cadence_days))
            // A cadence that runs off the end of the calendar is a value no
            // rhythm carries; the saturating answer keeps the read total, and
            // a date that cannot move further is one nothing is overdue by.
            .unwrap_or(Date::MAX)
    }

    /// **Has this rhythm gone quiet as of `as_of`** — due on that day or
    /// before it. The day it falls due is a day it is due, not the day after.
    pub fn overdue_on(&self, as_of: Date) -> bool {
        self.due_on() <= as_of
    }

    /// **The schedule this rhythm has after a check-in on `on`.**
    ///
    /// A consuming outcome moves the date the cycle counts from; a snooze
    /// returns the schedule unchanged, which is what makes the rhythm come back
    /// at its own date rather than a cadence later.
    pub fn after(&self, outcome: Outcome, on: Date) -> Schedule {
        if !outcome.consumes_the_cycle() {
            return *self;
        }
        let counts_from = match self.advances_from {
            AdvancesFrom::DueDate => self.due_on(),
            AdvancesFrom::CheckInDate => on,
        };
        Schedule {
            counts_from,
            ..*self
        }
    }
}

/// **Read a rhythm's schedule off what it holds**, or say which key stops it.
///
/// Nothing is defaulted here. A rhythm missing any of the three is one nothing
/// can compute a due date for, and the answer names the key rather than picking
/// a value that would be wrong in a way nobody could see.
pub fn schedule_of(fields: &BTreeMap<String, String>) -> Result<Schedule, NotSchedulable> {
    let cadence_days = read(fields, CADENCE_DAYS, &[], |held| {
        held.parse::<i32>().ok().filter(|days| *days > 0)
    })?;
    let advances_from = read(
        fields,
        ADVANCES_FROM,
        &AdvancesFrom::ALL.map(AdvancesFrom::as_token),
        AdvancesFrom::of_token,
    )?;
    let counts_from = read(fields, COUNTS_FROM, &[], |held| held.parse::<Date>().ok())?;
    Ok(Schedule {
        cadence_days,
        advances_from,
        counts_from,
    })
}

/// One key of a schedule, read through its own parser — so a key nobody wrote
/// and a key holding something unreadable come back as the two different
/// answers they are.
fn read<T>(
    fields: &BTreeMap<String, String>,
    key: &'static str,
    accepts: &[&'static str],
    parse: impl Fn(&str) -> Option<T>,
) -> Result<T, NotSchedulable> {
    let held = fields.get(key).map(|v| v.trim()).filter(|v| !v.is_empty());
    let refuse = |held: Option<&str>| NotSchedulable {
        key,
        held: held.map(str::to_string),
        accepts: accepts.to_vec(),
    };
    let held = held.ok_or_else(|| refuse(None))?;
    parse(held).ok_or_else(|| refuse(Some(held)))
}

/// **Has this rhythm gone quiet as of `as_of`.**
///
/// A rhythm whose schedule cannot be read is overdue. That is the loud answer
/// rather than the tidy one, and it is deliberate: the alternative is a rhythm
/// that carries half a schedule, surfaces at no boot ever, and is never heard
/// from again — the silent failure this whole module is written against. It
/// comes back with its fields, so a caller can see which key it is short of.
pub fn overdue(fields: &BTreeMap<String, String>, as_of: Date) -> bool {
    match schedule_of(fields) {
        Ok(schedule) => schedule.overdue_on(as_of),
        Err(_) => true,
    }
}

/// **What a check-in writes**, given what the rhythm holds now.
///
/// The caller supplies the outcome and the day; everything else is arithmetic
/// this module owns, because a caller doing it by hand is a caller who can get
/// it wrong once and never find out. Returns the fields the check-in record
/// carries — which are also, folded onto the thing, the rhythm's new state.
pub fn check_in(
    fields: &BTreeMap<String, String>,
    outcome: Outcome,
    on: Date,
) -> Result<BTreeMap<String, String>, NotSchedulable> {
    let schedule = schedule_of(fields)?;
    let mut written = BTreeMap::new();
    written.insert(OUTCOME.to_string(), outcome.as_token().to_string());
    written.insert(LAST_CHECK_IN.to_string(), on.to_string());
    let moved = schedule.after(outcome, on);
    // **A snooze writes no date the schedule reads.** Writing the unchanged
    // value would be indistinguishable in the fold from a cycle that advanced
    // onto the same day, and it would put a write in the key's history that
    // nothing did.
    if moved.counts_from != schedule.counts_from {
        written.insert(COUNTS_FROM.to_string(), moved.counts_from.to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    /// A rhythm that holds a whole schedule, as its fields fold to.
    fn weekly(counts_from: Date, advances_from: AdvancesFrom) -> BTreeMap<String, String> {
        [
            (CADENCE_DAYS.to_string(), "7".to_string()),
            (
                ADVANCES_FROM.to_string(),
                advances_from.as_token().to_string(),
            ),
            (COUNTS_FROM.to_string(), counts_from.to_string()),
        ]
        .into_iter()
        .collect()
    }

    /// **The day it falls due is a day it is due**, and the day before is not.
    /// The boundary is the whole of the read, so it is asserted from both
    /// sides: a comparison off by one day answers every other case correctly.
    #[test]
    fn a_rhythm_is_overdue_from_the_day_it_falls_due() {
        let fields = weekly(date(2026, 8, 1), AdvancesFrom::CheckInDate);
        let schedule = schedule_of(&fields).expect("a whole schedule reads");
        assert_eq!(schedule.due_on(), date(2026, 8, 8));

        assert!(!overdue(&fields, date(2026, 8, 7)), "the day before is not");
        assert!(overdue(&fields, date(2026, 8, 8)), "the day itself is");
        assert!(overdue(&fields, date(2026, 8, 20)), "and every day after");
    }

    /// **The two choices part company on a late check-in, and only then.**
    ///
    /// Both arms from one starting schedule, because the pair is the point: a
    /// build that ignored `advances_from` altogether passes either assertion
    /// alone.
    #[test]
    fn a_late_check_in_advances_from_the_date_the_rhythm_names() {
        let late = date(2026, 8, 12);

        // Due on the 8th, checked in on the 12th. From the due date, the next
        // one is a cadence after the 8th and the loop keeps its day.
        let from_due = weekly(date(2026, 8, 1), AdvancesFrom::DueDate);
        let moved = schedule_of(&from_due)
            .expect("a whole schedule reads")
            .after(Outcome::Ran, late);
        assert_eq!(moved.counts_from, date(2026, 8, 8));
        assert_eq!(moved.due_on(), date(2026, 8, 15));

        // The same check-in, from the day it happened: the schedule drifts, and
        // the next one is a whole cadence away from the doing.
        let from_check_in = weekly(date(2026, 8, 1), AdvancesFrom::CheckInDate);
        let moved = schedule_of(&from_check_in)
            .expect("a whole schedule reads")
            .after(Outcome::Ran, late);
        assert_eq!(moved.counts_from, late);
        assert_eq!(moved.due_on(), date(2026, 8, 19));
    }

    /// **The failure the no-default exists to prevent, reproduced.**
    ///
    /// A rhythm months behind, checked in today, advancing from the due date:
    /// the next cycle lands in the past, so the check-in that was meant to
    /// settle it re-arms it instead. Nothing here is broken — this is what the
    /// caller asked for — and it is why the choice cannot be guessed on their
    /// behalf.
    #[test]
    fn advancing_from_a_long_past_due_date_leaves_the_rhythm_still_overdue() {
        let fields = weekly(date(2026, 5, 1), AdvancesFrom::DueDate);
        let today = date(2026, 8, 18);
        assert!(overdue(&fields, today), "it starts overdue");

        let moved = schedule_of(&fields)
            .expect("a whole schedule reads")
            .after(Outcome::Ran, today);
        assert!(
            moved.overdue_on(today),
            "and a check-in that advances from the due date leaves it overdue: {moved:?}",
        );

        // The other choice settles it in one, which is what makes the pair a
        // decision rather than a formality.
        let drifting = weekly(date(2026, 5, 1), AdvancesFrom::CheckInDate);
        assert!(
            !schedule_of(&drifting)
                .expect("a whole schedule reads")
                .after(Outcome::Ran, today)
                .overdue_on(today),
            "advancing from the check-in date clears it",
        );
    }

    /// **Three outcomes, and the difference is whether the cycle is consumed.**
    /// A run and a skip move the schedule identically; only the record they
    /// leave tells them apart. A snooze moves nothing.
    #[test]
    fn a_skip_advances_the_cycle_and_a_snooze_does_not() {
        let fields = weekly(date(2026, 8, 1), AdvancesFrom::CheckInDate);
        let on = date(2026, 8, 10);

        let ran = check_in(&fields, Outcome::Ran, on).expect("a whole schedule takes a check-in");
        let skipped =
            check_in(&fields, Outcome::Skipped, on).expect("a whole schedule takes a check-in");
        assert_eq!(
            ran.get(COUNTS_FROM),
            skipped.get(COUNTS_FROM),
            "a skipped cycle advances exactly as a completed one does",
        );
        assert_eq!(ran.get(COUNTS_FROM).map(String::as_str), Some("2026-08-10"));
        assert_eq!(
            (
                ran.get(OUTCOME).map(String::as_str),
                skipped.get(OUTCOME).map(String::as_str)
            ),
            (Some("ran"), Some("skipped")),
            "and the record is the only place the difference survives",
        );

        let snoozed =
            check_in(&fields, Outcome::Snoozed, on).expect("a whole schedule takes a check-in");
        assert_eq!(
            snoozed.get(COUNTS_FROM),
            None,
            "a snooze writes no schedule date, so the fold keeps the one it had",
        );
        assert_eq!(
            snoozed.get(LAST_CHECK_IN).map(String::as_str),
            Some("2026-08-10"),
            "…but it is still a check-in, and it says when it happened",
        );
    }

    /// **A key the rhythm does not hold is named, and a key it holds wrongly is
    /// named differently.** Both are the caller's to repair and the repairs are
    /// not the same, so one sentence for both would send a reader looking for
    /// the wrong thing.
    #[test]
    fn a_rhythm_short_of_a_key_says_which_one_and_takes_no_check_in() {
        let mut fields = weekly(date(2026, 8, 1), AdvancesFrom::CheckInDate);
        fields.remove(ADVANCES_FROM);

        let refused = check_in(&fields, Outcome::Ran, date(2026, 8, 10))
            .expect_err("a rhythm that cannot say what it advances from takes no check-in");
        assert_eq!(refused.key, ADVANCES_FROM);
        assert_eq!(refused.held, None);
        assert_eq!(refused.accepts, vec!["due_date", "check_in_date"]);

        fields.insert(ADVANCES_FROM.to_string(), "whenever".to_string());
        let refused = check_in(&fields, Outcome::Ran, date(2026, 8, 10))
            .expect_err("a value outside the vocabulary is not a choice");
        assert_eq!(refused.held.as_deref(), Some("whenever"));

        // And an unreadable schedule is loud rather than quiet: it surfaces as
        // overdue instead of vanishing from every answer for ever.
        assert!(overdue(&fields, date(2026, 8, 10)));
    }

    /// **A rhythm nobody has scheduled is overdue.** The first thing a caller
    /// does with a new rhythm is create it, and the record that gives it a
    /// cadence is a separate write — so between the two it holds nothing. The
    /// loud answer is what gets it finished; the tidy one is a rhythm nobody
    /// ever hears from.
    #[test]
    fn a_rhythm_holding_nothing_is_overdue_rather_than_invisible() {
        assert!(overdue(&BTreeMap::new(), date(2026, 8, 18)));
    }
}
