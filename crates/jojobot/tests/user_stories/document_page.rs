//! "I keep a manual in jojobot. I want to open its page and read it, not read a
//! table of its records."
//!
//! **A document's page shows the document.** The name is the heading, the
//! one-liner sits under it, and the live sections follow in reading order,
//! rendered from markdown. The facts table stays below for the record-level
//! view. The page is the operator's window: looking changes nothing a bot sees.
//!
//! **What a person typed into a section never becomes markup on the page.** A
//! raw tag is shown as the text it is, a link that runs a script is not a link,
//! and an image that would make the browser fetch from a stranger's host is not
//! an image. The negatives ride in the same case as the markdown that does
//! render, so a page that rendered nothing could not pass them.

use serde_json::json;

use super::dsl::{Session, Story};

const OPENING_AND_CLOSING: &str = "# Fryer manual\n\nRead this **before** the first shift.\n\n## Opening\n\n- Switch the fryer on\n- Wait for the light\n\n## Closing\n\n1. Drain the oil\n2. Wipe the lid\n";

/// The rendered document on a page: the text between the markers of its block.
/// A page repeats a section's words in its facts table, so an assertion over the
/// whole page cannot tell the rendering from the record.
fn rendered(page: &str) -> &str {
    let open = page
        .find("<article id=\"document\">")
        .unwrap_or_else(|| panic!("the page carries no document block: {page}"));
    let rest = &page[open..];
    let end = rest
        .find("</article>")
        .unwrap_or_else(|| panic!("the document block does not end: {page}"));
    &rest[..end]
}

async fn put(session: &Session, markdown: &str) {
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": markdown}),
        )
        .await;
}

/// **The page reads like a document, and an ordinary thing's page does not.**
#[tokio::test]
async fn a_document_page_shows_its_name_its_one_liner_and_its_sections_as_markdown() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    put(&session, OPENING_AND_CLOSING).await;
    session
        .call(
            "capture",
            json!({
                "subject": "document:homer",
                "content": "what the manual is",
                "provenance": "testimony",
                "sets": {"one_liner": "How to run the fryer"},
            }),
        )
        .await;

    let page = story.page("document:homer").await;
    let page = page.raw();
    let block = rendered(page);

    let name = block
        .find("<h2>Fryer manual</h2>")
        .expect("the name heads the block");
    let one_liner = block
        .find("<p class=\"one-liner\">How to run the fryer</p>")
        .expect("the one-liner sits under the name");
    let opening = block
        .find("<h3 id=\"sec-opening\">Opening</h3>")
        .expect("a heading renders as a heading, with an anchor");
    assert!(name < one_liner && one_liner < opening, "{block}");
    assert!(block.contains("<strong>before</strong>"), "{block}");
    assert!(block.contains("<li>Switch the fryer on</li>"), "{block}");
    assert!(block.contains("<ol>"), "{block}");

    // The facts table stays below the document, for the record-level view.
    let facts = page.find("<h2>Facts</h2>").expect("the facts table stays");
    assert!(
        page.find("</article>").expect("the block ends") < facts,
        "{page}"
    );

    // The negative: a claim on an ordinary thing is not markdown, so its words
    // stay the words and the page carries no document block.
    session.add("thing:kettle", "Kettle").await;
    session.fact("thing:kettle", "# not a heading").await;
    let ordinary = story.page("thing:kettle").await;
    ordinary.says("# not a heading");
    ordinary.never_says("<article id=\"document\">");
}

/// **A raw tag in a section is shown as text, and the markdown beside it still
/// renders.**
#[tokio::test]
async fn a_raw_tag_in_a_section_is_text_and_the_markdown_beside_it_renders() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    put(
        &session,
        "## Notes\n\n<script>alert(1)</script>\n\nSome <b onclick=\"x()\">inline</b> html and **bold**.\n\n<div class=\"x\">block</div>\n",
    )
    .await;

    let page = story.page("document:homer").await;
    let block = rendered(page.raw());
    assert!(
        block.contains("<strong>bold</strong>"),
        "markdown renders: {block}"
    );
    assert!(
        block.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "the tag is shown as the text it is: {block}"
    );
    for tag in ["<script", "<b onclick", "<div class=\"x\">"] {
        assert!(
            !block.contains(tag),
            "{tag} reached the page as a tag: {block}"
        );
    }
}

