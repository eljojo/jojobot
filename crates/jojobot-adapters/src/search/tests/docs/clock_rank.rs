//! A named clock ranks the claim of the chosen day ahead of the stronger score,
//! and behind the claim that matches more of the query.

use super::super::*;

fn first_for(index: &FullTextIndex, query: SearchQuery) -> String {
    let hits = index.search(&query).expect("search ok");
    format!("{:?}", hits.first().expect("a hit"))
}

fn named(text: &str) -> SearchQuery {
    SearchQuery {
        rank_clock: RankClock::RecordedAt,
        rank_clock_chosen: true,
        ..SearchQuery::text(text)
    }
}

/// **Naming a clock puts recency ahead of the match score; naming none leaves the
/// stronger score first.** Both claims match both query words, so term coverage
/// ties. The older claim repeats the words and scores higher. Without a clock the
/// higher score leads and the day only breaks ties. With the default clock named,
/// the later claim leads: whether a clock was named, not which one, decides it.
/// The served text of the argument says this, so the two orders are pinned here.
#[tokio::test]
async fn a_named_clock_ranks_ahead_of_the_match_score_and_none_does_not() {
    let strong_and_old = fact(
        "person:alpha",
        "f1",
        "bought bike, bought bike, bought bike",
        date(2026, 1, 1),
    );
    let weak_and_new = fact("person:alpha", "f2", "bought bike", date(2026, 6, 1));
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![strong_and_old, weak_and_new],
    )]);

    let unnamed = first_for(&index, SearchQuery::text("bought bike"));
    assert!(
        unnamed.contains("bought bike, bought bike"),
        "with no clock named the higher score leads: {unnamed}"
    );
    let with_clock = first_for(&index, named("bought bike"));
    assert!(
        !with_clock.contains("bought bike, bought bike"),
        "with a clock named the claim of the later day leads: {with_clock}"
    );
}

/// **A named clock does not outrank matching more of the query.** The older claim
/// matches both words and the later one matches one, so the older leads whether or
/// not a clock is named. The served text says more matching terms lead first.
#[tokio::test]
async fn a_named_clock_does_not_outrank_a_claim_matching_more_of_the_query() {
    let both_words = fact("person:alpha", "f1", "bought the bike", date(2026, 1, 1));
    let one_word = fact("person:alpha", "f2", "bought the pump", date(2026, 6, 1));
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![both_words, one_word],
    )]);

    for query in [SearchQuery::text("bought bike"), named("bought bike")] {
        let first = first_for(&index, query);
        assert!(first.contains("the bike"), "{first}");
    }
}
