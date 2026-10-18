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
        partial: today <= to,
    })
}

/// Every bot's weeks, newest first, and within a week by bot. A week in which a
/// bot began no run and reported no compaction has no record.
///
/// `day_of` turns a recorded moment into a day in the frame the reader asked in.
/// `today` is the reader's own day.
pub fn weeks(runs: &[RunUsage], day_of: &dyn Fn(Timestamp) -> Date, today: Date) -> Vec<Week> {
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
    let mut all: Vec<Week> = counted.into_values().collect();
    for week in &mut all {
        week.counted_from = dated_from.get(&week.bot).copied();
    }
    all.sort_by(|a, b| b.from.cmp(&a.from).then_with(|| a.bot.cmp(&b.bot)));
    all
}

impl Week {
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
        let all = weeks(&runs, &in_utc, day("2026-10-20"));
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
        let all = weeks(&runs, &in_utc, day("2026-10-20"));
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
        let all = weeks(&runs, &in_utc, day("2026-10-20"));
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
        let all = weeks(&runs, &in_utc, day("2027-01-20"));
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
        let on = |today: &str| weeks(&runs, &in_utc, day(today))[0].partial;
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
        let all = weeks(&[early, late, undated], &in_utc, day("2026-10-20"));
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
        let all = weeks(&runs, &in_utc, day("2026-10-20"));
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
        let all = weeks(&runs, &in_utc, day("2026-10-14"));
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
}
