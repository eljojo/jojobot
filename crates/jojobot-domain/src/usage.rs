//! **What each bot's runs add up to, week by week** — the `usage` records.
//!
//! A usage record is computed from run history at the read and is stored nowhere,
//! so it cannot go stale and no write can reach it. One record answers one bot
//! for one ISO week. This module holds the arithmetic and the shape of the
//! record; the reading of runs and the question of who may see a record belong
//! to the callers.
//!
//! **The week of an event comes from the event's own stored date**, never from a
//! clock the reader holds: a run counts in the week of the day it began, and a
//! compaction in the week of the day its history row was written. The reader's
//! day decides only whether the newest week is still open.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::{Date, ISOWeekDate, Weekday};

use crate::memory::search::DocScan;
use crate::memory::{Entity, EntityId, EntityKind};

/// The week, as `YYYY-Www` in ISO 8601 terms.
pub const WEEK: &str = "week";
/// The Monday the week starts on.
pub const FROM: &str = "from";
/// The Sunday the week ends on.
pub const TO: &str = "to";
/// How many runs began in the week.
pub const RUNS: &str = "runs";
/// How many compactions the bot's runs reported in the week.
pub const COMPACTIONS: &str = "compactions";
/// The sum of the totals those compactions set back.
pub const SERVED_AT_COMPACTION: &str = "served_at_compaction";
/// Everything the runs that began in the week were served, over their lives.
pub const SERVED_CHARS: &str = "served_chars";
/// What `served_chars` is counted by. The only value so far is [`RUN_BEGAN`].
pub const SERVED_BASIS: &str = "served_basis";
/// The value of [`SERVED_BASIS`] that says a run's whole life is counted in the
/// week it began.
pub const RUN_BEGAN: &str = "run-began";
/// The earliest day the store dated any change of the bot's runs. A compaction
/// before it left no dated record, so a week before it can read low.
pub const COUNTED_FROM: &str = "counted_from";
/// `true` while the reader's day has not passed the week's last day.
pub const PARTIAL: &str = "partial";

/// How many calls the bot made in the week, refused or not. One key per verb
/// follows, as `calls_<verb>`.
pub const CALLS: &str = "calls";
/// How many of those calls were refused. One key per verb follows, as
/// `refused_<verb>`, and one per word that says who fixes the refusal, as
/// `refused_<verb>_<word>`.
pub const REFUSED: &str = "refused";

/// **How a call ended.** A call that reached a verb either landed, or was
/// refused, and a refusal may name who fixes it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallOutcome {
    /// The verb answered.
    Landed,
    /// The verb, or the gate in front of it, refused. `fix_by` is the word the
    /// refusal carried, and `None` when it carried none.
    Refused {
        /// `retry`, `change` or `person`.
        fix_by: Option<String>,
    },
}

impl CallOutcome {
    /// The two tokens a store keeps: the outcome, and the word that says who
    /// fixes a refusal (empty when there is none).
    pub fn tokens(&self) -> (&'static str, &str) {
        match self {
            CallOutcome::Landed => ("landed", ""),
            CallOutcome::Refused { fix_by } => ("refused", fix_by.as_deref().unwrap_or("")),
        }
    }

    /// The outcome a store's two tokens name, or `None` for a pair no call
    /// makes.
    pub fn from_tokens(outcome: &str, fix_by: &str) -> Option<Self> {
        match (outcome, fix_by) {
            ("landed", "") => Some(CallOutcome::Landed),
            ("refused", word) => Some(CallOutcome::Refused {
                fix_by: (!word.is_empty()).then(|| word.to_string()),
            }),
            _ => None,
        }
    }
}

/// **What one call is counted under**: who made it, the day of the run's own
/// clock, the verb and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CallKey {
    /// The bot behind the call's session. `None` is a call that carried no
    /// session, which belongs to no bot and is counted all the same.
    pub bot: Option<EntityId>,
    /// The day the run was in when the call was answered.
    pub day: Date,
    /// The verb's name. Never an argument or any text the caller sent.
    pub verb: String,
    /// How the call ended.
    pub outcome: CallOutcome,
}

/// How many calls one [`CallKey`] holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallCount {
    /// What the calls are counted under.
    pub key: CallKey,
    /// How many calls.
    pub count: u64,
}

