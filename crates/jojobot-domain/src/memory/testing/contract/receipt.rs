use super::support::{capture, edit, other_caller};
use super::*;
use crate::memory::Receipt;

/// An open inference about a fictional bike, the claim every case below
/// promotes.
async fn open_guess<M: Memory>(store: &M, handle: &str) -> Fact {
    let guess = capture(
        store,
        NewFact::about(
            EntityId(handle.to_string()),
            "the bike has two locks",
            date(2026, 10, 1),
        ),
    )
    .await;
    assert_eq!(guess.provenance, Provenance::Inference);
    assert_eq!(guess.standing, Standing::Open);
    guess
}

/// The claim a store holds under `id`, read back by recall.
async fn read_back<M: Memory>(store: &M, subject: &EntityId, id: &FactId) -> Fact {
    store
        .recall(subject)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| &f.id == id)
        .unwrap_or_else(|| panic!("recall must return the captured fact (id {id})"))
}

fn settle(receipt: Option<Receipt>) -> FactPatch {
    FactPatch {
        standing: Some(Standing::Settled),
        receipt,
        ..FactPatch::default()
    }
}

/// **Settling an open claim without a receipt is refused, and the refusal is
/// not the operator's flag.** The same claim, settled with a check, stands
/// settled and is still an inference. Both halves in one read, so the refusal
/// is about the missing receipt and not about a claim that cannot be settled.
pub async fn settling_an_inference_needs_a_receipt<M: Memory>(store: &M) {
    let guess = open_guess(store, "thing:contract-receipt-check").await;

    // `confirmed_by_user` was the old key to this door. It is not a receipt.
    for confirmed_by_user in [false, true] {
        let err = store
            .update_fact(
                &guess.address(),
                FactPatch {
                    confirmed_by_user,
                    ..settle(None)
                },
                &other_caller(),
            )
            .await
            .expect_err("a settling with no receipt must be refused");
        assert!(
            matches!(err, MemoryError::UnreceiptedSettling),
            "expected UnreceiptedSettling (confirmed_by_user {confirmed_by_user}), got {err:?}"
        );
    }
    let untouched = read_back(store, &guess.subject, &guess.id).await;
    assert_eq!(untouched.standing, Standing::Open);
    assert_eq!(untouched.receipt, None, "a refused settling stores nothing");

    let check = Receipt::Check("counted the locks on thing:contract-receipt-check".into());
    let settled = edit(store, &guess.address(), settle(Some(check.clone()))).await;
    assert_eq!(settled.standing, Standing::Settled);
    assert_eq!(
        settled.provenance,
        Provenance::Inference,
        "promotion moves standing and never provenance"
    );
    assert_eq!(settled.receipt, Some(check.clone()));

    let now = read_back(store, &guess.subject, &guess.id).await;
    assert_eq!(now.standing, Standing::Settled);
    assert_eq!(now.provenance, Provenance::Inference);
    assert_eq!(
        now.receipt,
        Some(check.clone()),
        "the receipt reaches the store"
    );

    let writes = store
        .claim_history(&guess.address())
        .await
        .expect("the writes of a claim read");
    assert_eq!(writes.len(), 2, "a capture and the settling: {writes:?}");
    assert_eq!(writes[0].receipt, None, "the first write had no receipt");
    assert_eq!(
        writes[1].receipt,
        Some(check),
        "the write that settled the claim carries its receipt"
    );
}

/// **The operator's own confirmation is the other shape**, with the day and
/// the place. It settles an inference without making it testimony.
pub async fn a_confirmed_receipt_settles_and_keeps_the_provenance<M: Memory>(store: &M) {
    let guess = open_guess(store, "thing:contract-receipt-confirmed").await;
    let confirmed = Receipt::Confirmed {
        on: date(2026, 10, 12),
        at: "message q7w2ke".into(),
    };
    let settled = edit(store, &guess.address(), settle(Some(confirmed.clone()))).await;
    assert_eq!(settled.standing, Standing::Settled);
    assert_eq!(settled.provenance, Provenance::Inference);

    let now = read_back(store, &guess.subject, &guess.id).await;
    assert_eq!(now.receipt, Some(confirmed.clone()));
    assert_eq!(now.provenance, Provenance::Inference);
    let writes = store.claim_history(&guess.address()).await.expect("reads");
    assert_eq!(writes.last().expect("a write").receipt, Some(confirmed));
}