/// **A handle in a paragraph is a link, and the same handle in code is not.**
#[tokio::test]
async fn a_handle_in_a_paragraph_is_a_link_and_the_same_handle_in_code_is_not() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("thing:kettle", "Kettle").await;
    put(
        &session,
        "## Links\n\nSee @thing:kettle for details, and `@thing:kettle` stays code.\n\n```\n@thing:kettle in a fence\n```\n",
    )
    .await;

    let page = story.page("document:homer").await;
    let block = rendered(page.raw());
    assert_eq!(
        block.matches(">@thing:kettle</a>").count(),
        1,
        "only the handle in the paragraph is a link: {block}"
    );
    assert!(block.contains("<code>@thing:kettle</code>"), "{block}");
    assert!(
        block.contains("<pre><code>@thing:kettle in a fence"),
        "{block}"
    );
}

/// **Sections read in the order they were put, and a moved one reads where it
/// was moved.**
#[tokio::test]
async fn sections_read_in_put_order_and_a_moved_section_reads_where_it_went() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    put(
        &session,
        "## Alpha\n\nfirst words\n\n## Beta\n\nsecond words\n\n## Gamma\n\nthird words\n",
    )
    .await;

    let place = |block: &str, word: &str| {
        block
            .find(word)
            .unwrap_or_else(|| panic!("{word} is on the page: {block}"))
    };
    let page = story.page("document:homer").await;
    let block = rendered(page.raw());
    assert!(
        place(block, "first words") < place(block, "second words")
            && place(block, "second words") < place(block, "third words"),
        "put order: {block}"
    );

    let facts = session.recall("document:homer").await.json();
    let address_of = |n: usize| {
        facts["objects"][0]["facts"][n]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string()
    };
    session
        .call(
            "update_fact",
            json!({"address": address_of(0), "after": address_of(2)}),
        )
        .await;

    let moved = story.page("document:homer").await;
    let block = rendered(moved.raw());
    assert!(
        place(block, "second words") < place(block, "third words")
            && place(block, "third words") < place(block, "first words"),
        "the first section was moved behind the last: {block}"
    );
}

/// **A link that runs a script is not a link, an image is not fetched, and an
/// ordinary link and an anchor stay links.**
#[tokio::test]
async fn a_link_that_runs_a_script_and_an_image_do_not_reach_the_page() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    put(
        &session,
        "## Targets\n\n[bad](javascript:alert(1)) and [good](https://example.org/path) and ![pic](http://tracker.example/p.png) and [anchor](#sec-targets)\n",
    )
    .await;

    let page = story.page("document:homer").await;
    let block = rendered(page.raw());
    assert!(
        block.contains("href=\"https://example.org/path\""),
        "{block}"
    );
    assert!(block.contains("href=\"#sec-targets\""), "{block}");
    assert!(block.contains(">good</a>"), "{block}");
    assert!(
        block.contains("bad"),
        "the words of a refused link stay: {block}"
    );
    assert!(block.contains("pic"), "the words of an image stay: {block}");
    for refused in ["javascript:", "<img", "tracker.example"] {
        assert!(
            !block.contains(refused),
            "{refused} reached the page: {block}"
        );
    }
}

/// **Two headings of one wording get two anchors, and a frontmatter block is
/// shown as the text it is.**
#[tokio::test]
async fn two_headings_of_one_wording_get_two_anchors_and_frontmatter_is_not_prose() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    put(
        &session,
        "---\ntitle: Fryer & oil\n---\n\n## Notes\n\nfirst\n\n## Notes\n\nsecond\n",
    )
    .await;

    let page = story.page("document:homer").await;
    let block = rendered(page.raw());
    assert!(block.contains("<h3 id=\"sec-notes\">Notes</h3>"), "{block}");
    assert!(
        block.contains("<h3 id=\"sec-notes-2\">Notes</h3>"),
        "{block}"
    );
    assert!(
        block.contains("<pre class=\"frontmatter\">title: Fryer &amp; oil"),
        "the frontmatter is text in a block of its own: {block}"
    );
    assert!(
        !block.contains("<hr"),
        "it is not a rule between two parts: {block}"
    );
}
