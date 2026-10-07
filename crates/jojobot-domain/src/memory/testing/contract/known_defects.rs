//! **Defects the product has today, written down as cases that fail.**
//!
//! A case here states how the product SHOULD answer and is marked `#[ignore]`
//! at both stores, with the reason. It is red when run on purpose, and it stays
//! out of the bar until a fix lands, so a defect nobody has scheduled is still
//! one a person can run and read rather than a sentence on a card.

use super::support::capture;
use super::*;
use crate::attention;

/// **A deadline written on a claim about a person, because the claim describes
/// one occasion, must not make the person owed.**
///
/// Every key on every claim folds onto the claim's subject, and what is owed is
/// found by the keys a thing holds, never by its kind. So a claim about a person
/// that carries `runs_out` makes the person show up as owed after that day —
/// the thing that was meant to be one occasion's deadline becomes the person's.
/// The only reason a person is kept out of the owed answer today is that no
/// carrier speaks for a person, and that holds only while nobody writes the key.
///
/// **Three answers, and each covers how the others pass on a build that is
/// wrong.** The claim still reads back with its deadline, which says the write
/// landed and the key was not simply refused. The same key in a bare map IS
/// owed after its day, which says the read understands the key and the negative
/// below is not passing because nothing is ever owed. And the person, whose
/// folded fields are what the owed read is handed, is not owed once the day has
/// passed — which is the answer the product does not give today.
///
/// **How keys pass from a record to its thing is an open ruling**, so this
/// states only the outcome and not a mechanism: a fix may stop the key at the
/// claim, or give a person a way to say which keys are theirs. This case holds
/// either.
pub async fn a_deadline_on_a_claim_about_a_person_does_not_make_the_person_owed<M: Memory>(
    store: &M,
) {
    let person = EntityId::person("person:contract-occasion-deadline");
    let day = date(2026, 6, 1);
    let after = date(2026, 6, 2);
    let deadline = (attention::RUNS_OUT.to_string(), day.to_string());

    let claim = capture(
        store,
        NewFact {
            fields: [deadline.clone()].into(),
            ..NewFact::about(
                person.clone(),
                "has a coupon in the drawer for one visit",
                date(2026, 5, 1),
            )
        },
    )
    .await;

    // The claim keeps the deadline it was written with.
    let read = store
        .recall(&person)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the claim reads back");
    assert_eq!(
        read.fields.get(attention::RUNS_OUT),
        Some(&deadline.1),
        "the deadline on the claim did not survive the write"
    );

    // The control: the read owes this key after its day, over a bare map.
    let carriers = attention::shipped();
    let carriers: Vec<&dyn attention::Carrier> = carriers.iter().map(|c| c.as_ref()).collect();
    let bare: std::collections::BTreeMap<String, String> = [deadline].into();
    assert!(
        attention::owed(&carriers, &bare).owed_on(after),
        "the owed read does not understand `runs_out`, so the answer below means nothing"
    );

    // The defect: the person, whose folded fields the owed read is handed.
    let folded = store.fields(&person).await.expect("fields should succeed");
    assert!(
        !attention::owed(&carriers, &folded).owed_on(after),
        "a person is owed because ONE claim about them carries a deadline for one occasion: {folded:?}"
    );
}
