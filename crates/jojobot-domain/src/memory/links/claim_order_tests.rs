//! **The order a thing's claims are listed in** is one function, and it is held
//! by what it does to labels, not by where it is called: a label sorts by the
//! number in it, a label with no number follows every numbered one, and two such
//! labels sort by their text.

use super::*;
use std::cmp::Ordering;

fn sorted(labels: &[&str]) -> Vec<String> {
    let mut labels: Vec<String> = labels.iter().map(|label| label.to_string()).collect();
    labels.sort_by(|a, b| claim_label_order(a, b));
    labels
}

#[test]
fn a_label_sorts_by_its_number_and_not_by_its_text() {
    assert_eq!(claim_label_order("f2", "f10"), Ordering::Less);
    assert_eq!(claim_label_order("f10", "f2"), Ordering::Greater);
    assert_eq!(claim_label_order("f9", "f9"), Ordering::Equal);
    assert_eq!(
        sorted(&["f10", "f2", "f1", "f11", "f9"]),
        ["f1", "f2", "f9", "f10", "f11"]
    );
}

#[test]
fn a_label_with_no_number_follows_every_numbered_label_by_its_text() {
    assert_eq!(
        sorted(&["note", "f10", "ff3", "f2", "alpha"]),
        ["f2", "f10", "alpha", "ff3", "note"],
        "ff3 holds no number: only one leading f is part of the label's form"
    );
}
