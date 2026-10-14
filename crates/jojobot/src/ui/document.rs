//! **A document's page: the document, rendered from its markdown.**
//!
//! The page of a `document:` thing opens with the document itself, above the
//! table of its records: its name as the heading, its one-liner under it, then
//! its live sections in reading order. A person reads a document on this page,
//! so it has to read like one.
//!
//! **What a person typed never becomes markup.** The markdown is read as a
//! CommonMark event stream and every event is decided here before it is
//! written out:
//!
//! * A raw HTML event is written as the text it is, so it is escaped and never a
//!   tag.
//! * A link goes only to `http`, `https`, `mailto`, an anchor or a relative
//!   path. Any other scheme, `javascript:` first among them, loses the link and
//!   keeps the words.
//! * An image is not drawn. It would make the reader's browser fetch from
//!   whatever host the text names, so the page keeps the alt text and nothing
//!   else.
//! * A handle in running text becomes a link to that thing's page, as it does
//!   everywhere else on the page, and a handle in a code span or a code block
//!   stays the text it is.
//!
//! Headings are written one level lower than they are marked up, since the
//! page's own `h1` is the listing and the document's name is the `h2`, and each
//! gets an `id` so a section can be linked to.

use std::collections::{BTreeMap, HashMap};

use jojobot_domain::memory::{Entity, EntityId};
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, TextMergeStream};

use crate::ui::pages::{escape, linkify};

/// The prefix of every heading's `id`, so none can collide with an id the page
/// itself uses.
const ANCHOR_PREFIX: &str = "sec-";

/// **The block of a document's page**: name, one-liner and the rendered text.
pub fn block(
    name: &str,
    one_liner: Option<&str>,
    markdown: &str,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let mut out = format!("<article id=\"document\">\n<h2>{}</h2>\n", escape(name));
    if let Some(line) = one_liner.map(str::trim).filter(|line| !line.is_empty()) {
        out.push_str(&format!("<p class=\"one-liner\">{}</p>\n", escape(line)));
    }
    if markdown.trim().is_empty() {
        out.push_str("<p>This document has no live sections.</p>\n");
    } else {
        out.push_str("<div class=\"document\">\n");
        out.push_str(&render(markdown, by_id));
        out.push_str("</div>\n");
    }
    out.push_str("</article>\n");
    out
}

/// Markdown to HTML, with every event decided on the way.
pub fn render(markdown: &str, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    // Merged, so a handle that the parser cut into pieces at a markup character
    // is read whole.
    let events: Vec<Event<'_>> = TextMergeStream::new(Parser::new_ext(markdown, options)).collect();

    let mut out = String::with_capacity(markdown.len() * 2);
    pulldown_cmark::html::push_html(&mut out, decided(events, by_id).into_iter());
    out
}

/// What each event becomes. The only events written without escaping are the
/// ones built here, from text that was escaped first.
fn decided<'a>(events: Vec<Event<'a>>, by_id: &HashMap<&EntityId, &Entity>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    // One entry per open link: whether the link was kept.
    let mut links: Vec<bool> = Vec::new();
    let mut in_code_block = false;
    let mut in_metadata = false;
    let mut in_link = false;

    let mut at = 0;
    while at < events.len() {
        let event = events[at].clone();
        at += 1;
        match event {
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Event::Text(raw)),
            Event::Start(Tag::HtmlBlock) => out.push(Event::Start(Tag::Paragraph)),
            Event::End(TagEnd::HtmlBlock) => out.push(Event::End(TagEnd::Paragraph)),
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {}
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let keep = may_link_to(&dest_url);
                links.push(keep);
                if keep {
                    in_link = true;
                    out.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::End(TagEnd::Link) => {
                if links.pop().unwrap_or(false) {
                    out.push(Event::End(TagEnd::Link));
                }
                in_link = links.iter().any(|kept| *kept);
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let id = anchor(&events[at..], &mut taken);
                out.push(Event::Start(Tag::Heading {
                    level: one_lower(level),
                    id: Some(CowStr::from(id)),
                    classes: Vec::new(),
                    attrs: Vec::new(),
                }));
            }
            Event::End(TagEnd::Heading(level)) => {
                out.push(Event::End(TagEnd::Heading(one_lower(level))));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                out.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                out.push(Event::End(TagEnd::CodeBlock));
            }
            // A frontmatter block is not prose: it is shown as the text it is.
            Event::Start(Tag::MetadataBlock(_)) => {
                in_metadata = true;
                out.push(Event::Html(CowStr::from("<pre class=\"frontmatter\">")));
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_metadata = false;
                out.push(Event::Html(CowStr::from("</pre>\n")));
            }
            Event::Text(text) if !in_code_block && !in_metadata && !in_link => {
                if text.contains('@') {
                    out.push(Event::Html(CowStr::from(linkify(&text, by_id))));
                } else {
                    out.push(Event::Text(text));
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Whether a link may go where it says. Browsers drop whitespace and control
/// characters inside a scheme, so they are dropped here before the scheme is
/// read, and a destination with a scheme is allowed only for the three below.
fn may_link_to(destination: &str) -> bool {
    let squeezed: String = destination
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let before_path = squeezed
        .find(['/', '?', '#'])
        .map_or(squeezed.as_str(), |end| &squeezed[..end]);
    match before_path.split_once(':') {
        None => true,
        Some((scheme, _)) => {
            ["http", "https", "mailto"].contains(&scheme.to_ascii_lowercase().as_str())
        }
    }
}

/// The heading one level lower than the one marked up, down to `h6`.
fn one_lower(level: HeadingLevel) -> HeadingLevel {
    match level {
        HeadingLevel::H1 => HeadingLevel::H2,
        HeadingLevel::H2 => HeadingLevel::H3,
        HeadingLevel::H3 => HeadingLevel::H4,
        HeadingLevel::H4 => HeadingLevel::H5,
        HeadingLevel::H5 | HeadingLevel::H6 => HeadingLevel::H6,
    }
}

/// **The `id` of the heading that opens just before `rest`**: its words as a
/// slug, kept apart from every id already given in this document by a number.
fn anchor(rest: &[Event<'_>], taken: &mut BTreeMap<String, usize>) -> String {
    let mut words = String::new();
    for event in rest {
        match event {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(text) | Event::Code(text) => words.push_str(text),
            Event::SoftBreak | Event::HardBreak => words.push(' '),
            _ => {}
        }
    }
    let mut slug = String::new();
    for c in words.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let base = format!(
        "{ANCHOR_PREFIX}{}",
        if slug.is_empty() { "section" } else { slug }
    );
    let seen = taken.entry(base.clone()).or_insert(0);
    *seen += 1;
    if *seen == 1 {
        base
    } else {
        format!("{base}-{seen}")
    }
}
