mod support;

use jojobot_domain::actor::Actor;
use jojobot_domain::memory::search::Hit;
use jojobot_query_api::SearchQuestion;
use support::ui_board::{search_over, seeded_board_over, seeded_memory};

async fn searched(words: &str) -> jojobot_query_api::Searched {
    let memory = seeded_memory().await;
    let board = seeded_board_over(memory.clone()).await;
    jojobot_mcp::memory::view_run::handle(
        memory.clone(),
        board.sessions.clone(),
        search_over(memory),
    )
    .search(
        &Actor::Operator,
        SearchQuestion {
            words: words.to_string(),
        },
    )
    .await
    .expect("the search is answered")
}

#[tokio::test]
async fn the_handle_answers_a_search_with_the_hit_and_the_loose_matching_note_on_a_thin_answer() {
    let answer = searched("Otto").await;
    assert!(
        answer.hits.iter().any(|hit| matches!(
            hit,
            Hit::Entity { entity, .. } if entity.id.as_str() == "bot:otto"
        )),
        "the entity named in the words is a hit"
    );
    assert!(
        answer.matching_note.is_some(),
        "a thin answer says the words match loosely"
    );
    assert!(
        answer.index_note.is_none(),
        "a loaded index has no coverage note"
    );
}
