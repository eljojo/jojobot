//! **The world a room is furnished with, read out of the room's own
//! document.**
//!
//! It was Rust — three hundred lines a room, in a module beside a markdown
//! brief that could not carry it, so the story and the world it happens in had
//! to be kept in agreement by hand. A year is not two phases and that shape
//! does not survive the scale: writing a year would mean writing a program.
//!
//! **So the world is data, in the document, beside the story.** No new
//! vocabulary — the five verbs [`Seed`] already has, written down instead of
//! called.
//!
//! # The syntax, and it is deliberately almost nothing
//!
//! A fenced `world` block. One item per line: a verb, then its fields
//! separated by `|`. **A pipe rather than quoting** because every field except
//! the last is a handle or a token, the last is free prose, and a rule about
//! quotes is a rule somebody has to learn.
//!
//! ```text
//! entity  thing:jukebox | The Jukebox
//! child   thing:jukebox | rhythm:deep-clean | Deep Clean The Jukebox
//! fact    person:milhouse | testimony | prefers the early sitting
//! record  person:milhouse | {"donuts": "3"} | ate three donuts
//! message dev | The brief | what somebody left waiting
//! ```
//!
//! **A handle carries its kind**, which is how a caller addresses one
//! everywhere else, so this reads the way the surface reads.
//!
//! **Anything this cannot express stays in Rust**, and a room that reaches for
//! that says so where a reader can count it — see the hatch in the lock
//! vocabulary. **An escape nobody counts is an escape everybody takes.**

use anyhow::{Context, Result, bail};

use crate::surface::Seed;

/// What opens and closes the block the world is written in.
const FENCE: &str = "```";
const OPENS: &str = "```world";

/// **Read the world out of a document**, or an empty one when it declares none.
///
/// A room with no world block is furnished with nothing, which is a room a
/// story may want: the occupant arrives to an empty jojobot and everything it
/// meets is what it wrote itself.
pub fn read(document: &str) -> Result<Seed> {
    let mut seed = Seed::new();
    for (at, line) in lines_of(document).into_iter() {
        seed = item(seed, &line)
            .with_context(|| format!("reading the world at line {at}: {line:?}"))?;
    }
    Ok(seed)
}

/// The lines inside the world block, with their line numbers so a refusal can
/// name where the mistake is rather than only what it was.
fn lines_of(document: &str) -> Vec<(usize, String)> {
    let mut inside = false;
    let mut found: Vec<(usize, String)> = Vec::new();
    for (at, line) in document.lines().enumerate() {
        let trimmed = line.trim();
        if !inside && trimmed == OPENS {
            inside = true;
            continue;
        }
        if inside && trimmed.starts_with(FENCE) {
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        // **An indented line continues the one above it.** A message body is
        // prose with paragraphs in it — it is where a room's whole goal lives —
        // and a format that could only carry one line could not furnish a real
        // room at all. Found by converting one, which is why the conversion
        // came before the year.
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, carried)) = found.last_mut() {
                carried.push('\n');
                carried.push_str(trimmed);
                continue;
            }
        }
        // A blank line inside a body is a paragraph break and belongs to it.
        if trimmed.is_empty() {
            if let Some((_, carried)) = found.last_mut()
                && carried.contains('\n')
            {
                carried.push('\n');
            }
            continue;
        }
        // A `#` line is a note to whoever maintains the room.
        if !trimmed.starts_with('#') {
            found.push((at + 1, trimmed.to_string()));
        }
    }
    found
}

