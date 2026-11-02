//! Text served to a page: a mention becomes a link to its page, and text that
//! only looks like a mention does not.

use super::*;
use jojobot_domain::memory::Boot;

pub(super) fn root(id: &str) -> Entity {
    let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
    let id = EntityId(id.to_string());
    Entity {
        folded_names: Vec::new(),
        kind: id.kind().expect("a fixture handle names its kind"),
        name: id.as_str().to_string(),
        id,
        aliases: Vec::new(),
        source: "a test".to_string(),
        crm: None,
        parent: None,
        boot: Boot::default(),
        merged_into: None,
        badge: None,
        archived: None,
    }
}

/// **A mention of a thing on the page becomes a link to the thing's own page,
/// and text shaped like a handle that names nothing is never a link.** Paired
/// in one read: a text that mentions nothing comes back unchanged, so a
/// `linkify` that linked everything, or nothing, fails one of the three.
#[test]
fn a_mention_becomes_a_link_and_a_handle_that_names_nothing_does_not() {
    let alpha = root("person:alpha");
    let handcart = root("thing:handcart");
    let by_id: HashMap<&EntityId, &Entity> = [&alpha, &handcart]
        .map(|e| (&e.id, e))
        .into_iter()
        .collect();

    let page = linkify("@person:alpha rode @thing:handcart", &by_id);
    assert!(
        page.contains("<a href=\"/person:alpha/\">@person:alpha</a>")
            && page.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "{page}"
    );

    let lost = linkify("twin of @thing:handcart-lost (no such handle)", &by_id);
    assert!(
        lost.contains("@thing:handcart-lost (no such handle)") && !lost.contains("<a href"),
        "{lost}"
    );

    assert_eq!(
        linkify("The widget stall runs on Thursdays", &by_id),
        "The widget stall runs on Thursdays"
    );
}

/// **A handle inside a fence that opens on the content is not a link, and a
/// handle in a sentence of another claim still is.** The page reads content and
/// details as one text for what is code, then links each in its own text.
#[test]
fn a_handle_inside_a_fence_across_content_and_details_is_not_a_link() {
    let milhouse = root("person:milhouse");
    let handcart = root("thing:handcart");
    let by_id: HashMap<&EntityId, &Entity> = [&milhouse, &handcart]
        .map(|e| (&e.id, e))
        .into_iter()
        .collect();

    let (content, details) = ("```text", "run @person:milhouse here\n```");
    let (in_content, in_details) = mention::followable_words(content, Some(details));
    let rendered = linkify_spans(details, in_details, &by_id);
    assert!(
        !rendered.contains("<a href") && rendered.contains("run @person:milhouse here"),
        "{rendered}"
    );
    assert!(linkify_spans(content, in_content, &by_id).starts_with("```text"));

    let sentence = linkify("@person:alpha rode @thing:handcart", &by_id);
    assert!(
        sentence.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "{sentence}"
    );
}
