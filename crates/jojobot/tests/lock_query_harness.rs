//! **Two lock cases that read a real room through a server built with the room
//! harness.**
//!
//! A lock's query reads the room through the harness route, which only a server
//! built with `room-harness` carries. This file is compiled only with the
//! `jojobot/room-harness` feature, which the check lane passes for the whole
//! workspace and the flake's production test build does not, so the check lane
//! runs the cases and the flake build never asks a harness route of a server
//! that has none.

use jojobot_exercise::run::Expectation;

/// **A refused query is a fourth way `held` reads false, and it must be
/// told apart from a query that ran and found the room wanting.**
///
/// Both halves, because a resolver that answered `refused` for everything
/// would pass on this alone: a subject that does not exist is refused, and
/// a query that runs cleanly is not.
#[tokio::test]
async fn a_refused_query_is_told_apart_from_one_that_ran_and_held() {
    let (_room, surface) = jojobot_exercise::room::Room::open_with_client(
        &jojobot_exercise::room::server_binary().expect("a jojobot binary"),
    )
    .await
    .expect("a room");
    let boundaries: Vec<jojobot_exercise::run::Boundary> = Vec::new();
    let seen = jojobot_exercise::run::Observed {
        room: &surface,
        boundaries: &boundaries,
    };

    let locks = jojobot_exercise::lock::read(
        "```locks\n\
         recall  {\"subject\": \"person:contract-nobody-such-handle\"}\n\
         carries \"anything\"\n\
         say     this cannot be measured because the subject does not exist\n\
         \n\
         search  {\"query\": \"*\", \"limit\": 1}\n\
         carries \"results\"\n\
         say     this ran cleanly and holds\n\
         ```\n",
    )
    .expect("both read");

    let refused = locks[0].check(&seen).await;
    assert!(
        refused.refused,
        "a subject that does not exist must be reported as refused: {}",
        refused.saying,
    );
    assert!(
        !refused.held,
        "a refused query cannot also be reported as holding",
    );

    let ran = locks[1].check(&seen).await;
    assert!(
        !ran.refused,
        "a query that ran cleanly must not be reported as refused: {}",
        ran.saying,
    );
    assert!(
        ran.held,
        "a query that ran cleanly and met its assertion must hold: {}",
        ran.saying,
    );
}

/// **A verb-naming lock's failure sentence names what it actually
/// checked** — the finished room, once, after every phase — rather than
/// implying it watched the phase it is written under. Both sites,
/// because a lock's query can fail two different ways: it can run
/// cleanly and miss its assertion, or it can be refused outright, and a
/// rewording that fixed only one would leave the other still speaking as
/// though it had watched a phase.
#[tokio::test]
async fn a_query_locks_failure_names_the_finished_room_as_its_frame() {
    let (_room, surface) = jojobot_exercise::room::Room::open_with_client(
        &jojobot_exercise::room::server_binary().expect("a jojobot binary"),
    )
    .await
    .expect("a room");
    let boundaries: Vec<jojobot_exercise::run::Boundary> = Vec::new();
    let seen = jojobot_exercise::run::Observed {
        room: &surface,
        boundaries: &boundaries,
    };

    let locks = jojobot_exercise::lock::read(
        "```locks\n\
         recall  {\"kind\": \"person\"}\n\
         carries person:contract-nobody-such-handle\n\
         say     nobody such is on the roster\n\
         \n\
         recall  {\"subject\": \"person:contract-nobody-such-handle\"}\n\
         carries \"anything\"\n\
         say     this cannot be measured because the subject does not exist\n\
         ```\n",
    )
    .expect("both read");

    let missed = locks[0].check(&seen).await;
    assert!(
        !missed.held && !missed.refused,
        "the first lock's query must run cleanly and miss its assertion: {}",
        missed.saying,
    );
    assert!(
        missed.saying.contains("finished room"),
        "a missed assertion's failure did not name the finished room as its frame: {}",
        missed.saying,
    );

    let refused = locks[1].check(&seen).await;
    assert!(
        refused.refused,
        "the second lock's query must be refused: {}",
        refused.saying,
    );
    assert!(
        refused.saying.contains("finished room"),
        "a refused query's failure did not name the finished room as its frame: {}",
        refused.saying,
    );
}
