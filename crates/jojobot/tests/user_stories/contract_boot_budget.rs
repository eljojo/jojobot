//! **This file is a contract, not a story.** It pins how much the boot may
//! weigh. Nothing here is a use case in a person's words: each case builds a
//! heavy instance, boots it, and measures the answer against the ceiling the
//! boot is built to stay under, and against what it must name when it leaves
//! something out. The stories in `boot.rs` ask what a fresh session finds; these
//! ask how big the answer is allowed to get.

use serde_json::json;

use super::boot::slug;
use super::dsl::Story;

/// **A fresh instance's anonymous boot stays under the ceiling, and says so
/// when it left part of the essay out.** The ceiling bounds the answer a
/// caller receives, serialized: the boot sizes what it ships by what it costs
/// there, so a caller never gets more than the ceiling while the boot claims
/// nothing was cut. Whatever the answer left out is named, and the next call
/// that reaches it is named beside it.
#[tokio::test]
async fn a_fresh_anonymous_boot_is_under_the_ceiling_and_names_what_it_left_out() {
    let story = Story::begin_with_nothing_written().await;
    let (anon, _) = story.call("start_here", json!({})).await;
    let whole = anon.raw().chars().count();
    assert!(
        whole <= jojobot_domain::text::BOOT_ANSWER.budget,
        "the answer is {whole} characters, over the ceiling"
    );
    let booted = anon.json();
    if booted["orientation_elided"] == true {
        assert!(
            booted["orientation_note"].is_string(),
            "a boot that left something out says what: {booted}"
        );
    } else {
        assert!(booted["orientation_note"].is_null(), "{booted}");
    }
    // The core ships either way: a caller is always taught the surface.
    assert!(
        booted["orientation"]
            .as_str()
            .is_some_and(|essay| essay.starts_with("# jojobot")),
        "{booted}"
    );
}

/// **An instance as heavy as a real one gets**: many bots, and one thing of each
/// kind that can stand alone. The anonymous boot names every bot, so this is
/// what takes room from the essay's remainder.
/// How many bots the heavy instance has: enough that the anonymous boot, which
/// names every one, has to leave part of the essay out.
const HEAVY_BOTS: usize = 150;

async fn make_the_instance_heavy(story: &Story) -> super::dsl::Session {
    let s = story.session().await;
    for i in 0..HEAVY_BOTS {
        s.add(&format!("bot:{}", slug(i)), &slug(i + 10_000)).await;
    }
    for (n, kind) in [
        "person", "project", "place", "event", "work", "thing", "org", "topic", "pet", "machine",
        "thread",
    ]
    .iter()
    .enumerate()
    {
        s.add(&format!("{kind}:{}", slug(n + 20_000)), &slug(n + 30_000))
            .await;
    }
    s
}

/// The quoted names in a refusal that lists them: the remainder's sections, by
/// the exact heading each is fetched with, and `opening` for the paragraphs
/// before the first heading.
fn quoted_section_names(refusal: &serde_json::Value) -> Vec<String> {
    refusal["how_to_proceed"]
        .as_str()
        .unwrap_or_default()
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|name| *name == "opening" || name.starts_with("## "))
        .map(str::to_string)
        .collect()
}

/// **What the anonymous boot of this instance leaves out of the essay, checked
/// against everything it promises.** The core ships whole (it is what a named
/// boot is served as its orientation); whatever remainder follows is whole
/// sections in order; every section it did not ship is named in the note and
/// comes back whole from `start_here` with `section`, and the shipped part and
/// the fetched parts together are the remainder, each heading once and in order.
/// Returns how many sections it left out.
async fn the_anonymous_boot_names_everything_it_leaves_out(story: &Story) -> usize {
    let (named, _) = story
        .call("start_here", json!({"bot": "assistant", "brief": false}))
        .await;
    let core = named.json()["orientation"]
        .as_str()
        .expect("a named boot is served the core as its orientation")
        .to_string();
    let (anon, _) = story.call("start_here", json!({})).await;
    let booted = anon.json();
    let shipped = booted["orientation"]
        .as_str()
        .expect("the anonymous boot ships an orientation");
    assert!(
        shipped.starts_with(&core),
        "the anonymous boot does not open with the whole core: {booted}"
    );
    let rest = &shipped[core.len()..];

    let listing = story
        .refused_at_the_door("start_here", json!({"section": "## No such section"}))
        .await;
    let names = quoted_section_names(&listing.json());
    assert!(
        names.len() > 3,
        "the essay's sections could not be listed from the refusal: {}",
        listing.json()
    );
    let note = booted["orientation_note"].as_str().unwrap_or_default();
    let opening_shipped = !rest.trim().is_empty();
    let mut left_out = 0;
    let mut rebuilt = rest.to_string();
    for name in &names {
        let shipped_whole = if name == "opening" {
            opening_shipped
        } else {
            rest.lines().any(|line| line.trim() == name)
        };
        if shipped_whole {
            continue;
        }
        left_out += 1;
        assert!(
            note.contains(name.as_str()),
            "the boot left out {name:?} and did not name it: {note}"
        );
        let (fetched, _) = story.call("start_here", json!({"section": name})).await;
        let body = fetched.json()["section"]["body"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{name:?} does not come back from section: {}",
                    fetched.json()
                )
            })
            .to_string();
        assert!(
            name == "opening" || body.trim_start().starts_with(name.as_str()),
            "{name:?} comes back without its own heading: {body}"
        );
        assert!(!body.trim().is_empty(), "{name:?} comes back empty");
        rebuilt.push_str(&body);
    }
    assert_eq!(
        booted["orientation_elided"],
        left_out > 0,
        "the boot's own flag disagrees with what it left out: {booted}"
    );
    // The shipped part and the fetched parts are the remainder: each heading
    // appears once, in the order the essay lists them.
    let mut at = 0;
    for name in names.iter().filter(|name| name.as_str() != "opening") {
        let found = rebuilt[at..]
            .find(&format!("\n{name}"))
            .or_else(|| rebuilt[at..].starts_with(name.as_str()).then_some(0))
            .unwrap_or_else(|| panic!("{name:?} is not in the remainder, in order: {rebuilt}"));
        at += found + name.len();
    }
    left_out
}

