//! **A recurrence rule: "the 1st of every month", "every other Thursday".**
//!
//! A rule is an RFC 5545 `RRULE` value, parsed and expanded by the `rrule`
//! crate, so no date arithmetic of the calendar lives here. This module only
//! narrows what a rhythm accepts to what a rhythm can use, and turns the
//! crate's answer into a civil date.
//!
//! **Dates only.** A rhythm falls due on a day, so a rule that names a time of
//! day, a count, an end or a frequency finer than a day is refused with the
//! key that says it better. **The rule is anchored once**: the crate expands a
//! rule from a fixed start, and that start is what keeps "every other
//! Thursday" on the same Thursdays. Re-anchoring it to each cycle would walk
//! the phase by a week after one late check-in.

use jiff::civil::Date;
use rrule::RRuleSet;

/// **Why a rule is not one a rhythm can carry**, in words that name the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleRefused {
    /// The text is not an `RRULE` value.
    Unparseable { reason: String },
    /// A frequency finer than a day: a rhythm falls due on a day.
    FinerThanADay { freq: String },
    /// A part that ends the rule, which a rhythm ends by being archived instead.
    Ends { part: &'static str },
    /// A time of day, or a calendar property around the rule.
    NotADate { part: String },
    /// A rule no day satisfies, such as 30 February, or that no day on or after
    /// the day it runs from satisfies, such as every other 29 February from an
    /// odd year. `from` is that day, and is `None` for a rule asked about before
    /// the loop has a day to run from.
    NeverOccurs { from: Option<Date> },
}

impl std::fmt::Display for RuleRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleRefused::Unparseable { reason } => write!(
                f,
                "that is not an RFC 5545 recurrence rule ({reason}). Send a value such as \
                 FREQ=MONTHLY;BYMONTHDAY=1"
            ),
            RuleRefused::FinerThanADay { freq } => write!(
                f,
                "FREQ={freq} is finer than a day, and a rhythm falls due on a day. Use \
                 FREQ=DAILY or a coarser frequency"
            ),
            RuleRefused::Ends { part } => write!(
                f,
                "{part} ends the rule. Take {part} off the rule and archive the rhythm with \
                 archive_entity when the loop is over"
            ),
            RuleRefused::NotADate { part } => write!(
                f,
                "{part} names a time of day or a calendar property, and a rhythm is scheduled by \
                 date alone. Send only the FREQ rule and its day parts"
            ),
            RuleRefused::NeverOccurs { from: Some(from) } => write!(
                f,
                "no day on or after {from}, the day the loop counts from, satisfies that rule, \
                 so the rhythm would never fall due. Check its BYMONTH, BYMONTHDAY and INTERVAL \
                 parts, or the day it counts from"
            ),
            RuleRefused::NeverOccurs { from: None } => write!(
                f,
                "no day ever satisfies that rule, so the rhythm would never fall due. Check its \
                 BYMONTH and BYMONTHDAY parts"
            ),
        }
    }
}

/// The day a question about a rule alone is asked from. A rule's words and its
/// acceptance do not depend on the anchor.
const UNANCHORED: Date = Date::constant(2000, 1, 3);

/// **The words of a rule made of FREQ, INTERVAL, BYMONTHDAY and BYDAY**, or
/// nothing when it holds any other part, a value these words do not cover, both
/// a day of the month and a day of the week, or a day list under a frequency
/// that is not its own (days of the month under MONTHLY, days of the week under
/// WEEKLY).
fn words_of(rule: &str) -> Option<String> {
    let (mut unit, mut interval) = (None, 1u32);
    let (mut monthdays, mut weekdays): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for part in rule.split(';') {
        let (name, value) = part.split_once('=')?;
        let value = value.trim();
        match name.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                unit = Some(match value.to_ascii_uppercase().as_str() {
                    "DAILY" => "day",
                    "WEEKLY" => "week",
                    "MONTHLY" => "month",
                    "YEARLY" => "year",
                    _ => return None,
                });
            }
            "INTERVAL" => interval = value.parse().ok().filter(|n| *n >= 1)?,
            "BYMONTHDAY" => {
                monthdays = value
                    .split(',')
                    .map(|day| match day.trim().parse::<i32>().ok()? {
                        -1 => Some("the last day".to_string()),
                        n @ 1..=31 => Some(format!("day {n}")),
                        _ => None,
                    })
                    .collect::<Option<_>>()?;
            }
            "BYDAY" => {
                weekdays = value
                    .split(',')
                    .map(|day| {
                        let name = match day.trim().to_ascii_uppercase().as_str() {
                            "MO" => "Monday",
                            "TU" => "Tuesday",
                            "WE" => "Wednesday",
                            "TH" => "Thursday",
                            "FR" => "Friday",
                            "SA" => "Saturday",
                            "SU" => "Sunday",
                            _ => return None,
                        };
                        Some(name.to_string())
                    })
                    .collect::<Option<_>>()?;
            }
            _ => return None,
        }
    }
    let unit = unit?;
    let every = if interval == 1 {
        format!("every {unit}")
    } else {
        format!("every {interval} {unit}s")
    };
    // **A day list is worded only under the frequency it reads as spoken.**
    // Under any other frequency RFC 5545 expands or limits it differently
    // (BYDAY under MONTHLY is every such weekday of the month, BYMONTHDAY under
    // YEARLY is that day of every month, under DAILY it limits the days), and
    // these words would say something the rule does not, so those read as the
    // rule itself.
    match (monthdays.is_empty(), weekdays.is_empty(), unit) {
        (false, true, "month") => Some(format!("on {} of {every}", joined(&monthdays))),
        (true, false, "week") => Some(format!("{every} on {}", joined(&weekdays))),
        (true, true, _) => Some(every),
        _ => None,
    }
}

