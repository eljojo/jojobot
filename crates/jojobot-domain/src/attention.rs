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
/// It is a STORED date rather than one recomputed on every read, because
/// recomputing it means reading every check-in ever recorded and folding them
/// in order — the scan the projection exists to replace. A consuming check-in
/// moves it; a snooze leaves it exactly where it was, which is how a snoozed
/// rhythm returns at its own date instead of at a later one.
///
/// **The check-in that opens a loop supplies it**, from that check-in's own
/// date — see [`check_in`]. So no caller ever has to type it, which is what
/// makes *do not send this key* a rule a caller can actually keep.
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
    pub(crate) key: &'static str,
    /// What the rhythm holds under it, when it holds anything. `None` is the
    /// key nobody wrote, and the two need different repairs: one is a value to
    /// correct, the other is a value to add.
    pub(crate) held: Option<String>,
    /// The values this key accepts, when it is a vocabulary. Empty for a key
    /// whose value is a number or a date.
    pub(crate) accepts: Vec<&'static str>,
    /// **Set when the loop has no basis and a consuming outcome would have
    /// supplied it.** The repair is a different outcome rather than a key to
    /// capture, so the sentence and the way through are both different — and
    /// telling this caller to capture the missing key would send them to type
    /// the one value a check-in exists to derive.
    pub(crate) a_consuming_outcome_would_open_it: bool,
}

impl NotSchedulable {
    /// **Whether this refusal already carries its own way through.**
    ///
    /// A caller composing advice around it needs to know: every other refusal
    /// here names a key to capture, and this one names an outcome to send
    /// instead. Appending the usual advice to it would contradict it.
    pub fn names_its_own_way_through(&self) -> bool {
        self.a_consuming_outcome_would_open_it
    }
}

impl std::fmt::Display for NotSchedulable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let accepts = if self.accepts.is_empty() {
            String::new()
        } else {
            format!(" ({})", self.accepts.join(" or "))
        };
        if self.a_consuming_outcome_would_open_it {
            let opens = Outcome::ALL
                .iter()
                .filter(|outcome| outcome.consumes_the_cycle())
                .map(|outcome| format!("'{}'", outcome.as_token()))
                .collect::<Vec<_>>()
                .join(" or ");
            return write!(
                f,
                "this loop has no basis yet and a snooze does not open one, because a snooze is \
                 the outcome that leaves a schedule where it was and there is no schedule here to \
                 leave. Check in with {opens}, dated the day the loop last ran, and jojobot works \
                 the basis out from there"
            );
        }
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
    pub(crate) cadence_days: i32,
    /// Which date the next cycle counts from, when a check-in is late.
    pub(crate) advances_from: AdvancesFrom,
    /// The date this cycle counts from.
    pub(crate) counts_from: Date,
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
        // Set by `check_in`, which is the only place that knows the outcome.
        a_consuming_outcome_would_open_it: false,
    };
    let held = held.ok_or_else(|| refuse(None))?;
    parse(held).ok_or_else(|| refuse(Some(held)))
}

/// **When one of these falls due, as this carrier computes it.**
///
/// Three answers rather than two, and the third is the whole reason this is not
/// a boolean. A thing that carries no due moment at all is not late — it is
/// simply not that sort of thing, or not yet. A thing whose carrier says it
/// SHOULD have one and cannot compute it is late and loud, because the
/// alternative is a half-built loop that surfaces at no read ever and is never
/// heard from again.
///
/// **A boolean collapses the first two into one answer**, which is the defect
/// this shape exists to avoid: an overdue check that returns true whenever it
/// cannot read a schedule marks every person, place and project overdue, since
/// none of them has a schedule to read.
///
/// **A fourth answer draws the same line one level in.** A thing that
/// SHOULD have a schedule and genuinely cannot say what it is (a key missing
/// or a value that will not parse) is [`Due::Unreadable`] — but a cadence and
/// a policy with no basis yet is not that: it is what declaring the loop
/// leaves behind before its first check-in, and [`Due::Unreadable`] would
/// report it late on the day it was created. Collapsing the two the way
/// `Never`/`Unreadable` used to be collapsed marks every fresh loop overdue
/// before a cycle could possibly have elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// **Nothing here falls due.** The thing carries none of what this carrier
    /// computes a due moment from, so there is nothing to be late about.
    Never,
    /// The day it fell or falls due.
    On(Date),
    /// **A cadence and a policy are declared, and the loop has never been
    /// checked in.** Not a defect: it is the shape `add_entity` plus nothing
    /// else leaves behind, and it stays this way, unowed, until the first
    /// check-in opens it.
    NotYetOpened,
    /// **It should have a due moment and cannot say what it is.** Late, loudly,
    /// and it comes back carrying its fields so a reader can see which key it
    /// is short of.
    Unreadable,
}

