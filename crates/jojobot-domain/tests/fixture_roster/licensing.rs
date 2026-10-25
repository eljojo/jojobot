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
