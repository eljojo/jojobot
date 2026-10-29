//! Every fold on the longboard is read for an invented date, not the first one.

use serde_json::{Value, json};

fn pile() -> Vec<Value> {
    vec![
        json!({"address": "thing:longboard#f1", "recorded_at": "2026-09-01",
               "happened_at": "2026-08-30", "content": "patched the nose"}),
        json!({"address": "thing:longboard#f2", "recorded_at": "2026-09-14",
               "content": "a crack near the nose"}),
    ]
}

fn fold(address: &str, happened_at: &str, content: &str) -> Value {
    json!({"address": address, "recorded_at": "2026-12-13", "happened_at": happened_at,
           "content": content, "stands_for": ["thing:longboard#f1", "thing:longboard#f2"]})
}

/// **A second fold that names a day nobody gave is caught**, and so is a first
/// one; folds that name only given days hold.
#[test]
fn a_later_fold_with_an_invented_day_is_not_hidden_behind_the_first() {
    let mut honest = pile();
    honest.push(fold(
        "thing:longboard#f3",
        "2026-08-30",
        "the pile since 2026-09-14",
    ));
    assert!(super::fold_invents_no_date(&honest).is_ok());

    let mut second_invents = honest.clone();
    second_invents.push(fold("thing:longboard#f4", "2026-10-02", "again"));
    assert!(
        super::fold_invents_no_date(&second_invents).is_err(),
        "a second fold's invented day passed behind an honest first one"
    );

    let mut first_invents = pile();
    first_invents.push(fold(
        "thing:longboard#f3",
        "2026-08-30",
        "the pile since 2026-09-20",
    ));
    assert!(super::fold_invents_no_date(&first_invents).is_err());

    assert!(
        super::fold_invents_no_date(&pile()).is_err(),
        "no fold at all held"
    );
}