impl Due {
    /// **Has this moment passed, as of the day the read was asked about.**
    pub fn owed_on(self, as_of: Date) -> bool {
        match self {
            Due::Never => false,
            Due::On(day) => day <= as_of,
            Due::NotYetOpened => false,
            Due::Unreadable => true,
        }
    }
}

/// **How a KIND of thing says when one of its things falls due.**
///
/// The read asks what is owed and compares a moment to a day; it never knows
/// how any particular sort of thing computes that moment. A loop's is its
/// schedule; the next carrier's will be something else, and it lands by
/// answering here rather than by the read growing a branch.
pub trait Carrier: Send + Sync {
    /// The kind token whose things this computes for.
    fn kind(&self) -> &str;
    /// When this thing falls due, read off what it holds.
    fn due(&self, fields: &BTreeMap<String, String>) -> Due;
}

/// **The loop carrier**: a rhythm falls due a cadence after the date its cycle
/// counts from.
pub struct Rhythms;

impl Carrier for Rhythms {
    fn kind(&self) -> &str {
        "rhythm"
    }

    /// **A loop carrying none of the schedule is not late.** It is a loop
    /// somebody wrote down and has not put a schedule on — the fern, watered
    /// when it looks dry — and nagging it would be inventing a promise nobody
    /// made. A loop carrying SOME of the schedule is the half-built one, and
    /// that is the case the loud answer exists for — unless the only thing
    /// missing is the basis itself, which is the shape a declared, never
    /// checked-in loop takes and not a defect. See [`Due::NotYetOpened`].
    fn due(&self, fields: &BTreeMap<String, String>) -> Due {
        let carries = [CADENCE_DAYS, ADVANCES_FROM, COUNTS_FROM]
            .iter()
            .filter(|key| fields.contains_key(**key))
            .count();
        if carries == 0 {
            return Due::Never;
        }
        match schedule_of(fields) {
            Ok(schedule) => Due::On(schedule.due_on()),
            // **The one key a check-in derives, and only when nobody wrote
            // it.** The same distinction `opens_the_loop` draws: a basis that
            // is present and unreadable is a value to correct, not one this
            // reads as merely unopened.
            Err(why) if why.key == COUNTS_FROM && why.held.is_none() => Due::NotYetOpened,
            Err(_) => Due::Unreadable,
        }
    }
}

/// **What is owed, over any carrier.** The read hands the object's kind and its
/// folded fields; a kind no carrier answers for owes nothing.
pub fn owed(carriers: &[&dyn Carrier], kind: &str, fields: &BTreeMap<String, String>) -> Due {
    carriers
        .iter()
        .find(|carrier| carrier.kind() == kind)
        .map(|carrier| carrier.due(fields))
        .unwrap_or(Due::Never)
}

/// The carriers this build ships.
pub fn shipped() -> Vec<Box<dyn Carrier>> {
    vec![Box::new(Rhythms)]
}

