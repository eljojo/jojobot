//! February's holder lock counts links on the skate tool, not anywhere in the world.

use serde_json::json;

/// One boundary's world: the entity list, a newline, then the search answer.
fn world(links: &[(&str, &str)]) -> String {
    let hits: Vec<_> = links
        .iter()
        .map(|(address, subject)| {
            json!({"hit": "fact", "status": "active", "address": address,
                   "parent": {"id": subject},
                   "edge": {"type": "connection", "object": "person:ralph"}})
        })
        .collect();
    format!("{{}}\n{}", json!({"results": hits}))
}

/// **A sitting that points at the holder from something else has not said who
/// had the skate tool.** The same person is the object of a link on the
/// skateboard, and the lock must not take that for the answer.
#[test]
fn a_link_at_the_holder_from_another_thing_is_not_the_holder_recorded() {
    let before = world(&[]);
    let elsewhere = world(&[("thing:skateboard#f3", "thing:skateboard")]);
    let on_the_tool = world(&[("thing:skate-tool#f2", "thing:skate-tool")]);

    assert!(
        super::holder_recorded(&before, &elsewhere).is_err(),
        "a link at the holder from the skateboard held the lock about the skate tool"
    );
    assert!(
        super::holder_recorded(&before, &on_the_tool).is_ok(),
        "a link at the holder on the skate tool did not hold the lock"
    );
}