/// "a", "a and b", "a, b and c".
fn joined(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// **A rule a rhythm can carry, anchored to the day it was opened.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    /// The rule as sent, without a leading `RRULE:`.
    rule: String,
    /// The day the rule's calendar starts on, which fixes the phase of an
    /// `INTERVAL`.
    anchor: Date,
}

impl Recurrence {
    /// **Parse a rule and anchor it.** Refused when it is not a rule a rhythm
    /// can carry; see [`RuleRefused`].
    pub fn new(rule: &str, anchor: Date) -> Result<Recurrence, RuleRefused> {
        let rule = rule.trim();
        let rule = rule
            .strip_prefix("RRULE:")
            .or_else(|| rule.strip_prefix("rrule:"))
            .unwrap_or(rule)
            .trim()
            .to_string();
        let upper = rule.to_ascii_uppercase();
        if rule.contains('\n')
            || upper.contains("DTSTART")
            || upper.contains("EXDATE")
            || upper.contains("RDATE")
            || upper.contains("EXRULE")
        {
            return Err(RuleRefused::NotADate {
                part: "DTSTART, RDATE, EXDATE or a second line".to_string(),
            });
        }
        for part in upper.split(';') {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            match name.trim() {
                "COUNT" => return Err(RuleRefused::Ends { part: "COUNT" }),
                "UNTIL" => return Err(RuleRefused::Ends { part: "UNTIL" }),
                "BYHOUR" | "BYMINUTE" | "BYSECOND" => {
                    return Err(RuleRefused::NotADate {
                        part: name.trim().to_string(),
                    });
                }
                "FREQ" if matches!(value.trim(), "HOURLY" | "MINUTELY" | "SECONDLY") => {
                    return Err(RuleRefused::FinerThanADay {
                        freq: value.trim().to_string(),
                    });
                }
                _ => {}
            }
        }
        let held = Recurrence { rule, anchor };
        let set = held
            .set()
            .map_err(|reason| RuleRefused::Unparseable { reason })?;
        // One expansion proves the rule can be asked at all and that some day
        // satisfies it. A rule with no day inside the crate's search limit is
        // refused here and not left to fall due never.
        let probe = set.after(held.start_of(anchor)).all(1);
        if probe.dates.is_empty() {
            return Err(RuleRefused::NeverOccurs { from: Some(anchor) });
        }
        Ok(held)
    }

    /// **Whether a rule would be accepted**, for a write that has no anchor yet.
    pub fn check(rule: &str) -> Result<(), RuleRefused> {
        Recurrence::new(rule, UNANCHORED)
            .map(|_| ())
            .map_err(|refused| match refused {
                // The fixed day is not the loop's, so the refusal does not name it.
                RuleRefused::NeverOccurs { .. } => RuleRefused::NeverOccurs { from: None },
                other => other,
            })
    }

    /// **A rule in words**, for a page: "on day 1 of every month". Refused as
    /// [`Recurrence::check`] refuses it, so a rule a rhythm cannot carry has no
    /// words. A rule with a part the words do not cover reads as the rule itself.
    pub fn describe(rule: &str) -> Result<String, RuleRefused> {
        let held = Recurrence::new(rule, UNANCHORED)?;
        Ok(words_of(&held.rule).unwrap_or_else(|| format!("by the rule {}", held.rule)))
    }

