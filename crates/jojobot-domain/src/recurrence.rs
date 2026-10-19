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
    /// A part that ends the rule, which a rhythm ends with `ended` instead.
    Ends { part: &'static str },
    /// A time of day, or a calendar property around the rule.
    NotADate { part: String },
    /// A rule no day ever satisfies, such as 30 February.
    NeverOccurs,
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
                "{part} ends the rule, and a rhythm ends with `ended`. Take {part} off the rule \
                 and send `ended` when the loop is over"
            ),
            RuleRefused::NotADate { part } => write!(
                f,
                "{part} names a time of day or a calendar property, and a rhythm is scheduled by \
                 date alone. Send only the FREQ rule and its day parts"
            ),
            RuleRefused::NeverOccurs => write!(
                f,
                "no day ever satisfies that rule, so the rhythm would never fall due. Check its \
                 BYMONTH and BYMONTHDAY parts"
            ),
        }
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
            return Err(RuleRefused::NeverOccurs);
        }
        Ok(held)
    }

    /// **Whether a rule would be accepted**, for a write that has no anchor yet.
    pub fn check(rule: &str) -> Result<(), RuleRefused> {
        Recurrence::new(rule, Date::constant(2000, 1, 3)).map(|_| ())
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
            RuleRefused::NeverOccurs
        ));
        assert!(Recurrence::check("RRULE:FREQ=WEEKLY;BYDAY=MO").is_ok());
    }
}
