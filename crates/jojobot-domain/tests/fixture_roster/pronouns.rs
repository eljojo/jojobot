use super::*;

/// **The gendered pronouns of English.** A closed set of words rather than a
/// list of sentences somebody wrote: the words are the language's and cannot
/// go stale, where a catalogue of forbidden phrasings re-embeds what it
/// filters and misses the next author's wording (rule 106).
pub(super) const PRONOUNS: &[&str] = &[
    "he", "him", "his", "she", "her", "hers", "himself", "herself",
];

/// **The one comment that excuses a literal.** On the line above a statement it
/// says the string literals of that statement, up to the line that ends in `;`,
/// are deliberate bad examples, which the gate has to be able to write in order
/// to test itself.
const NEGATIVE_EXAMPLE: &str = "// pronoun-check: negative example";

/// A string literal of a Rust source: the line it opens on, its text with the
/// common escapes read, and whether it is the value of a `content` key or of a
/// key inside a `sets` object.
struct Literal {
    line: usize,
    text: String,
    argument: bool,
}

/// **Every string literal outside a comment.** A literal may run over lines; a
/// comment (block comments nest) and a character literal are skipped, and a raw
/// string is read up to its closing hashes. `\n`, `\t`, `\r`, `\0`, `\\`, `\"` and
/// `\'` are read as the character they write, and a backslash at the end of a line
/// joins it to the next, so a word after an escape is a word.
fn literals_of(source: &str) -> Vec<Literal> {
    let chars: Vec<char> = source.chars().collect();
    let (mut found, mut at, mut line) = (Vec::new(), 0, 1);
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        match (c, next) {
            ('\n', _) => line += 1,
            ('/', Some('/')) => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                continue;
            }
            ('/', Some('*')) => {
                let mut depth = 0;
                while at + 1 < chars.len() {
                    match (chars[at], chars[at + 1]) {
                        ('/', '*') => depth += 1,
                        ('*', '/') => depth -= 1,
                        ('\n', _) => line += 1,
                        _ => {}
                    }
                    at += if depth == 0 { 2 } else { 1 };
                    if depth == 0 {
                        break;
                    }
                }
                continue;
            }
            ('\'', Some(n)) => {
                let closes = if n == '\\' { at + 3 } else { at + 2 };
                if chars.get(closes) == Some(&'\'') {
                    at = closes + 1;
                    continue;
                }
            }
            ('"', _) | ('r', Some('"' | '#')) => {
                let raw = c == 'r';
                let hashes = chars[at + 1..].iter().take_while(|c| **c == '#').count();
                if raw && chars.get(at + 1 + hashes) != Some(&'"') {
                    at += 1;
                    continue;
                }
                let (opened, start) = (line, at);
                at += if raw { hashes + 2 } else { 1 };
                let mut text = String::new();
                while at < chars.len() {
                    let here = chars[at];
                    let closes = here == '"'
                        && (!raw || chars[at + 1..].iter().take(hashes).all(|c| *c == '#'));
                    if closes {
                        at += if raw { hashes } else { 0 };
                        break;
                    }
                    line += usize::from(here == '\n');
                    if !raw && here == '\\' {
                        at += 1;
                        let escaped = chars.get(at).copied().unwrap_or('\\');
                        match escaped {
                            'n' => text.push('\n'),
                            't' => text.push('\t'),
                            'r' => text.push('\r'),
                            '0' => text.push('\0'),
                            '\\' | '"' | '\'' => text.push(escaped),
                            '\n' => {
                                line += 1;
                                while chars.get(at + 1).is_some_and(|c| c.is_whitespace()) {
                                    at += 1;
                                    line += usize::from(chars[at] == '\n');
                                }
                            }
                            other => {
                                text.push('\\');
                                text.push(other);
                            }
                        }
                    } else {
                        text.push(here);
                    }
                    at += 1;
                }
                found.push(Literal {
                    line: opened,
                    text,
                    argument: is_argument(&chars, start),
                });
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// The key written just before `at`, as a bare word or a quoted string.
fn key_before(chars: &[char], mut at: usize) -> String {
    while at > 0 && chars[at - 1].is_whitespace() {
        at -= 1;
    }
    let end = at;
    if at > 0 && chars[at - 1] == '"' {
        let close = at - 1;
        at = close;
        while at > 0 && chars[at - 1] != '"' {
            at -= 1;
        }
        return chars[at..close].iter().collect();
    }
    while at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        at -= 1;
    }
    chars[at..end].iter().collect()
}

/// **Whether the literal opening at `start` is the value of a `content` key, or
/// of a key inside an object under a `sets` key**: the two places a call carries
/// what somebody said. A literal is a value when a `:` stands before it; a key
/// of `content` makes it content, and otherwise the nearest unclosed `{` before
/// it decides, by the key in front of that brace.
fn is_argument(chars: &[char], start: usize) -> bool {
    let mut at = start;
    while at > 0 && chars[at - 1].is_whitespace() {
        at -= 1;
    }
    if at == 0 || chars[at - 1] != ':' || (at >= 2 && chars[at - 2] == ':') {
        return false;
    }
    if key_before(chars, at - 1) == "content" {
        return true;
    }
    let (mut depth, mut back) = (0, at - 1);
    while back > 0 {
        back -= 1;
        match chars[back] {
            '}' => depth += 1,
            '{' if depth > 0 => depth -= 1,
            '{' => {
                let mut colon = back;
                while colon > 0 && chars[colon - 1].is_whitespace() {
                    colon -= 1;
                }
                return colon > 0
                    && chars[colon - 1] == ':'
                    && key_before(chars, colon - 1) == "sets";
            }
            _ => {}
        }
    }
    false
}

/// The line where the function that starts on `at` ends, or `at` itself for a
/// declaration with no body. A body ends at the line that closes it at the
/// indent of the `fn`.
fn function_end(lines: &[&str], at: usize, indent: &str) -> usize {
    let head = (at..lines.len())
        .find(|n| lines[*n].contains('{') || lines[*n].trim_end().ends_with(';'))
        .unwrap_or(lines.len() - 1);
    if !lines[head].contains('{') {
        return head;
    }
    let close = format!("{indent}}}");
    (head..lines.len())
        .find(|n| lines[*n] == close || (*n == head && lines[head].ends_with('}')))
        .unwrap_or(lines.len() - 1)
}

/// **Every `content` or `sets` literal that gives a person a pronoun nobody in
/// it names**, as `path:line: the literal`.
///
/// A literal is testimony when it is the value of a `content` key, or of a key in
/// an object under `sets`, in a function that names testimony provenance, as
/// `Provenance::Testimony` or the string `"testimony"`. Its own text is the whole
/// window: a roster name or a handle inside the literal licenses it, and nothing
/// around it does, so a fixture name beside it licenses nothing. A literal about a
/// bot names it by handle and passes the same way. A literal under
/// [`NEGATIVE_EXAMPLE`] is skipped. Every other literal of such a function, a
/// label or a subject, is not what anybody said and is not read.
///
/// ⚠️ **What it does not reach**: a value a helper passes on, such as
/// `s.undated_fact("person:milhouse", "...")`, or one built before the call and
/// sent by name; a text scan sees a key and its value and not the call that
/// carries them.
fn testimony_literals_for_nobody(files: &[PathBuf]) -> Vec<String> {
    let names = character_names();
    let mut unattached = Vec::new();
    for file in files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
    {
        let text = fs::read_to_string(file).expect("readable source file");
        let lines: Vec<&str> = text.lines().collect();
        let literals = literals_of(&text);
        let mut excused = std::collections::BTreeSet::new();
        for (marker, line) in lines.iter().enumerate() {
            if line.trim() == NEGATIVE_EXAMPLE {
                let statement = &lines[marker + 1..];
                let length = statement.iter().position(|l| l.trim_end().ends_with(';'));
                excused.extend(marker + 2..=marker + 2 + length.unwrap_or(0));
            }
        }
        let mut at = 0;
        while at < lines.len() {
            let trimmed = lines[at].trim_start();
            let is_fn = trimmed.contains("fn ")
                && trimmed
                    .split("fn ")
                    .next()
                    .is_some_and(|before| before.split_whitespace().all(is_fn_qualifier));
            if !is_fn {
                at += 1;
                continue;
            }
            let end = function_end(&lines, at, &lines[at][..lines[at].len() - trimmed.len()]);
            let body = lines[at..=end].join("\n");
            if body.contains("Provenance::Testimony") || body.contains("\"testimony\"") {
                for literal in literals
                    .iter()
                    .filter(|l| l.argument && (at + 1..=end + 1).contains(&l.line))
                {
                    let gendered = PRONOUNS
                        .iter()
                        .any(|word| says_word(&literal.text, word, false));
                    if gendered
                        && !excused.contains(&literal.line)
                        && !names_somebody(&literal.text, &names)
                    {
                        unattached.push(format!(
                            "{}:{}: {}",
                            file.display(),
                            literal.line,
                            literal.text.trim()
                        ));
                    }
                }
            }
            at = end + 1;
        }
    }
    unattached.sort();
    unattached
}

/// A word that may stand before `fn` in an item's head.
fn is_fn_qualifier(word: &str) -> bool {
    word == "async"
        || word == "unsafe"
        || word == "const"
        || word == "pub"
        || (word.starts_with("pub(") && word.ends_with(')'))
}

/// **No content or sets literal of a testimony call gives the user a pronoun.**
/// The bright line writes the operator as a role; a fixture's quotation of what
/// the operator said is where a pronoun slips in, and nearby subject names license
/// it when the whole block is the window.
#[test]
fn no_testimony_literal_in_the_workspace_gives_the_speaker_a_pronoun() {
    let files = scanned_files(&workspace_root());
    assert!(files.len() > 10, "the scan must actually see the workspace");

    let unattached = testimony_literals_for_nobody(&files);
    assert!(
        unattached.is_empty(),
        "these testimony literals give the operator a pronoun. Name the speaker as the \
         operator, or name the character in the literal; a deliberate bad example takes the \
         marker comment above its statement:\n{}",
        unattached.join("\n")
    );
}

/// **Each half, or the pass above proves nothing.** A pronoun alone in a content
/// or sets literal of a testimony function is reported, behind an escape too, and
/// so is one beside a licensing name in ANOTHER literal. A name in the literal, the
/// marker over a statement of several lines, a label that is no content, a function
/// that names no testimony and a function after a bodiless declaration pass.
#[test]
fn testimony_literal_pass_reads_a_pronoun_alone_and_a_named_one_apart() {
    let scratch = Scratch::new("testimony-literals");
    let testimony = "    let p = Provenance::Testimony;\n";
    let sources = [
        (
            "alone",
            format!(
                "fn a() {{\n{testimony}    let _ = json!({{\"content\": \"It is his word.\"}});\n}}\n"
            ),
        ),
        (
            "beside",
            format!(
                "fn b() {{\n{testimony}    let s = json!({{\"subject\": \"person:milhouse\", \"content\": \"It is his word.\"}});\n}}\n"
            ),
        ),
        (
            "escaped",
            format!(
                "fn b() {{\n{testimony}    let _ = json!({{\"content\": \"A line.\\nhe said so.\"}});\n}}\n"
            ),
        ),
        (
            "sets",
            format!(
                "fn b() {{\n{testimony}    let _ = json!({{\"sets\": {{\"note\": \"It is his word.\"}}}});\n}}\n"
            ),
        ),
        (
            "named",
            format!(
                "fn c() {{\n{testimony}    let _ = json!({{\"content\": \"Milhouse said it is his.\"}});\n}}\n"
            ),
        ),
        (
            "marked",
            format!(
                "fn d() {{\n{testimony}    // pronoun-check: negative example\n    let _ = json!({{\n        \"content\": \"It is his word.\",\n    }});\n}}\n"
            ),
        ),
        (
            "label",
            format!("fn e() {{\n{testimony}    let _ = shape(\"what he has of mine\");\n}}\n"),
        ),
        (
            "other",
            "fn f() {\n    let _ = json!({\"content\": \"It is his word.\"});\n}\n".to_string(),
        ),
        (
            "bodiless",
            format!(
                "trait T {{\n    fn a();\n}}\nfn t() {{\n{testimony}}}\nfn c() {{\n    let _ = json!({{\"content\": \"It is his word.\"}});\n}}\n"
            ),
        ),
    ];
    for (file, text) in &sources {
        scratch.write(&format!("crates/{file}.rs"), text);
    }

    let unattached = testimony_literals_for_nobody(&scanned_files(&scratch.0));

    for reported in ["alone.rs", "beside.rs", "escaped.rs", "sets.rs"] {
        assert!(
            unattached.iter().any(|line| line.contains(reported)),
            "{reported} has to be reported, or the others prove nothing: {unattached:?}"
        );
    }
    for passed in [
        "named.rs",
        "marked.rs",
        "label.rs",
        "other.rs",
        "bodiless.rs",
    ] {
        assert!(
            !unattached.iter().any(|line| line.contains(passed)),
            "{passed} must pass, or the pass flags every pronoun: {unattached:?}"
        );
    }
}

/// **The reader of literals keeps each one's line and text across the forms a
/// source writes them in**: a raw string with a quote inside, the quote as a
/// character, a literal over three lines, an escape, a line joined by a
/// backslash, and a nested block comment that hides a literal.
#[test]
fn literals_are_read_with_their_lines_across_raw_strings_chars_and_comments() {
    let source = "let a = r#\"raw \"quoted\" text\"#;\n\
                  let b = '\"';\n\
                  let c = \"one\n two\n three\";\n\
                  /* outer /* inner */ \"hidden\" */\n\
                  let d = \"tab\\tend\\\n     joined\";\n\
                  let e = \"last\";\n";
    let read: Vec<(usize, String)> = literals_of(source)
        .into_iter()
        .map(|literal| (literal.line, literal.text))
        .collect();
    assert_eq!(
        read,
        [
            (1, "raw \"quoted\" text".to_string()),
            (3, "one\n two\n three".to_string()),
            (7, "tab\tendjoined".to_string()),
            (9, "last".to_string()),
        ]
    );
}