/// **A receipt that says nothing is no receipt.** A blank check and a blank
/// place are refused and the claim stays open; the same two with their text
/// settle it.
pub async fn a_receipt_with_nothing_in_it_is_refused<M: Memory>(store: &M) {
    let guess = open_guess(store, "thing:contract-receipt-blank").await;
    for blank in [
        Receipt::Check("   ".into()),
        Receipt::Confirmed {
            on: date(2026, 10, 12),
            at: " ".into(),
        },
    ] {
        let err = store
            .update_fact(
                &guess.address(),
                settle(Some(blank.clone())),
                &other_caller(),
            )
            .await
            .expect_err("an empty receipt must be refused");
        assert!(
            matches!(err, MemoryError::InvalidFact(_)),
            "expected InvalidFact for {blank:?}, got {err:?}"
        );
    }
    assert_eq!(
        read_back(store, &guess.subject, &guess.id).await.standing,
        Standing::Open
    );

    let settled = edit(
        store,
        &guess.address(),
        settle(Some(Receipt::Check(
            "  read the lock count off the frame  ".into(),
        ))),
    )
    .await;
    assert_eq!(
        settled.receipt,
        Some(Receipt::Check("read the lock count off the frame".into())),
        "the text is stored trimmed"
    );
}

/// **Reopening takes the receipt off the claim and the writes keep it.** A
/// claim that is open again is not promoted, so it reads back with none; the
/// chain still says what settled it, and a later settling needs a new one.
pub async fn reopening_takes_the_receipt_off_and_the_history_keeps_it<M: Memory>(store: &M) {
    let guess = open_guess(store, "thing:contract-receipt-reopen").await;
    let check = Receipt::Check("counted the locks".into());
    edit(store, &guess.address(), settle(Some(check.clone()))).await;

    let reopened = edit(
        store,
        &guess.address(),
        FactPatch {
            standing: Some(Standing::Open),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(reopened.standing, Standing::Open);
    assert_eq!(reopened.receipt, None);
    assert_eq!(
        read_back(store, &guess.subject, &guess.id).await.receipt,
        None
    );

    let writes = store.claim_history(&guess.address()).await.expect("reads");
    assert_eq!(writes.len(), 3, "{writes:?}");
    assert_eq!(
        writes[1].receipt,
        Some(check),
        "the settling write keeps it"
    );
    assert_eq!(writes[2].receipt, None, "the reopening write has none");

    let err = store
        .update_fact(&guess.address(), settle(None), &other_caller())
        .await
        .expect_err("a second settling needs a receipt of its own");
    assert!(matches!(err, MemoryError::UnreceiptedSettling), "{err:?}");
}

/// **An edit that says nothing about the receipt leaves it**, and a capture
/// takes one without being refused for lacking one (rule 48: the gate is on a
/// promotion, never on declaring a standing).
pub async fn a_receipt_survives_edits_that_do_not_name_it_and_a_capture_may_carry_one<M: Memory>(
    store: &M,
) {
    let guess = open_guess(store, "thing:contract-receipt-kept").await;
    let check = Receipt::Check("counted the locks".into());
    edit(store, &guess.address(), settle(Some(check.clone()))).await;
    let retitled = edit(
        store,
        &guess.address(),
        FactPatch {
            details: Some("seen from the front".into()),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(retitled.receipt, Some(check.clone()), "{retitled:?}");
    assert_eq!(
        read_back(store, &guess.subject, &guess.id).await.receipt,
        Some(check)
    );

    let subject = EntityId("thing:contract-receipt-capture".into());
    let declared = capture(
        store,
        NewFact {
            standing: Some(Standing::Settled),
            ..NewFact::about(subject.clone(), "the bike is red", date(2026, 10, 1))
        },
    )
    .await;
    assert_eq!(declared.standing, Standing::Settled);
    assert_eq!(
        declared.receipt, None,
        "a capture is not refused for lacking one"
    );

    let carried = Receipt::Check("looked at the bike".into());
    let with = capture(
        store,
        NewFact {
            standing: Some(Standing::Settled),
            receipt: Some(carried.clone()),
            ..NewFact::about(subject.clone(), "the bike has a bell", date(2026, 10, 1))
        },
    )
    .await;
    assert_eq!(with.receipt, Some(carried.clone()));
    assert_eq!(
        read_back(store, &subject, &with.id).await.receipt,
        Some(carried.clone())
    );
    let writes = store.claim_history(&with.address()).await.expect("reads");
    assert_eq!(writes[0].receipt, Some(carried));
}

/// **A receipt goes only with a claim that will stand settled.** A receipt on a
/// claim left open is refused, and so is a receipt sent in the same patch that
/// reopens a settled claim; neither changes the claim. The positive: a claim
/// that is settled already takes a new receipt without naming standing, and the
/// new one replaces the old.
pub async fn a_receipt_goes_only_with_a_claim_that_stands_settled<M: Memory>(store: &M) {
    let guess = open_guess(store, "thing:contract-receipt-unsettled").await;
    let check = Receipt::Check("counted the locks on thing:contract-receipt-unsettled".into());

    // An open claim and a receipt, with no move to settled.
    let err = store
        .update_fact(
            &guess.address(),
            FactPatch {
                receipt: Some(check.clone()),
                ..FactPatch::default()
            },
            &other_caller(),
        )
        .await
        .expect_err("a receipt on a claim left open must be refused");
    assert!(
        matches!(err, MemoryError::ReceiptWithoutSettling),
        "expected ReceiptWithoutSettling, got {err:?}"
    );
    let untouched = read_back(store, &guess.subject, &guess.id).await;
    assert_eq!(untouched.standing, Standing::Open);
    assert_eq!(untouched.receipt, None);

    // Settled with the receipt, then a patch that reopens it AND sends one.
    edit(store, &guess.address(), settle(Some(check.clone()))).await;
    let err = store
        .update_fact(
            &guess.address(),
            FactPatch {
                standing: Some(Standing::Open),
                receipt: Some(Receipt::Check("looked again".into())),
                ..FactPatch::default()
            },
            &other_caller(),
        )
        .await
        .expect_err("a receipt beside a reopening must be refused");
    assert!(
        matches!(err, MemoryError::ReceiptWithoutSettling),
        "expected ReceiptWithoutSettling, got {err:?}"
    );
    let held = read_back(store, &guess.subject, &guess.id).await;
    assert_eq!(
        held.standing,
        Standing::Settled,
        "the refusal reopened nothing"
    );
    assert_eq!(held.receipt, Some(check));

    // The positive: a settled claim takes a new receipt without naming standing.
    let newer = Receipt::Check("counted them a second time".into());
    let replaced = edit(
        store,
        &guess.address(),
        FactPatch {
            receipt: Some(newer.clone()),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(replaced.standing, Standing::Settled);
    assert_eq!(replaced.receipt, Some(newer.clone()));
    assert_eq!(
        read_back(store, &guess.subject, &guess.id).await.receipt,
        Some(newer)
    );
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_receipt<M: Memory>(store: &M) {
    settling_an_inference_needs_a_receipt(store).await;
    a_confirmed_receipt_settles_and_keeps_the_provenance(store).await;
    a_receipt_with_nothing_in_it_is_refused(store).await;
    reopening_takes_the_receipt_off_and_the_history_keeps_it(store).await;
    a_receipt_survives_edits_that_do_not_name_it_and_a_capture_may_carry_one(store).await;
    a_receipt_goes_only_with_a_claim_that_stands_settled(store).await;
}