/// One item of furniture.
fn item(seed: Seed, line: &str) -> Result<Seed> {
    let (verb, rest) = line
        .split_once(char::is_whitespace)
        .with_context(|| "a line of the world is a verb and then its fields")?;
    let fields: Vec<&str> = rest.split('|').map(str::trim).collect();
    let handle = |at: usize| -> Result<(String, String)> {
        let whole = fields
            .get(at)
            .copied()
            .filter(|f| !f.is_empty())
            .with_context(|| format!("{verb} wants a handle in field {}", at + 1))?;
        let (kind, slug) = whole.split_once(':').with_context(|| {
            format!("{whole:?} is no handle — a handle carries its kind, as `thing:jukebox` does")
        })?;
        Ok((kind.to_string(), slug.to_string()))
    };
    let text = |at: usize| -> Result<String> {
        fields
            .get(at)
            .copied()
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .with_context(|| format!("{verb} wants something in field {}", at + 1))
    };
    match verb {
        "entity" => {
            let (kind, slug) = handle(0)?;
            seed.entity(&kind, &slug, &text(1)?)
        }
        "child" => {
            let parent = text(0)?;
            let (kind, slug) = handle(1)?;
            seed.child(&parent, &kind, &slug, &text(2)?)
        }
        "fact" => seed.fact(&text(0)?, &text(2)?, &text(1)?),
        "record" => {
            let fields = serde_json::from_str(&text(1)?)
                .with_context(|| "a record's keys are json, as `{\"donuts\": \"3\"}`")?;
            seed.record(&text(0)?, &text(2)?, fields)
        }
        "message" => Ok(seed.message(&text(0)?, &text(1)?, &text(2)?)),
        // **Named rather than ignored.** A line nobody reads is furniture the
        // author believes is there, and a room measured against a world it
        // does not have fails somewhere else entirely.
        other => bail!(
            "{other:?} furnishes nothing — the world is written in entity, child, fact, record \
             and message",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every verb the world has, out of a document.**
    ///
    /// One case rather than five, because what is being asserted is that the
    /// block reaches the builder at all — and a case per verb would pass
    /// individually on a reader that stopped after the first line.
    #[test]
    fn a_document_furnishes_a_room_in_the_five_verbs_it_already_had() {
        let read = read(
            "prose a maintainer reads\n\
             \n\
             ```world\n\
             # a note, and it furnishes nothing\n\
             entity  thing:jukebox | The Jukebox\n\
             child   thing:jukebox | rhythm:deep-clean | Deep Clean The Jukebox\n\
             fact    thing:jukebox | testimony | it sticks on the third track\n\
             record  thing:jukebox | {\"plays\": \"40\"} | played all evening\n\
             message dev | The brief | what somebody left waiting\n\
             ```\n\
             \n\
             more prose\n",
        )
        .expect("the world reads");
        assert_eq!(
            read.len(),
            5,
            "every line of the block furnished something: {read:?}",
        );
    }

    /// **A message body is prose with paragraphs in it**, and that is where a
    /// room's whole goal lives.
    ///
    /// Found by converting a real room: the first thing the format could not
    /// carry was the one field that matters most. **Both halves** — the body
    /// keeps its paragraphs, and the line after it is still its own item rather
    /// than swallowed.
    #[test]
    fn a_message_body_may_be_prose_with_paragraphs_and_the_next_item_survives() {
        let read = read(
            "```world\n\
             message dev | The brief | I have been writing down the jobs.\n\
             \n\
             \x20   And I want to ask which are still owing.\n\
             entity thing:jukebox | The Jukebox\n\
             ```\n",
        )
        .expect("the world reads");
        assert_eq!(
            read.len(),
            2,
            "the body is one item and the entity after it is another: {read:?}",
        );
        let body = format!("{read:?}");
        assert!(
            body.contains("still owing"),
            "the continued line is in the body: {body}",
        );
    }

    /// **A document with no world block furnishes nothing**, and that is a room
    /// somebody may want rather than a mistake.
    ///
    /// The positive the case above rests on: without it, a reader that returned
    /// five of anything would pass.
    #[test]
    fn a_document_with_no_world_block_furnishes_nothing() {
        assert_eq!(read("just prose, no block\n").expect("reads").len(), 0);
    }

    /// **A verb nobody implements is refused, naming itself.**
    ///
    /// A line silently ignored is furniture the author believes is in the room,
    /// and the run then fails somewhere else entirely — against a world that
    /// was never built.
    #[test]
    fn a_line_that_furnishes_nothing_is_refused_rather_than_skipped() {
        let refused = read("```world\nfurnish thing:jukebox | The Jukebox\n```\n")
            .expect_err("a line nobody reads is refused");
        let said = format!("{refused:#}");
        assert!(
            said.contains("furnish") && said.contains("line 2"),
            "the refusal names the word and where it is: {said}",
        );
    }

    /// **A handle without its kind is refused**, because everywhere else on
    /// this surface a handle carries one and a bare slug would furnish a thing
    /// under a kind nobody named.
    #[test]
    fn a_handle_without_its_kind_is_refused() {
        let refused =
            read("```world\nentity jukebox | The Jukebox\n```\n").expect_err("no kind, no handle");
        assert!(
            format!("{refused:#}").contains("carries its kind"),
            "the refusal says what a handle is: {refused:#}",
        );
    }
}
