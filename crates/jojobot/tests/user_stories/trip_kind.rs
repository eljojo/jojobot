//! "I planned a trip. Is a trip a thing jojobot knows, or a type somebody declared?"
//!
//! **A trip is a kind the software ships, as a rhythm is.** Its four keys belong
//! to the kind, so a thing filed as `trip:` carries them from its first write,
//! and no type called `trip` is declared on the instance. The two type
//! questions still work over it: a trip with all four keys FITS, a trip with the
//! two places only ANSWERS and names the dates it lacks. A key the kind asks for
//! is required to fit, never to write.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_trip_is_a_kind_whose_keys_decide_which_trips_fit() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("place:springfield", "Springfield").await;
    s.add("place:shelbyville", "Shelbyville").await;

    // ── ruler 1: on a fresh store the kind names its four keys, before any trip ──
    let asked = s
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    let described = asked["type_keys"]
        .as_array()
        .and_then(|types| types.iter().find(|t| t["name"] == "trip"))
        .unwrap_or_else(|| panic!("the answer describes the trip keys: {asked}"));
    let named: Vec<&str> = described["fields"]
        .as_array()
        .expect("the keys are a list")
        .iter()
        .filter_map(|k| k["key"].as_str())
        .collect();
    for key in ["departs_from", "arrives_at", "leaves_on", "returns_on"] {
        assert!(named.contains(&key), "{key} is named in {described}");
    }

    // ── a trip is filed as a trip, the way a rhythm is filed as a rhythm ──────
    s.add("trip:springfield-1999", "The Springfield trip").await;
    s.add("trip:shelbyville-2001", "The Shelbyville trip").await;
    s.event_with(
        "trip:springfield-1999",
        "four days there and back",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:shelbyville",
            "leaves_on": "2026-09-03",
            "returns_on": "2026-09-07",
        }),
        &[],
    )
    .await;
    // **Required to fit, never to write**: two places and no dates is accepted.
    s.event_with(
        "trip:shelbyville-2001",
        "we are going, at some point",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:shelbyville",
        }),
        &[],
    )
    .await;

    // ── ruler 2: the full trip fits; the half-planned one answers and lacks ────
    let described = s
        .call("search", json!({ "answers_type": "trip", "limit": 50 }))
        .await;
    described.says("trip:springfield-1999");
    described.says("trip:shelbyville-2001");
    described.says("\"lacking\":[\"leaves_on\",\"returns_on\"]");
    let fits = s
        .call("search", json!({ "fits_type": "trip", "limit": 50 }))
        .await;
    fits.says("trip:springfield-1999");
    fits.never_says("trip:shelbyville-2001");

    s.wrap("filed two trips as a kind the software ships").await;
    story.finish().await;
}

/// **The name belongs to the kind.** A caller cannot declare a type called
/// `trip`, and the refusal says the call must change, so sending it again is
/// not the next step. The same declaration under another name is not refused,
/// so the name is the whole cause.
#[tokio::test]
async fn declaring_a_type_called_trip_is_refused_and_the_same_fields_under_another_name_are_not() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let fields = json!([{ "key": "cadence_km", "holds": "number" }]);
    let refused = s
        .refused("declare_type", json!({ "name": "trip", "fields": fields }))
        .await;
    refused.holds_at("/fix_by", "change");
    // Paired: the same fields under a name of a caller's own are not refused.
    let declared = s
        .call(
            "declare_type",
            json!({ "name": "journey", "fields": fields }),
        )
        .await;
    declared.says("journey");
    s.wrap("tried to declare the name the software owns").await;
    story.finish().await;
}