/// **Pin (a): the anonymous boot ships the core whole and names every section it
/// leaves out, on a fresh real instance and on a heavy one.** The essay is
/// ranked against the room the instance leaves, so the whole of it is not a
/// promise: what is promised is that nothing is dropped unnamed and that every
/// dropped section is one call away, whole. The heavy instance must actually
/// cut, or the pin would hold over a boot that never had to name anything.
#[tokio::test]
async fn the_anonymous_boot_names_every_section_it_leaves_out_on_a_fresh_and_a_heavy_instance() {
    let fresh = Story::begin_with_nothing_written().await;
    the_anonymous_boot_names_everything_it_leaves_out(&fresh).await;
    fresh.finish().await;

    let heavy = Story::begin("bot:otto").await;
    let _writer = make_the_instance_heavy(&heavy).await;
    let left_out = the_anonymous_boot_names_everything_it_leaves_out(&heavy).await;
    eprintln!("the heavy instance's anonymous boot left out {left_out} section(s)");
    assert!(
        left_out >= 1,
        "the heavy instance left nothing out, so this pin proved nothing about naming"
    );
    heavy.finish().await;
}

/// **Pin (b): a named boot of a heavy identity fits the core and its floor under
/// the ceiling, with the margin printed.** A named boot is served the core and
/// what cannot be cut. The identity here is a heavy one on a heavy instance:
/// the roster of a hundred and fifty bots in the snapshot, a long charter, and
/// starred rules until the guard on a star refuses one or the seats run out.
///
/// **`MARGIN` is the room the answer keeps under the ceiling** for the next
/// additions to the core and for the characters a serialized answer spends on
/// escapes. A change that eats it fails here before a boot is cut.
///
/// **The margin is 450, the PM's call, overturnable.** Each shipped type adds
/// its name to the vocabulary every boot carries, about 26 characters, and the
/// sixth type (`computed-dates`) took the room under 500. 450 leaves room for
/// one more shipped type of that size. The pressure underneath is that every
/// shipped type costs every boot, which is a design question and not this pin's.
#[tokio::test]
async fn a_named_boot_of_a_heavy_identity_fits_core_and_floor_with_a_margin() {
    const MARGIN: usize = 450;
    const CHARTER_CHARS: usize = 6_000;
    const STARRED_RULES: usize = 3;
    let story = Story::begin("bot:otto").await;
    let s = make_the_instance_heavy(&story).await;
    let prose: String = (0..CHARTER_CHARS / 12)
        .map(|word| format!("{}c ", slug(word + 40_000)))
        .collect();
    s.call("set_charter", json!({"bot": "bot:otto", "prose": prose}))
        .await;
    let paragraph: String = (0..60)
        .map(|word| format!("{}rule{word} ", slug(word)))
        .collect();
    for rule in 0..STARRED_RULES {
        s.call(
            "capture",
            json!({"subject": "bot:otto", "content": format!("{rule}: {paragraph}"),
                   "provenance": "testimony", "fields": {"starred": "true"}}),
        )
        .await;
    }
    // No `resume`: a boot that sends one withholds the charter, and the charter
    // is what makes this identity heavy.
    let (named, _) = story.call("start_here", json!({"bot": "otto"})).await;
    let whole = named.raw().chars().count();
    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
    assert_eq!(
        named.json()["identity"]["charter_elided"],
        false,
        "the charter is part of the weight this pin measures: {}",
        named.json()["identity"]
    );
    assert!(
        named.json().get("over_the_ceiling").is_none(),
        "the named boot says its floor is over the ceiling: {}",
        named.json()["over_the_ceiling"]
    );
    eprintln!(
        "named boot of a heavy identity (a charter of {CHARTER_CHARS} characters, {STARRED_RULES} \
         starred rules, {HEAVY_BOTS} bots on the roster): {whole} of {budget} characters, margin {}",
        budget.saturating_sub(whole)
    );
    assert!(
        whole + MARGIN <= budget,
        "the named boot is {whole} of {budget} characters, which leaves {} and the pin keeps \
         {MARGIN}",
        budget.saturating_sub(whole),
    );
    story.finish().await;
}