/// **Whether this check-in is the one that opens the loop.**
///
/// Only the basis is ever derived, and only when nobody has written one: the
/// cadence and the policy are declarations no check-in can make for itself,
/// and a basis that is present and unreadable is a value to correct rather
/// than one to overrule.
///
/// **A snooze cannot open a loop.** It is defined as the outcome that leaves
/// the schedule where it was, and on a loop with no basis there is no schedule
/// to leave anywhere — so the one property that separates the three outcomes
/// is what decides this, rather than a rule of its own.
fn opens_the_loop(why: &NotSchedulable, outcome: Outcome) -> bool {
    why.key == COUNTS_FROM && why.held.is_none() && outcome.consumes_the_cycle()
}

/// **What a check-in writes**, given what the rhythm holds now.
///
/// The caller supplies the outcome and the day; everything else is arithmetic
/// this module owns, because a caller doing it by hand is a caller who can get
/// it wrong once and never find out. Returns the fields the check-in record
/// carries — which are also, folded onto the thing, the rhythm's new state.
///
/// **The first cycle rests on a derivation like every later one.** A loop that
/// holds a cadence and a policy but no basis is opened by this check-in, and
/// the basis is the check-in's own date. Without that, opening a loop meant
/// typing the basis by hand — the one move [`COUNTS_FROM`] exists to keep away
/// from callers — because a check-in was refused until a basis it could have
/// supplied was already there.
///
/// **The opening basis is the check-in's date under either policy**, and it is
/// not run through [`Schedule::after`]. [`AdvancesFrom`] chooses between the
/// day a cycle fell due and the day the check-in happened; on an opening
/// check-in no cycle has ever fallen due, so there is nothing for it to choose
/// between and a due date to advance from does not exist.
pub fn check_in(
    fields: &BTreeMap<String, String>,
    outcome: Outcome,
    on: Date,
) -> Result<BTreeMap<String, String>, NotSchedulable> {
    let mut written = BTreeMap::new();
    written.insert(OUTCOME.to_string(), outcome.as_token().to_string());
    written.insert(LAST_CHECK_IN.to_string(), on.to_string());
    match schedule_of(fields) {
        Ok(schedule) => {
            let moved = schedule.after(outcome, on);
            // **A snooze writes no date the schedule reads.** Writing the
            // unchanged value would be indistinguishable in the fold from a
            // cycle that advanced onto the same day, and it would put a write
            // in the key's history that nothing did.
            if moved.counts_from != schedule.counts_from {
                written.insert(COUNTS_FROM.to_string(), moved.counts_from.to_string());
            }
        }
        Err(why) if opens_the_loop(&why, outcome) => {
            written.insert(COUNTS_FROM.to_string(), on.to_string());
        }
        // **The complement of [`opens_the_loop`], and it is read off the same
        // predicate rather than restated.** Reaching here with the basis
        // simply absent means the outcome is the only reason it was not
        // opened, so the refusal says that instead of naming a key to capture.
        Err(mut why) => {
            why.a_consuming_outcome_would_open_it = why.key == COUNTS_FROM && why.held.is_none();
            return Err(why);
        }
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

        assert!(
            !Rhythms.due(&fields).owed_on(date(2026, 8, 7)),
            "the day before is not"
        );
        assert!(
            Rhythms.due(&fields).owed_on(date(2026, 8, 8)),
            "the day itself is"
        );
        assert!(
            Rhythms.due(&fields).owed_on(date(2026, 8, 20)),
            "and every day after"
        );
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
        assert!(Rhythms.due(&fields).owed_on(today), "it starts overdue");

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

    /// A rhythm that holds a cadence and a policy and has never been checked
    /// in — the shape `add_entity` plus one capture leaves behind.
    fn unopened(advances_from: AdvancesFrom) -> BTreeMap<String, String> {
        let mut fields = weekly(date(2026, 8, 1), advances_from);
        fields.remove(COUNTS_FROM);
        fields
    }

    /// **A loop with no basis yet is opened by the check-in itself**, and the
    /// basis is that check-in's own date.
    ///
    /// **Asserted under BOTH policies, which is the whole point of the case.**
    /// `advances_from` chooses between the day a cycle fell due and the day
    /// the check-in happened, and on an opening check-in no cycle has ever
    /// fallen due — so the policy has nothing to choose between and the two
    /// must agree. A build that ran the opening date through
    /// [`Schedule::after`] would pass under `CheckInDate` and put the basis a
    /// whole cadence late under `DueDate`.
    #[test]
    fn an_opening_check_in_derives_the_basis_from_its_own_date() {
        let on = date(2026, 6, 14);
        for advances_from in AdvancesFrom::ALL {
            let fields = unopened(advances_from);
            let opened = check_in(&fields, Outcome::Ran, on)
                .expect("a cadenced loop with no basis takes the check-in that opens it");
            assert_eq!(
                opened.get(COUNTS_FROM).map(String::as_str),
                Some("2026-06-14"),
                "under {}, the basis is the check-in's own date",
                advances_from.as_token(),
            );

            // **The read the caller actually makes**, folded the way the store
            // folds it: what the loop holds now is what it held plus what this
            // check-in wrote.
            let mut folded = fields.clone();
            folded.extend(opened.clone());
            assert_eq!(
                schedule_of(&folded)
                    .expect("an opened loop reads a whole schedule")
                    .due_on(),
                date(2026, 6, 21),
                "and the next one is a cadence after it, under {}",
                advances_from.as_token(),
            );
        }
    }

    /// **Only an outcome that consumes the cycle opens a loop.**
    ///
    /// A snooze is defined as not moving the schedule, and on a loop with no
    /// basis there is no schedule to leave where it was — so it refuses, and
    /// it refuses naming the key a caller can add. A run and a skip both open
    /// it, and they open it identically, exactly as they advance an opened one
    /// identically.
    #[test]
    fn only_an_outcome_that_consumes_the_cycle_opens_a_loop() {
        let on = date(2026, 6, 14);
        let fields = unopened(AdvancesFrom::CheckInDate);

        let ran = check_in(&fields, Outcome::Ran, on).expect("a run opens it");
        let skipped = check_in(&fields, Outcome::Skipped, on).expect("a skip opens it too");
        assert_eq!(
            ran.get(COUNTS_FROM),
            skipped.get(COUNTS_FROM),
            "a skipped opening cycle rests on the same basis a completed one does",
        );

        let refused = check_in(&fields, Outcome::Snoozed, on)
            .expect_err("a snooze moves nothing, so it cannot open a loop that has no basis");
        assert_eq!(refused.key, COUNTS_FROM);
        assert_eq!(
            refused.held, None,
            "and it is the key nobody wrote rather than a value to correct",
        );
    }

    /// **The derive adds a key nobody wrote; it never overrules one somebody
    /// did.** A basis that is present and unreadable is a value to correct, and
    /// silently replacing it would throw away what the caller meant to say.
    #[test]
    fn an_opening_derive_does_not_rescue_a_basis_that_is_unreadable() {
        let mut fields = unopened(AdvancesFrom::CheckInDate);
        fields.insert(COUNTS_FROM.to_string(), "sometime".to_string());

        let refused = check_in(&fields, Outcome::Ran, date(2026, 6, 14))
            .expect_err("a basis that is there and unreadable is not one to derive over");
        assert_eq!(refused.key, COUNTS_FROM);
        assert_eq!(refused.held.as_deref(), Some("sometime"));
    }

    /// **A loop short of its cadence still refuses, and the derive does not
    /// reach it.** The opening derive supplies the one key a check-in can know
    /// by itself; it cannot invent how long a cycle lasts.
    #[test]
    fn an_opening_check_in_cannot_invent_a_cadence() {
        let mut fields = unopened(AdvancesFrom::CheckInDate);
        fields.remove(CADENCE_DAYS);

        let refused = check_in(&fields, Outcome::Ran, date(2026, 6, 14))
            .expect_err("no check-in can say how long a cycle lasts");
        assert_eq!(refused.key, CADENCE_DAYS);
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
        assert!(Rhythms.due(&fields).owed_on(date(2026, 8, 10)));
    }

    /// **A loop carrying none of a schedule is not late, and one carrying half
    /// of it is.**
    ///
    /// One answer for the two marks every person, place and project overdue: a
    /// check that says "late" whenever it cannot read a schedule says it
    /// loudest about the things that have none.
    ///
    /// **The distinction is what the loop CARRIES, not what it is.** A loop
    /// somebody wrote down and has not put a schedule on is the fern — watered
    /// when it looks dry — and nagging it invents a promise nobody made. A loop
    /// carrying a cadence and no date is the half-built one, and the loud
    /// answer is what gets it finished.
    ///
    /// Both in one case, because either alone passes against a build that
    /// answers the same for everything.
    #[test]
    fn a_loop_with_no_schedule_is_not_late_and_half_a_schedule_is() {
        assert_eq!(
            Rhythms.due(&BTreeMap::new()),
            Due::Never,
            "a loop nobody has scheduled owes nothing",
        );
        let half: BTreeMap<String, String> = [(CADENCE_DAYS.to_string(), "7".to_string())]
            .into_iter()
            .collect();
        assert_eq!(
            Rhythms.due(&half),
            Due::Unreadable,
            "a loop carrying part of a schedule cannot say when it is due, and says so",
        );
        assert!(Rhythms.due(&half).owed_on(date(2026, 8, 18)));
    }

    /// **A loop that is declared but never checked in is not late — that is
    /// what `add_entity` plus nothing else leaves behind, the day it is
    /// created and every day after until somebody checks in.**
    ///
    /// A basis that is PRESENT and unreadable is a different fact and stays
    /// loud: `opens_the_loop` already draws this line for `check_in` (a value
    /// to correct rather than one to overrule), and `due` must draw it the
    /// same way, or a garbled basis quietly stops being late.
    ///
    /// Both in one case, for the reason the pairing above gives: either alone
    /// passes against a build that treats every unreadable schedule the same.
    #[test]
    fn a_declared_loop_with_no_check_in_yet_is_not_late_and_a_garbled_basis_still_is() {
        let declared = unopened(AdvancesFrom::CheckInDate);
        assert_eq!(
            Rhythms.due(&declared),
            Due::NotYetOpened,
            "a cadence and a policy with no basis yet is what creating the loop leaves \
             behind, not a defect",
        );
        assert!(
            !Rhythms.due(&declared).owed_on(date(2026, 8, 1)),
            "…so it is not late on the day it was declared",
        );

        let mut garbled = declared.clone();
        garbled.insert(COUNTS_FROM.to_string(), "sometime".to_string());
        assert_eq!(
            Rhythms.due(&garbled),
            Due::Unreadable,
            "a basis that is there and unreadable stays loud, unlike one that was never written",
        );
        assert!(Rhythms.due(&garbled).owed_on(date(2026, 8, 1)));
    }

    /// **A kind no carrier speaks for owes nothing**, which is what this whole
    /// shape is for: a person has no schedule to read, and a check that reads
    /// "cannot compute" as "late" marks every one of them.
    #[test]
    fn a_kind_with_no_carrier_owes_nothing() {
        let carriers: Vec<Box<dyn Carrier>> = shipped();
        let asked: Vec<&dyn Carrier> = carriers.iter().map(AsRef::as_ref).collect();
        assert_eq!(
            owed(&asked, "person", &BTreeMap::new()),
            Due::Never,
            "nobody computes a due moment for a person",
        );
        // The positive in the same read: the carrier that IS there answers.
        let whole = weekly(date(2026, 8, 1), AdvancesFrom::CheckInDate);
        assert!(
            matches!(owed(&asked, "rhythm", &whole), Due::On(_)),
            "and the loop carrier does compute one",
        );
    }
}
