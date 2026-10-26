use super::*;

/// **The gendered pronouns of English.** A closed set of words rather than a
/// list of sentences somebody wrote: the words are the language's and cannot
/// go stale, where a catalogue of forbidden phrasings re-embeds what it
/// filters and misses the next author's wording (rule 106).
pub(super) const PRONOUNS: &[&str] = &[
    "he", "him", "his", "she", "her", "hers", "himself", "herself",
];

/// **The one comment that excuses a literal.** On the line above a statement it
/// says the string literals of that statement, up to its `;`, are deliberate bad
/// examples, which the gate has to be able to write in order to test itself.
const NEGATIVE_EXAMPLE: &str = "// pronoun-check: negative example";

/// A string literal of a Rust source: the line it opens on and its text.
struct Literal {
    line: usize,
    text: String,
}

/// **Every string literal outside a comment**, as the line it opens on and its
/// text. A literal may run over lines. A comment and a character literal are
/// skipped, and a raw string is read up to its closing hashes.
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
                while at + 1 < chars.len() && !(chars[at] == '*' && chars[at + 1] == '/') {
                    line += usize::from(chars[at] == '\n');
                    at += 1;
                }
                at += 2;
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
                let opened = line;
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
                    if !raw && here == '\\' {
                        text.push(here);
                        at += 1;
                    }
                    if let Some(taken) = chars.get(at) {
                        line += usize::from(*taken == '\n');
                        text.push(*taken);
                    }
                    at += 1;
                }
                found.push(Literal { line: opened, text });
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// **Every string literal that gives a person a pronoun nobody in it names**,
/// as `path:line: the literal`.
///
/// A literal is testimony when it sits in a function that names testimony
/// provenance, as `Provenance::Testimony` or the string `"testimony"`; such a
/// function writes what the operator said. Its own text is the whole window: a
/// roster name or a handle inside the literal licenses it, and nothing around it
/// does, so a fixture name beside it licenses nothing. A literal about a bot
/// names it by handle and passes the same way. A literal under
/// [`NEGATIVE_EXAMPLE`] is skipped.
///
/// ⚠️ **It reads every literal of such a function, not only the content and sets
/// arguments of a call**: a text scan cannot tell an argument's position, and a
/// pronoun in any literal there is worth a name.
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
            let close = format!("{}}}", &lines[at][..lines[at].len() - trimmed.len()]);
            let end = (at..lines.len())
                .find(|n| lines[*n] == close || (*n == at && lines[at].ends_with('}')))
                .unwrap_or(lines.len() - 1);
            let body = lines[at..=end].join("\n");
            if body.contains("Provenance::Testimony") || body.contains("\"testimony\"") {
                for literal in literals
                    .iter()
                    .filter(|l| (at + 1..=end + 1).contains(&l.line))
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

/// **No string literal of a testimony function gives the user a pronoun.** The
/// bright line writes the operator as a role; a fixture's quotation of what the
/// operator said is where a pronoun slips in, and nearby subject names license it
/// when the whole block is the window.
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

/// **Each half, or the pass above proves nothing.** A pronoun alone in a testimony
/// literal is reported, and so is one beside a licensing name in ANOTHER literal;
/// a name in the literal, the marker and a function that names no testimony pass.
#[test]
fn testimony_literal_pass_reads_a_pronoun_alone_and_a_named_one_apart() {
    let scratch = Scratch::new("testimony-literals");
    // pronoun-check: negative example
    let alone =
        "fn a() {\n    let p = Provenance::Testimony;\n    let _ = \"It is his word.\";\n}\n";
    // pronoun-check: negative example
    let beside = "fn b() {\n    let p = \"testimony\";\n    let s = \"person:milhouse\";\n    let _ = \"It is his word.\";\n}\n";
    // pronoun-check: negative example
    let named = "fn c() {\n    let p = Provenance::Testimony;\n    let _ = \"Milhouse said it is his.\";\n}\n";
    // pronoun-check: negative example
    let marked = "fn d() {\n    let p = Provenance::Testimony;\n    // pronoun-check: negative example\n    let _ = \"It is his word.\";\n}\n";
    // pronoun-check: negative example
    let other = "fn e() {\n    let _ = \"It is his word.\";\n}\n";
    for (file, text) in [
        ("alone", alone),
        ("beside", beside),
        ("named", named),
        ("marked", marked),
        ("other", other),
    ] {
        scratch.write(&format!("crates/{file}.rs"), text);
    }

    let unattached = testimony_literals_for_nobody(&scanned_files(&scratch.0));

    for reported in ["alone.rs", "beside.rs"] {
        assert!(
            unattached.iter().any(|line| line.contains(reported)),
            "{reported} has to be reported, or the others prove nothing: {unattached:?}"
        );
    }
    for passed in ["named.rs", "marked.rs", "other.rs"] {
        assert!(
            !unattached.iter().any(|line| line.contains(passed)),
            "{passed} must pass, or the pass flags every pronoun: {unattached:?}"
        );
    }
}