/// One compaction: the total it set back, and when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reset {
    /// The characters the run had been served when the compaction was reported.
    pub total: u64,
    /// When the store recorded the report.
    pub at: Timestamp,
}

/// **What one run contributes**, read off the run history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunUsage {
    /// The bot the run belongs to.
    pub bot: EntityId,
    /// The day the run began: the day it stated, else the day it started on.
    pub began: Date,
    /// The characters the run has been served since its last compaction.
    pub served_chars: u64,
    /// Every compaction the run reported.
    pub resets: Vec<Reset>,
    /// The earliest moment the store dated any change of this run.
    pub dated_from: Option<Timestamp>,
}

/// **What one bot's calls of one verb came to in a week.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerbCalls {
    /// Every call, refused or not.
    pub calls: u64,
    /// The calls that were refused.
    pub refused: u64,
    /// The refused calls by the word that says who fixes them. A refusal that
    /// carried no word is in `refused` and in none of these.
    pub by_word: BTreeMap<String, u64>,
}

/// **One bot's week.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Week {
    /// The bot.
    pub bot: EntityId,
    /// The ISO year the week belongs to, which is not always the calendar year
    /// of its days.
    pub year: i16,
    /// The ISO week number, from one.
    pub number: i8,
    /// The Monday.
    pub from: Date,
    /// The Sunday.
    pub to: Date,
    /// Runs that began in the week.
    pub runs: u64,
    /// Compactions recorded in the week.
    pub compactions: u64,
    /// The sum of the totals those compactions set back.
    pub served_at_compaction: u64,
    /// What the runs that began in the week were served, over their lives.
    pub served_chars: u64,
    /// The earliest day any change of the bot's runs was dated.
    pub counted_from: Option<Date>,
    /// The calls the bot made in the week, by verb.
    pub verbs: BTreeMap<String, VerbCalls>,
    /// Whether the reader's day is still inside the week.
    pub partial: bool,
}

/// The ISO week a day falls in, as its year, number and Monday.
fn iso_week_of(day: Date) -> (i16, i8, Date) {
    let iso = day.iso_week_date();
    let monday = ISOWeekDate::new(iso.year(), iso.week(), Weekday::Monday)
        .expect("a week of a real day has a Monday")
        .date();
    (iso.year(), iso.week(), monday)
}

/// The record of `bot` for the week `day` falls in, made when it is first asked for.
fn week_in<'a>(
    counted: &'a mut BTreeMap<(EntityId, Date), Week>,
    bot: &EntityId,
    day: Date,
    today: Date,
) -> &'a mut Week {
    let (year, number, from) = iso_week_of(day);
    let to = from
        .checked_add(jiff::Span::new().days(6))
        .expect("a Sunday exists");
    counted.entry((bot.clone(), from)).or_insert_with(|| Week {
        bot: bot.clone(),
        year,
        number,
        from,
        to,
        runs: 0,
        compactions: 0,
        served_at_compaction: 0,
        served_chars: 0,
        counted_from: None,
        verbs: BTreeMap::new(),
        partial: today <= to,
    })
}

/// Every bot's weeks, newest first, and within a week by bot. A week in which a
/// bot began no run, reported no compaction and made no call has no record.
///
/// The week of a call comes from the day it was counted under, which the run's
/// own clock named when the call was answered. A call that belongs to no bot has
/// no record to join.
///
/// `day_of` turns a recorded moment into a day in the frame the reader asked in.
/// `today` is the reader's own day.
pub fn weeks(
    runs: &[RunUsage],
    calls: &[CallCount],
    day_of: &dyn Fn(Timestamp) -> Date,
    today: Date,
) -> Vec<Week> {
    let mut counted: BTreeMap<(EntityId, Date), Week> = BTreeMap::new();
    let mut dated_from: BTreeMap<&EntityId, Date> = BTreeMap::new();
    for run in runs {
        let lifetime = run.served_chars + run.resets.iter().map(|reset| reset.total).sum::<u64>();
        let began = week_in(&mut counted, &run.bot, run.began, today);
        began.runs += 1;
        began.served_chars += lifetime;
        for reset in &run.resets {
            let compacted = week_in(&mut counted, &run.bot, day_of(reset.at), today);
            compacted.compactions += 1;
            compacted.served_at_compaction += reset.total;
        }
        if let Some(first) = run.dated_from.map(day_of) {
            let held = dated_from.entry(&run.bot).or_insert(first);
            *held = (*held).min(first);
        }
    }
    for held in calls {
        let Some(bot) = &held.key.bot else {
            continue;
        };
        let week = week_in(&mut counted, bot, held.key.day, today);
        let verb = week.verbs.entry(held.key.verb.clone()).or_default();
        verb.calls += held.count;
        if let CallOutcome::Refused { fix_by } = &held.key.outcome {
            verb.refused += held.count;
            if let Some(word) = fix_by {
                *verb.by_word.entry(word.clone()).or_default() += held.count;
            }
        }
    }
    let mut all: Vec<Week> = counted.into_values().collect();
    for week in &mut all {
        week.counted_from = dated_from.get(&week.bot).copied();
    }
    all.sort_by(|a, b| b.from.cmp(&a.from).then_with(|| a.bot.cmp(&b.bot)));
    all
}