    /// The first day the rule occurs on that is strictly after `day`.
    pub fn first_after(&self, day: Date) -> Option<Date> {
        let set = self.set().ok()?;
        let after = set.after(self.start_of(day.checked_add(jiff::Span::new().days(1)).ok()?));
        let found = after.all(1);
        let at = found.dates.first()?;
        use chrono::Datelike;
        Date::new(at.year() as i16, at.month() as i8, at.day() as i8).ok()
    }

    /// The anchor day.
    pub fn anchor(&self) -> Date {
        self.anchor
    }

    /// The rule as held.
    pub fn rule(&self) -> &str {
        &self.rule
    }

    fn start_of(&self, day: Date) -> chrono::DateTime<rrule::Tz> {
        use chrono::TimeZone;
        rrule::Tz::UTC
            .with_ymd_and_hms(
                i32::from(day.year()),
                day.month() as u32,
                day.day() as u32,
                0,
                0,
                0,
            )
            .single()
            .expect("midnight UTC exists on every day")
    }

    fn set(&self) -> Result<RRuleSet, String> {
        let a = self.anchor;
        let text = format!(
            "DTSTART:{:04}{:02}{:02}T000000Z\nRRULE:{}",
            a.year(),
            a.month(),
            a.day(),
            self.rule
        );
        text.parse::<RRuleSet>().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    /// **A rule reads as words a person would say**, each frequency, an interval,
    /// the days of a month and the days of a week. A rule with a part the words do
    /// not cover reads as the rule itself, and a rule a rhythm cannot carry has no
    /// words.
    #[test]
    fn a_rule_reads_as_words_and_an_uncovered_part_reads_as_the_rule() {
        for (rule, said) in [
            ("FREQ=MONTHLY;BYMONTHDAY=1", "on day 1 of every month"),
            (
                "RRULE:FREQ=MONTHLY;BYMONTHDAY=-1",
                "on the last day of every month",
            ),
            (
                "FREQ=MONTHLY;BYMONTHDAY=1,15",
                "on day 1 and day 15 of every month",
            ),
            ("FREQ=DAILY;INTERVAL=3", "every 3 days"),
            ("FREQ=WEEKLY;BYDAY=TH", "every week on Thursday"),
            (
                "freq=weekly;interval=2;byday=mo,th",
                "every 2 weeks on Monday and Thursday",
            ),
            (
                "FREQ=WEEKLY;BYDAY=MO,WE,FR",
                "every week on Monday, Wednesday and Friday",
            ),
            ("FREQ=YEARLY", "every year"),
            (
                "FREQ=MONTHLY;BYSETPOS=1;BYDAY=MO",
                "by the rule FREQ=MONTHLY;BYSETPOS=1;BYDAY=MO",
            ),
            (
                "FREQ=MONTHLY;BYMONTHDAY=1;BYDAY=MO",
                "by the rule FREQ=MONTHLY;BYMONTHDAY=1;BYDAY=MO",
            ),
            // A day list under another frequency reads differently in RFC 5545
            // (every Monday of the month; the 15th of every month of the year;
            // the 1st of each month out of every day), so the words never pair
            // a frequency with the part that is not its own.
            ("FREQ=MONTHLY;BYDAY=MO", "by the rule FREQ=MONTHLY;BYDAY=MO"),
            (
                "FREQ=YEARLY;BYMONTHDAY=15",
                "by the rule FREQ=YEARLY;BYMONTHDAY=15",
            ),
            (
                "FREQ=DAILY;BYMONTHDAY=1",
                "by the rule FREQ=DAILY;BYMONTHDAY=1",
            ),
        ] {
            assert_eq!(Recurrence::describe(rule).as_deref(), Ok(said), "{rule}");
        }
        for rule in ["FREQ=HOURLY", "FREQ=DAILY;COUNT=3", "often"] {
            assert!(Recurrence::describe(rule).is_err(), "{rule}");
        }
        // The values the rule parser itself refuses are not reachable through
        // `describe`, so the words are asked directly.
        for rule in [
            "FREQ=DAILY;INTERVAL=0",
            "FREQ=MONTHLY;BYMONTHDAY=0",
            "FREQ=MONTHLY;BYMONTHDAY=40",
            "FREQ=WEEKLY;BYDAY=1TH",
            "FREQ=SOMETIMES",
            "INTERVAL=2",
            "FREQ",
        ] {
            assert_eq!(words_of(rule), None, "{rule}");
        }
    }

    /// The next `n` occurrences of `rule`, each strictly after the last, from a
    /// rule anchored on `anchor` and asked after `from`.
    fn walk(rule: &str, anchor: Date, from: Date, n: usize) -> Vec<Date> {
        let held = Recurrence::new(rule, anchor).expect("the rule is accepted");
        let mut found = Vec::new();
        let mut at = from;
        for _ in 0..n {
            at = held.first_after(at).expect("another occurrence");
            found.push(at);
        }
        found
    }

    #[test]
    fn the_first_of_every_month_falls_on_each_first() {
        assert_eq!(
            walk(
                "FREQ=MONTHLY;BYMONTHDAY=1",
                date(2026, 1, 1),
                date(2026, 1, 1),
                3
            ),
            [date(2026, 2, 1), date(2026, 3, 1), date(2026, 4, 1)]
        );
    }

    #[test]
    fn the_last_day_of_the_month_is_found_by_setpos_and_by_a_negative_day() {
        let by_setpos = walk(
            "FREQ=MONTHLY;BYMONTHDAY=28,29,30,31;BYSETPOS=-1",
            date(2028, 1, 1),
            date(2028, 1, 1),
            4,
        );
        assert_eq!(
            by_setpos,
            [
                date(2028, 1, 31),
                date(2028, 2, 29),
                date(2028, 3, 31),
                date(2028, 4, 30)
            ]
        );
        let negative = walk(
            "FREQ=MONTHLY;BYMONTHDAY=-1",
            date(2027, 1, 1),
            date(2027, 1, 1),
            3,
        );
        assert_eq!(
            negative,
            [date(2027, 1, 31), date(2027, 2, 28), date(2027, 3, 31)]
        );
    }

    #[test]
    fn the_first_saturday_and_the_last_friday_of_the_month() {
        assert_eq!(
            walk(
                "FREQ=MONTHLY;BYDAY=1SA",
                date(2026, 1, 1),
                date(2026, 1, 1),
                2
            ),
            [date(2026, 1, 3), date(2026, 2, 7)]
        );
        assert_eq!(
            walk(
                "FREQ=MONTHLY;BYDAY=-1FR",
                date(2026, 1, 1),
                date(2026, 1, 1),
                2
            ),
            [date(2026, 1, 30), date(2026, 2, 27)]
        );
    }

    #[test]
    fn a_fortnightly_thursday_keeps_its_phase_from_the_anchor_alone() {
        let held =
            Recurrence::new("FREQ=WEEKLY;INTERVAL=2;BYDAY=TH", date(2026, 1, 1)).expect("accepted");
        // Anchored on Thursday 2026-01-01, the Thursdays are 1 Jan, 15 Jan, 29 Jan.
        assert_eq!(held.first_after(date(2026, 1, 1)), Some(date(2026, 1, 15)));
        // Asked from a day that is NOT the previous occurrence (a late check-in),
        // the answer is still on the anchor's fortnights, never a week off.
        assert_eq!(held.first_after(date(2026, 1, 20)), Some(date(2026, 1, 29)));
        assert_eq!(held.first_after(date(2026, 1, 8)), Some(date(2026, 1, 15)));
    }

    #[test]
    fn every_march_first() {
        assert_eq!(
            walk(
                "FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=1",
                date(2026, 1, 1),
                date(2026, 1, 1),
                2
            ),
            [date(2026, 3, 1), date(2027, 3, 1)]
        );
    }

    #[test]
    fn a_bare_day_31_skips_the_months_without_one() {
        assert_eq!(
            walk(
                "FREQ=MONTHLY;BYMONTHDAY=31",
                date(2026, 1, 1),
                date(2026, 1, 1),
                3
            ),
            [date(2026, 1, 31), date(2026, 3, 31), date(2026, 5, 31)]
        );
    }

    #[test]
    fn a_rule_a_rhythm_cannot_carry_is_refused_naming_the_part() {
        let refused = |rule: &str| Recurrence::new(rule, date(2026, 1, 1)).expect_err(rule);
        assert!(matches!(
            refused("FREQ=HOURLY"),
            RuleRefused::FinerThanADay { .. }
        ));
        assert!(matches!(
            refused("FREQ=DAILY;COUNT=3"),
            RuleRefused::Ends { part: "COUNT" }
        ));
        assert!(matches!(
            refused("FREQ=DAILY;UNTIL=20270101"),
            RuleRefused::Ends { part: "UNTIL" }
        ));
        assert!(matches!(
            refused("FREQ=DAILY;BYHOUR=9"),
            RuleRefused::NotADate { .. }
        ));
        assert!(matches!(
            refused("DTSTART:20260101T000000Z\nFREQ=DAILY"),
            RuleRefused::NotADate { .. }
        ));
        assert!(matches!(refused("often"), RuleRefused::Unparseable { .. }));
        assert!(matches!(
            refused("FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30"),
            RuleRefused::NeverOccurs { from: Some(_) }
        ));
        assert!(Recurrence::check("RRULE:FREQ=WEEKLY;BYDAY=MO").is_ok());
    }
}
