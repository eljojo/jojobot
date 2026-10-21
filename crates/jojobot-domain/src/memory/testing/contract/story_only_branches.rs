//! **Branches a user story alone held, proven against any [`Memory`] adapter.**
//! A story's failure points at a scenario; these point at the branch.

use super::support::{capture, ensure, state};
use super::*;

/// **A calendar rule no day satisfies is refused with the day the loop runs it
/// from when the loop has one, and without any day when it has none yet.** The
/// rule is the 30th of February, which no start day can satisfy. With a
/// `counts_from` the refusal names that day and the key; with neither
/// `counts_from` nor `anchored_on` it names no day and no key, because it has no
/// day to name. **Paired**: a rule that can recur lands on a loop with no day.
pub async fn a_rule_no_day_satisfies_is_refused_with_a_day_only_when_the_loop_has_one<M: Memory>(
    store: &M,
) {
    let never = "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30";
    let with_day = EntityId("rhythm:contract-kiln".into());
    let no_day = EntityId("rhythm:contract-loom".into());
    let fine = EntityId("rhythm:contract-anvil".into());
    for id in [&with_day, &no_day, &fine] {
        ensure(store, id).await;
    }
    let write = |id: &EntityId, rule: &str, basis: Option<&str>| {
        let mut fields = vec![("rrule".to_string(), rule.to_string())];
        if let Some(day) = basis {
            fields.push(("counts_from".to_string(), day.to_string()));
        }
        state(NewFact {
            fields: fields.into_iter().collect(),
            ..NewFact::about(id.clone(), "a rule", date(2026, 8, 1))
        })
    };

    let dated = store
        .capture(write(&with_day, never, Some("2027-03-01")))
        .await;
    assert!(
        matches!(&dated, Err(MemoryError::InvalidFact(why))
            if why.contains("2027-03-01") && why.contains("counts_from")),
        "a loop with a day is told the day and the key that holds it, got {dated:?}",
    );

    let undated = store.capture(write(&no_day, never, None)).await;
    assert!(
        matches!(&undated, Err(MemoryError::InvalidFact(why))
            if why.contains("rrule")
                && !why.contains("counts_from")
                && !why.contains("anchored_on")
                && !why.chars().any(|c| c.is_ascii_digit())),
        "a loop with no day is refused naming no day and no key, got {undated:?}",
    );
    assert!(
        store
            .recall(&no_day)
            .await
            .expect("a recall of a refused loop")
            .is_empty(),
        "the refused write leaves nothing behind",
    );

    // The positive both refusals rest on: a rule that can recur lands with no day.
    capture(store, write(&fine, "FREQ=MONTHLY;BYMONTHDAY=1", None)).await;
}
