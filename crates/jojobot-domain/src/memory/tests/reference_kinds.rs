use super::*;

#[tokio::test]
async fn waiting_on_takes_a_bot_or_a_person_and_refuses_work_on_the_fake() {
    contract::waiting_on_naming_work_is_refused_toward_depends_on(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn a_reference_naming_two_kinds_survives_the_fake() {
    contract::a_reference_naming_two_kinds_keeps_both_and_one_kind_keeps_its_spelling(
        &InMemoryMemory::booted(),
    )
    .await;
}
