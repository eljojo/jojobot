//! "What would I look for when I open jojobot in the browser?"
//!
//! **The operator's window is read through the pages a person opens**, over a
//! store that bots filled with the tools they hold. The homepage lists what
//! jojobot holds, kind by kind, and what is due; the org chart puts each bot
//! under the bot it reports to; a kind's page lists every thing of that kind; the
//! search box asks the search the bots use. Each story seeds through MCP calls
//! and opens the page the way a browser does, login included.

use serde_json::json;

use super::dsl::{Answer, Story};

/// The HTML of the element with this id, from its tag to the end of its section
/// or table.
fn section(page: &Answer, id: &str) -> String {
    let raw = page.raw();
    let start = raw
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("no element {id:?} on {}: {raw}", "the page"));
    let rest = &raw[start..];
    let end = rest
        .find("</section>")
        .or_else(|| rest.find("</table>"))
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[tokio::test]
async fn the_homepage_lists_what_is_held_by_kind_and_what_is_due() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add("place:springfield", "Springfield").await;
    // A loop that went quiet long ago: it is what the due box exists to show.
    s.call(
        "add_entity",
        json!({"kind": "rhythm", "handle": "descale", "name": "Descale the machine",
               "source": "user-named", "parent": "person:homer",
               "sets": {"cadence_days": "7", "advances_from": "check_in_date"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "rhythm:descale", "content": "the first descale",
               "provenance": "testimony", "recorded_at": "2026-01-01", "check_in": "ran"}),
    )
    .await;

    let home = story.follow("/").await;

    // A box for each kind that has members, with its members linked, and the way
    // to all of them.
    let people = section(&home, "kind-person");
    assert!(people.contains("href=\"/person:homer/\""), "{people}");
    assert!(people.contains("Homer"), "{people}");
    assert!(people.contains("href=\"/ui/kind/person/\""), "{people}");
    let places = section(&home, "kind-place");
    assert!(places.contains("href=\"/place:springfield/\""), "{places}");
    // The negative that makes the boxes mean something: the town is not among the
    // people.
    assert!(!people.contains("springfield"), "{people}");

    // The top level lists what sits under nothing.
    let roots = section(&home, "roots");
    assert!(roots.contains("person:homer"), "{roots}");

    // What is due is the loop that went quiet, linked.
    let due = section(&home, "due");
    assert!(
        due.contains("href=\"/person:homer/rhythm:descale/\""),
        "the quiet loop is in the due box: {due}"
    );
    // Every page carries the search box.
    assert!(home.raw().contains("class=\"find\""), "{}", home.raw());
    story.finish().await;
}

#[tokio::test]
async fn the_org_chart_puts_each_bot_under_the_bot_it_reports_to() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:sigma", "Sigma").await;
    let omega = story.as_bot("bot:omega").await;
    omega
        .call(
            "capture",
            json!({"subject": "bot:sigma", "content": "sigma reports to omega",
                   "provenance": "testimony", "sets": {"reports_to": "bot:omega"}}),
        )
        .await;

    let chart = story.follow("/ui/org/").await;
    let raw = chart.raw();
    let omega_at = raw
        .find("href=\"/bot:omega/\"")
        .unwrap_or_else(|| panic!("omega is on the chart: {raw}"));
    let sigma_at = raw
        .find("href=\"/bot:sigma/\"")
        .unwrap_or_else(|| panic!("sigma is on the chart: {raw}"));
    assert!(omega_at < sigma_at, "the head comes first: {raw}");
    // Sigma sits one level under omega, and omega is a head.
    let sigma_line = &raw[..sigma_at];
    let line_start = sigma_line.rfind("<li").expect("sigma's line");
    assert!(
        sigma_line[line_start..].contains("data-depth=\"1\""),
        "{raw}"
    );
    let omega_line = &raw[..omega_at];
    let omega_start = omega_line.rfind("<li").expect("omega's line");
    assert!(
        omega_line[omega_start..].contains("data-depth=\"0\""),
        "{raw}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_kinds_page_lists_every_thing_of_that_kind_and_no_other() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;
    s.add("place:springfield", "Springfield").await;
    s.add("place:shelbyville", "Shelbyville").await;
    s.add("person:homer", "Homer").await;

    let places = story.follow("/ui/kind/place/").await;
    let table = section(&places, "things");
    for handle in ["place:springfield", "place:shelbyville"] {
        assert!(
            table.contains(&format!("href=\"/{handle}/\"")),
            "{handle} is listed: {table}"
        );
    }
    // Paired with the positive above: a person is not in the list of places.
    assert!(!table.contains("person:homer"), "{table}");
    story.finish().await;
}

#[tokio::test]
async fn the_search_box_finds_what_a_bot_wrote_and_says_when_nothing_is_found() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.call(
        "capture",
        json!({"subject": "person:homer", "content": "keeps bees behind the plant",
               "provenance": "testimony"}),
    )
    .await;

    let found = story.follow("/ui/search?q=bees").await;
    let table = section(&found, "found");
    assert!(table.contains("href=\"/person:homer/\""), "{table}");
    assert!(table.contains("keeps bees behind the plant"), "{table}");

    let none = story.follow("/ui/search?q=zebra").await;
    assert!(
        none.raw().contains("id=\"nothing-found\""),
        "{}",
        none.raw()
    );
    assert!(!none.raw().contains("id=\"found\""), "{}", none.raw());
    story.finish().await;
}