impl Week {
    /// **The call counts as fields.** The two totals are there once the bot made
    /// a call in the week; a verb the bot never called has no key, and neither
    /// has a word that never fixed one of its refusals.
    fn call_fields(&self) -> Vec<(String, String)> {
        if self.verbs.is_empty() {
            return Vec::new();
        }
        let total: u64 = self.verbs.values().map(|verb| verb.calls).sum();
        let refused: u64 = self.verbs.values().map(|verb| verb.refused).sum();
        let mut fields = vec![
            (CALLS.to_string(), total.to_string()),
            (REFUSED.to_string(), refused.to_string()),
        ];
        for (name, verb) in &self.verbs {
            fields.push((format!("{CALLS}_{name}"), verb.calls.to_string()));
            if verb.refused > 0 {
                fields.push((format!("{REFUSED}_{name}"), verb.refused.to_string()));
            }
            for (word, count) in &verb.by_word {
                fields.push((format!("{REFUSED}_{name}_{word}"), count.to_string()));
            }
        }
        fields
    }

    /// The handle the record answers to: `usage:<bot>-<year>-w<week>`.
    pub fn handle(&self) -> EntityId {
        EntityId::new(
            EntityKind::USAGE,
            format!("{}-{}-w{:02}", self.bot.slug(), self.year, self.number),
        )
    }

