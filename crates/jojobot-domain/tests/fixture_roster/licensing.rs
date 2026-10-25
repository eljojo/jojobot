use super::*;

/// One run of a block's text that lies on a single line and inside a single
/// sentence.
struct Piece<'a> {
    /// Index into the block's lines.
    line: usize,
    text: &'a str,
    sentence: usize,
}

/// **Every prose line of a block that says a pronoun no name licenses**, as
/// indexes into `lines`.
///
/// **A name licenses the pronouns of its own sentence and of the sentence
/// after it.** A sentence ends at `.`, `?` or `!` followed by white space or the
/// end of the line, and it runs across line breaks, so a comment wrapped over
/// three lines is one sentence. The code of the block is text too: a handle in a
/// call is a name, in the sentence the code line makes. A name does not reach a
/// pronoun two sentences on, which is the reading the block-wide licence gave
/// up. English lets a pronoun follow its name into the next sentence, so the
/// window stops there and does not widen.
///
/// `lines` pairs each line of the block with whether it is prose; only a prose
/// line is reported.
pub(super) fn unlicensed_lines(lines: &[(&str, bool)], names: &[String]) -> Vec<usize> {
    let pieces = pieces_of(lines);
    let sentences = pieces.last().map_or(0, |piece| piece.sentence + 1);
    let names_one: Vec<bool> = (0..sentences)
        .map(|sentence| {
            let text = pieces
                .iter()
                .filter(|piece| piece.sentence == sentence)
                .map(|piece| piece.text)
                .collect::<Vec<_>>()
                .join(" ");
            names_somebody(&text, names)
        })
        .collect();
    let licensed: Vec<bool> = (0..sentences)
        .map(|at| names_one[at] || (at > 0 && names_one[at - 1]))
        .collect();
    let mut reported = Vec::new();
    for piece in &pieces {
        let (_, prose) = lines[piece.line];
        if prose
            && !licensed[piece.sentence]
            && PRONOUNS
                .iter()
                .any(|word| says_word(piece.text, word, false))
            && !reported.contains(&piece.line)
        {
            reported.push(piece.line);
        }
    }
    reported
}

/// Cut a block into [`Piece`]s, numbering sentences from zero.
fn pieces_of<'a>(lines: &[(&'a str, bool)]) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    let mut sentence = 0;
    for (line, (text, _)) in lines.iter().enumerate() {
        let mut from = 0;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (at, &(offset, c)) in chars.iter().enumerate() {
            // The `!` of a `//!` comment marker is not a sentence end.
            let ends = matches!(c, '.' | '?' | '!')
                && (at == 0 || chars[at - 1].1 != '/')
                && chars
                    .get(at + 1)
                    .is_none_or(|&(_, next)| next.is_whitespace());
            if ends {
                let to = offset + c.len_utf8();
                pieces.push(Piece {
                    line,
                    text: &text[from..to],
                    sentence,
                });
                sentence += 1;
                from = to;
            }
        }
        if from < text.len() {
            pieces.push(Piece {
                line,
                text: &text[from..],
                sentence,
            });
        }
    }
    pieces
}

/// **A roster name that is also an ordinary word licenses its own sentence and
/// the next, and no more.** The first sentence below capitalizes "Ghost" as the
/// start of a phrase, and the roster holds a character of that name, so that
/// sentence counts as naming somebody. A pronoun two sentences on is reported
/// all the same: the block no longer licenses it. The control file has the same
/// pronoun without that word and is reported too, which shows the sentence
/// check still reads.
///
/// This was a pinned known limit while a name licensed its whole block. The
/// remaining limit is inside the window: an ordinary word still licenses a
/// pronoun in its own sentence and the next.
#[test]
fn an_ordinary_word_roster_name_does_not_license_a_pronoun_two_sentences_on() {
    let scratch = Scratch::new("ordinary-word-license");
    scratch.write(
        "crates/licensed.rs",
        "// Ghost writing is not what this file does.\n\
         // Nothing else here is written by hand.\n\
         // It is the surface he reads himself.\nfn a() {}\n",
    );
    scratch.write(
        "crates/control.rs",
        "// Typing is what this file does.\n\
         // Nothing else here is written by hand.\n\
         // It is the surface he reads himself.\nfn b() {}\n",
    );

    let unattached = pronouns_for_nobody(&scanned_files(&scratch.0));

    assert!(
        unattached.iter().any(|line| line.contains("control.rs")),
        "the control file has to be reported, or the file below proves nothing: \
         {unattached:?}"
    );
    assert!(
        unattached.iter().any(|line| line.contains("licensed.rs")),
        "an ordinary-word roster name two sentences back must not license this pronoun: \
         {unattached:?}"
    );
}

/// **A name licenses the pronouns of its own sentence and of the sentence after
/// it, and none of the rest of its block.**
///
/// `far.rs` names the character, says something else, and only then says the
/// pronoun for the operator. The other files are the controls: the pronoun
/// shares the name's sentence across a line break, follows it in the next
/// sentence, or sits in a `//!` comment whose marker must not end a sentence.
/// Without them `far.rs` would pass the case for a gate that flagged every
/// pronoun. `code_after.rs` shows the window runs forward only: a handle in the
/// code below a comment does not license the comment.
#[test]
fn a_name_licenses_its_own_sentence_and_the_next_and_not_the_rest_of_its_block() {
    let scratch = Scratch::new("sentence-license");
    scratch.write(
        "crates/far.rs",
        "// Milhouse lent the wrench and never got it back.\n\
         // The operator opens the page.\n\
         // It is the one he reads.\nfn a() {}\n",
    );
    scratch.write(
        "crates/next_sentence.rs",
        "// Milhouse lent the wrench and never got it back.\n\
         // The walk finds his tools.\nfn b() {}\n",
    );
    scratch.write(
        "crates/same_sentence.rs",
        "// The wrench Milhouse lent is not\n\
         // returned until his turn comes.\nfn c() {}\n",
    );
    scratch.write(
        "crates/doc_comment.rs",
        "//! The wrench Milhouse lent is not\n//! returned until his turn comes.\nfn d() {}\n",
    );
    scratch.write(
        "crates/code_after.rs",
        "// The page the operator opens is the one he reads.\n\
         fn e() { let _ = \"person:milhouse\"; }\n",
    );

    let unattached = pronouns_for_nobody(&scanned_files(&scratch.0));

    for file in ["far.rs", "code_after.rs"] {
        assert!(
            unattached.iter().any(|line| line.contains(file)),
            "a name out of the pronoun's reach must not license it, {file}: {unattached:?}"
        );
    }
    for file in ["next_sentence.rs", "same_sentence.rs", "doc_comment.rs"] {
        assert!(
            !unattached.iter().any(|line| line.contains(file)),
            "a name in the pronoun's sentence or the one before licenses it, {file}: \
             {unattached:?}"
        );
    }
}
