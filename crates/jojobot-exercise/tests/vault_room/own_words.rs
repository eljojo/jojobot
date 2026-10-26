//! **Two locks that a correct sitting must not fail, played as the paid run
//! played them.**
//!
//! A paid run of the vault failed two locks with answers that were right: the
//! packing lock wanted the entry's text to be exactly "camera" and the list held
//! "The camera", and the December overdue lock wanted the desk owed when the
//! March sitting had, rightly, ended it. Each case here plays the year as that
//! run did and asks every lock; the controls say the scenario really is the one
//! the case names.

use super::*;

/// **The packing list in the operator's own words holds every lock.** The three
/// things are kept as "The passports", "The travel adapter" and "The camera";
/// November ticks the first two and the camera stays open.
#[tokio::test]
async fn a_packing_list_in_the_operators_own_words_holds_every_lock() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_with(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::Whole,
        Lists {
            september: SeptemberList::InTheOperatorsWords,
            ..LISTS_KEPT
        },
        FILED_AS_USUAL,
    )
    .await;
    let list = surface
        .call("recall", json!({"subject": "list:packing"}))
        .await;
    assert!(
        list.contains("\"text\":\"The camera\""),
        "the control failed: the list does not hold the camera in the operator's words, so this case proves nothing: {list}",
    );
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a packing list in the operator's own words failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **A desk the March sitting ended holds every lock, December's overdue lock
/// among them.** "The desk stays" ends the window, so nothing is owed for it and
/// the read counts it among the finished. The control reads the same question
/// the lock asks and finds the desk finished.
#[tokio::test]
async fn a_desk_the_march_sitting_ended_holds_every_lock() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_with(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::Whole,
        LISTS_KEPT,
        Filed {
            desk: Desk::Ended,
            ..FILED_AS_USUAL
        },
    )
    .await;
    let asked = json!({"answers_type": "runs-out", "overdue": {"as_of": "2026-12-06"}});
    let december = surface.call("recall", asked).await;
    assert!(
        december.contains("\"finished\":{\"count\":1"),
        "the control failed: the desk is not counted among the finished, so the sitting did not end it and this case proves nothing: {december}",
    );
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a desk the March sitting ended failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// The sentence of December's overdue lock, as far as it is told apart.
const OVERDUE_LOCK: &str = "fails to say how many of the rest it correctly left out";

/// **A pack undone is not packed.** The passports and the adapter are ticked
/// done, then the passports are undone: two done ticks stand in the history and
/// one entry stands packed. The lock that reads two packed reddens, and no other
/// lock does.
#[tokio::test]
async fn a_packing_list_with_one_pack_undone_reds_only_the_packing_check() {
    reds_exactly(
        Lists {
            november: NovemberList::OnePackedThenUnpacked,
            ..LISTS_KEPT
        },
        &[TICKED_LOCK],
    )
    .await;
}

/// **A December that files a fifth window not due yet reddens December's overdue
/// lock and no other.** The read leaves out five windows for not being due where
/// the lock holds that it leaves out four. The control asks the same question
/// the lock asks and finds five.
#[tokio::test]
async fn december_filing_a_fifth_window_that_is_not_due_reds_only_the_overdue_lock() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_with(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::AFifthWindow,
        LISTS_KEPT,
        FILED_AS_USUAL,
    )
    .await;
    let asked = json!({"answers_type": "runs-out", "overdue": {"as_of": "2026-12-06"}});
    let december = surface.call("recall", asked).await;
    assert!(
        december.contains("\"not_due_yet\":{\"count\":5,"),
        "the control failed: the read does not leave five windows out for not being due, so this case proves nothing: {december}",
    );
    let red: Vec<String> = judge_all(&surface, &boundaries)
        .await
        .into_iter()
        .filter(|outcome| !outcome.held)
        .map(|outcome| outcome.saying)
        .collect();
    assert_eq!(
        red.len(),
        1,
        "more than the overdue lock reddened: {red:#?}"
    );
    assert!(red[0].contains(OVERDUE_LOCK), "{red:#?}");
}
