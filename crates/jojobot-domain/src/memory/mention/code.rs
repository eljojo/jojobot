//! **Where the code is in a text a mention is read from.** Text inside an inline code
//! span or a fenced block is literal (rule 450), so every walk over a mention
//! starts from the ranges this module finds.

/// **Every `@` that is not inside code**, in order.
///
/// Text inside an inline code span or a fenced block is literal (rule 450): a
/// document that shows sample code holds handles that name nothing, and reading
/// them as mentions would refuse the whole write. **Every walk over a mention
/// starts here**, so what is stored, what is served and what a write guard
/// screens cannot disagree about where code is. Replacements this module makes
/// hold no backtick or tilde, so the code in stored text is where the code in
/// written text was.
pub(crate) fn outside_code(text: &str) -> Vec<usize> {
    let code = code_ranges(text);
    text.match_indices('@')
        .map(|(at, _)| at)
        .filter(|at| !within(&code, *at))
        .collect()
}

/// [`outside_code`] for one field of a claim, with the code decided beside the
/// other field.
pub(super) fn outside_code_beside(text: &str, beside: super::Beside<'_>) -> Vec<usize> {
    let code = beside.code_in(text);
    text.match_indices('@')
        .map(|(at, _)| at)
        .filter(|at| !within(&code, *at))
        .collect()
}

/// Whether a position sits inside one of these ranges.
pub(super) fn within(code: &[std::ops::Range<usize>], at: usize) -> bool {
    code.iter().any(|range| range.contains(&at))
}

/// **Where the code is**: fenced blocks first, then inline spans in what is
/// left. Only two markdown shapes are read, and nothing else about markdown is.
///
/// * **A fenced block** opens on a line of three or more backticks or tildes
///   (any indent; a backtick fence may not have a backtick after it) and closes
///   on a line of the same character, at least as long, with nothing after it.
///   **A fence that never closes is not a fence.** Treating the rest of the text
///   as code would drop every link after a stray fence without a word, so it is
///   read as the text it is and the lines after it are read again.
/// * **An inline span** opens on a run of backticks and closes on the next run
///   of the same length, within one paragraph. A run with no partner is literal.
/// * **An indented block is not code.** It cannot be told from the continuation
///   of a list item without reading the list, and skipping a real mention is the
///   worse mistake.
pub(super) fn code_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<(usize, &str)> = {
        let mut at = 0;
        text.split_inclusive('\n')
            .map(|line| {
                let start = at;
                at += line.len();
                (start, line)
            })
            .collect()
    };
    let mut fenced: Vec<std::ops::Range<usize>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((mark, length)) = fence_opening(lines[i].1) else {
            i += 1;
            continue;
        };
        match (i + 1..lines.len()).find(|&j| fence_closes(lines[j].1, mark, length)) {
            Some(j) => {
                fenced.push(lines[i].0..lines[j].0 + lines[j].1.len());
                i = j + 1;
            }
            None => i += 1,
        }
    }
    let mut code = fenced.clone();
    let mut from = 0;
    for range in fenced
        .iter()
        .chain(std::iter::once(&(text.len()..text.len())))
    {
        code.extend(
            inline_spans(&text[from..range.start])
                .into_iter()
                .map(|span| span.start + from..span.end + from),
        );
        from = range.end;
    }
    code
}

/// The fence character and length a line opens a fence with, if it does.
fn fence_opening(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let mark = *trimmed.as_bytes().first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let length = trimmed.bytes().take_while(|b| *b == mark).count();
    let after = &trimmed[length..];
    (length >= 3 && !(mark == b'`' && after.contains('`'))).then_some((mark, length))
}

/// Whether a line closes the fence of this character and length.
fn fence_closes(line: &str, mark: u8, length: usize) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let run = trimmed.bytes().take_while(|b| *b == mark).count();
    run >= length && trimmed[run..].trim().is_empty()
}

/// The inline code spans in a piece of text that holds no fenced block.
fn inline_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let run_at = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let length = run_at(i);
        let limit = paragraph_end(text, i);
        let mut j = i + length;
        let mut closed = None;
        while j < limit {
            if bytes[j] == b'`' {
                let found = run_at(j);
                if found == length {
                    closed = Some(j + found);
                    break;
                }
                j += found;
            } else {
                j += 1;
            }
        }
        match closed {
            Some(end) => {
                spans.push(i..end);
                i = end;
            }
            None => i += length,
        }
    }
    spans
}

/// Where the paragraph that holds `from` ends: the start of the next blank line,
/// or the end of the text.
fn paragraph_end(text: &str, from: usize) -> usize {
    let mut at = from;
    while let Some(newline) = text[at..].find('\n') {
        let line_end = at + newline;
        let rest = &text[line_end + 1..];
        let next = rest.find('\n').map_or(rest.len(), |n| n);
        if rest[..next].trim().is_empty() && line_end + 1 < text.len() {
            return line_end;
        }
        at = line_end + 1;
    }
    text.len()
}