    /// The record as a document the query can select: owned by its bot, a child
    /// of it, holding the numbers as fields.
    pub fn projected(&self) -> DocScan {
        let id = self.handle();
        let fields: BTreeMap<String, String> = [
            (WEEK, format!("{}-W{:02}", self.year, self.number)),
            (FROM, self.from.to_string()),
            (TO, self.to.to_string()),
            (RUNS, self.runs.to_string()),
            (COMPACTIONS, self.compactions.to_string()),
            (SERVED_AT_COMPACTION, self.served_at_compaction.to_string()),
            (SERVED_CHARS, self.served_chars.to_string()),
            (SERVED_BASIS, RUN_BEGAN.to_string()),
            (PARTIAL, self.partial.to_string()),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .chain(
            self.counted_from
                .map(|day| (COUNTED_FROM.to_string(), day.to_string())),
        )
        .chain(self.call_fields())
        .collect();
        DocScan {
            doc_id: id.as_str().to_string(),
            title: format!("{} {}-W{:02}", self.bot, self.year, self.number),
            prose: String::new(),
            entity: Some(Entity {
                folded_names: Vec::new(),
                name: format!("{} usage, week {}-W{:02}", self.bot, self.year, self.number),
                id,
                kind: EntityKind::USAGE,
                aliases: Vec::new(),
                source: "jojobot".to_string(),
                crm: None,
                // **The bot owns its weeks**, so the tree already says whose
                // they are and no second copy has to be kept in step.
                parent: Some(self.bot.clone()),
                boot: Default::default(),
                merged_into: None,
                badge: None,
                archived: None,
            }),
            facts: Vec::new(),
            fields,
            owner: Some(self.bot.clone()),
            unmapped: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Date {
        text.parse().expect("a test day")
    }

    fn at(text: &str) -> Timestamp {
        format!("{text}T12:00:00Z").parse().expect("a test moment")
    }

    fn in_utc(moment: Timestamp) -> Date {
        moment.to_zoned(jiff::tz::TimeZone::UTC).date()
    }

    fn run(bot: &str, began: &str, served: u64, resets: &[(u64, &str)]) -> RunUsage {
        RunUsage {
            bot: EntityId(format!("bot:{bot}")),
            began: day(began),
            served_chars: served,
            resets: resets
                .iter()
                .map(|(total, when)| Reset {
                    total: *total,
                    at: at(when),
                })
                .collect(),
            dated_from: resets.first().map(|(_, when)| at(when)),
        }
    }

    fn week_of<'a>(all: &'a [Week], bot: &str, from: &str) -> &'a Week {
        all.iter()
            .find(|w| w.bot.as_str() == format!("bot:{bot}") && w.from == day(from))
            .unwrap_or_else(|| panic!("no week of {bot} from {from}: {all:?}"))
    }

    /// A run counts in the week it began and a compaction in the week it was
    /// recorded, and the two can differ for one run.
    #[test]
    fn a_run_and_its_compaction_count_in_their_own_weeks() {
        // Monday 2026-10-05 starts ISO week 41; Monday 2026-10-12 starts week 42.
        let runs = [run("bart", "2026-10-07", 40, &[(100, "2026-10-14")])];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-20"));
        let began = week_of(&all, "bart", "2026-10-05");
        assert_eq!((began.number, began.runs, began.compactions), (41, 1, 0));
        let compacted = week_of(&all, "bart", "2026-10-12");
        assert_eq!(
            (compacted.number, compacted.runs, compacted.compactions),
            (42, 0, 1)
        );
        assert_eq!(compacted.served_at_compaction, 100);
        assert_eq!(all.len(), 2, "a week nothing happened in has no record");
    }

    /// What a run was served is its current total plus every total a compaction
    /// set back, counted in the week it began.
    #[test]
    fn served_is_the_whole_life_of_a_run_counted_where_it_began() {
        let runs = [run(
            "bart",
            "2026-10-07",
            40,
            &[(100, "2026-10-08"), (30, "2026-10-14")],
        )];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-20"));
        assert_eq!(week_of(&all, "bart", "2026-10-05").served_chars, 170);
        assert_eq!(week_of(&all, "bart", "2026-10-12").served_chars, 0);
    }

    /// Several runs of several bots add up per bot and per week.
    #[test]
    fn bots_and_runs_add_up_separately() {
        let runs = [
            run("bart", "2026-10-06", 10, &[]),
            run("bart", "2026-10-09", 20, &[(5, "2026-10-09")]),
            run("marge", "2026-10-06", 7, &[]),
        ];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-20"));
        let bart = week_of(&all, "bart", "2026-10-05");
        assert_eq!((bart.runs, bart.compactions, bart.served_chars), (2, 1, 35));
        let marge = week_of(&all, "marge", "2026-10-05");
        assert_eq!(
            (marge.runs, marge.compactions, marge.served_chars),
            (1, 0, 7)
        );
    }

    /// The week belongs to the ISO year, not the calendar year: the first days
    /// of January 2027 are in week 53 of 2026.
    #[test]
    fn the_week_follows_the_iso_year() {
        let runs = [run("bart", "2027-01-02", 1, &[])];
        let all = weeks(&runs, &[], &in_utc, day("2027-01-20"));
        let w = week_of(&all, "bart", "2026-12-28");
        assert_eq!((w.year, w.number), (2026, 53));
        assert_eq!(w.to, day("2027-01-03"));
        assert_eq!(w.handle().as_str(), "usage:bart-2026-w53");
    }

    /// A week is partial until the reader's day has passed its Sunday, and the
    /// reader's day is the only thing that decides it.
    #[test]
    fn the_open_week_is_partial_against_the_readers_day() {
        let runs = [run("bart", "2026-10-07", 1, &[])];
        let on = |today: &str| weeks(&runs, &[], &in_utc, day(today))[0].partial;
        assert!(on("2026-10-07"), "inside the week");
        assert!(on("2026-10-11"), "on its Sunday the week is still open");
        assert!(!on("2026-10-12"), "the Monday after closes it");
        assert!(on("2026-10-01"), "a reader before the week sees it open");
    }

    /// The first day the store dated any change of the bot's runs is named, so
    /// a reader knows from when the counts can be trusted.
    #[test]
    fn counted_from_is_the_earliest_dated_change_of_the_bot() {
        let mut early = run("bart", "2026-10-06", 1, &[]);
        early.dated_from = Some(at("2026-10-03"));
        let late = run("bart", "2026-10-13", 1, &[(9, "2026-10-14")]);
        let undated = RunUsage {
            dated_from: None,
            ..run("marge", "2026-10-06", 1, &[])
        };
        let all = weeks(&[early, late, undated], &[], &in_utc, day("2026-10-20"));
        assert_eq!(
            week_of(&all, "bart", "2026-10-12").counted_from,
            Some(day("2026-10-03"))
        );
        assert_eq!(week_of(&all, "marge", "2026-10-05").counted_from, None);
    }

    /// Newest week first, and the bots of one week in handle order.
    #[test]
    fn weeks_come_newest_first_then_by_bot() {
        let runs = [
            run("marge", "2026-10-06", 1, &[]),
            run("bart", "2026-10-13", 1, &[]),
            run("bart", "2026-10-06", 1, &[]),
        ];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-20"));
        let order: Vec<_> = all
            .iter()
            .map(|w| (w.from.to_string(), w.bot.as_str().to_string()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("2026-10-12".to_string(), "bot:bart".to_string()),
                ("2026-10-05".to_string(), "bot:bart".to_string()),
                ("2026-10-05".to_string(), "bot:marge".to_string()),
            ]
        );
    }

    /// The record reads as an object of its own kind, a child of its bot, owned
    /// by that bot, holding every number as a field the query can filter on.
    #[test]
    fn the_projection_is_an_object_of_the_bot() {
        let runs = [run("bart", "2026-10-07", 40, &[(100, "2026-10-14")])];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-14"));
        let doc = week_of(&all, "bart", "2026-10-12").projected();
        let entity = doc.entity.as_ref().expect("a usage record is an entity");
        assert_eq!(entity.id.as_str(), "usage:bart-2026-w42");
        assert_eq!(entity.kind, EntityKind::USAGE);
        assert_eq!(
            entity.parent.as_ref().map(EntityId::as_str),
            Some("bot:bart")
        );
        assert_eq!(doc.owner.as_ref().map(EntityId::as_str), Some("bot:bart"));
        let field = |key: &str| doc.fields.get(key).map(String::as_str);
        assert_eq!(field(WEEK), Some("2026-W42"));
        assert_eq!(field(FROM), Some("2026-10-12"));
        assert_eq!(field(TO), Some("2026-10-18"));
        assert_eq!(field(RUNS), Some("0"));
        assert_eq!(field(COMPACTIONS), Some("1"));
        assert_eq!(field(SERVED_AT_COMPACTION), Some("100"));
        assert_eq!(field(SERVED_CHARS), Some("0"));
        assert_eq!(field(SERVED_BASIS), Some(RUN_BEGAN));
        assert_eq!(field(COUNTED_FROM), Some("2026-10-14"));
        assert_eq!(field(PARTIAL), Some("true"));
        assert!(doc.facts.is_empty(), "a computed record holds no claims");
    }

    fn counted(
        bot: Option<&str>,
        on: &str,
        verb: &str,
        outcome: CallOutcome,
        count: u64,
    ) -> CallCount {
        CallCount {
            key: CallKey {
                bot: bot.map(|bot| EntityId(format!("bot:{bot}"))),
                day: day(on),
                verb: verb.to_string(),
                outcome,
            },
            count,
        }
    }

    fn refused(word: Option<&str>) -> CallOutcome {
        CallOutcome::Refused {
            fix_by: word.map(str::to_string),
        }
    }

    /// A call counts in the week of the day it was counted under, for the bot
    /// that made it, and a bot with no run that week still has the record.
    #[test]
    fn calls_count_in_the_week_of_their_day_for_their_bot() {
        let calls = [
            counted(Some("bart"), "2026-10-11", "recall", CallOutcome::Landed, 2),
            counted(Some("bart"), "2026-10-12", "recall", CallOutcome::Landed, 5),
            counted(
                Some("marge"),
                "2026-10-12",
                "recall",
                CallOutcome::Landed,
                1,
            ),
        ];
        let all = weeks(&[], &calls, &in_utc, day("2026-10-20"));
        let calls_of = |bot: &str, from: &str| week_of(&all, bot, from).verbs["recall"].calls;
        assert_eq!(calls_of("bart", "2026-10-05"), 2, "Sunday is week 41");
        assert_eq!(calls_of("bart", "2026-10-12"), 5, "Monday is week 42");
        assert_eq!(calls_of("marge", "2026-10-12"), 1);
        assert_eq!(all.len(), 3);
    }

    /// A refusal counts as a call and as a refusal, under the word that says who
    /// fixes it; a refusal with no word is counted in no word.
    #[test]
    fn a_refusal_is_a_call_and_a_refusal_under_its_word() {
        let calls = [
            counted(
                Some("bart"),
                "2026-10-14",
                "capture",
                CallOutcome::Landed,
                4,
            ),
            counted(
                Some("bart"),
                "2026-10-14",
                "capture",
                refused(Some("change")),
                2,
            ),
            counted(
                Some("bart"),
                "2026-10-15",
                "capture",
                refused(Some("retry")),
                1,
            ),
            counted(Some("bart"), "2026-10-15", "capture", refused(None), 3),
        ];
        let all = weeks(&[], &calls, &in_utc, day("2026-10-20"));
        let capture = &week_of(&all, "bart", "2026-10-12").verbs["capture"];
        assert_eq!((capture.calls, capture.refused), (10, 6));
        assert_eq!(capture.by_word["change"], 2);
        assert_eq!(capture.by_word["retry"], 1);
        assert!(!capture.by_word.contains_key("person"));
    }

    /// A call that carried no session belongs to no bot, so it opens no record.
    #[test]
    fn a_call_with_no_bot_joins_no_week() {
        let calls = [counted(
            None,
            "2026-10-14",
            "start_here",
            CallOutcome::Landed,
            9,
        )];
        assert!(weeks(&[], &calls, &in_utc, day("2026-10-20")).is_empty());
    }

    /// The counts are fields the query can filter on: totals once the bot made a
    /// call, a key per verb called, and a key per word that fixed a refusal.
    #[test]
    fn the_call_counts_are_fields_of_the_record() {
        let calls = [
            counted(Some("bart"), "2026-10-14", "recall", CallOutcome::Landed, 1),
            counted(
                Some("bart"),
                "2026-10-14",
                "capture",
                CallOutcome::Landed,
                1,
            ),
            counted(
                Some("bart"),
                "2026-10-14",
                "capture",
                refused(Some("change")),
                1,
            ),
        ];
        let all = weeks(&[], &calls, &in_utc, day("2026-10-14"));
        let doc = week_of(&all, "bart", "2026-10-12").projected();
        let field = |key: &str| doc.fields.get(key).map(String::as_str);
        assert_eq!(field(CALLS), Some("3"));
        assert_eq!(field(REFUSED), Some("1"));
        assert_eq!(field("calls_recall"), Some("1"));
        assert_eq!(field("calls_capture"), Some("2"));
        assert_eq!(field("refused_capture"), Some("1"));
        assert_eq!(field("refused_capture_change"), Some("1"));
        assert_eq!(field("refused_recall"), None);
        assert_eq!(field("refused_capture_retry"), None);
        assert_eq!(field("calls_journal"), None);
    }

    /// A week the bot made no call in holds no call key at all, so a reader
    /// cannot take a missing count for a count of none.
    #[test]
    fn a_week_with_no_calls_carries_no_call_fields() {
        let runs = [run("bart", "2026-10-07", 40, &[])];
        let all = weeks(&runs, &[], &in_utc, day("2026-10-14"));
        let doc = week_of(&all, "bart", "2026-10-05").projected();
        assert!(doc.fields.keys().all(|key| !key.starts_with("calls")));
        assert!(!doc.fields.contains_key(REFUSED));
    }

    /// The two tokens a store keeps read back as the outcome they came from, and
    /// a pair no call makes reads as nothing.
    #[test]
    fn an_outcome_survives_its_tokens() {
        for outcome in [CallOutcome::Landed, refused(Some("person")), refused(None)] {
            let (kept, word) = outcome.tokens();
            assert_eq!(CallOutcome::from_tokens(kept, word), Some(outcome));
        }
        assert_eq!(CallOutcome::from_tokens("landed", "retry"), None);
        assert_eq!(CallOutcome::from_tokens("lost", ""), None);
    }
}
