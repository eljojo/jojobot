//! **A Down double forwards every port method it does not mean to fail.** A double
//! that injects one failure has to behave like the real store everywhere else:
//! otherwise a case built on it passes because a method silently returned the
//! trait's default body, and it tests that default and not the store. The route
//! chosen is a scan of the sources, because the trait gives a body to the methods
//! that a compile-time route would have to make required, and that is a change to
//! production. The scan names each method a double leaves out.
//!
//! The scanner reads text, so its own cases hold it: a double that leaves a default
//! out is reported, and one that implements it is not.

/// The source of the Memory port and of the doubles it is held against.
const MEMORY_PORT: &str = include_str!("../../../../jojobot-domain/src/memory.rs");
const DOWN_MEMORY: &str = include_str!("down_memory.rs");
/// The source of the Mailboxes port and of its Down double.
const MAILBOX_PORT: &str = include_str!("../../../../jojobot-domain/src/mailbox.rs");
const DOWN_MAILBOXES: &str = include_str!("../../mailboxes/testing.rs");
/// The source of the memory doubles that wrap a store and fail one thing.
const MEMORY_DOUBLES: &str = include_str!("fold_behind.rs");

/// **The text with comments, strings and character literals blanked**, so a brace
/// or a word inside any of them is not read as code. Length and lines are kept.
fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        if c == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                out.push(' ');
                at += 1;
            }
        } else if c == '/' && next == Some('*') {
            while at < chars.len() && !(chars[at] == '*' && chars.get(at + 1) == Some(&'/')) {
                out.push(if chars[at] == '\n' { '\n' } else { ' ' });
                at += 1;
            }
            out.push_str("  ");
            at += 2;
        } else if c == '"' {
            out.push(' ');
            at += 1;
            while at < chars.len() && chars[at] != '"' {
                if chars[at] == '\\' {
                    out.push(' ');
                    at += 1;
                }
                out.push(if chars.get(at) == Some(&'\n') {
                    '\n'
                } else {
                    ' '
                });
                at += 1;
            }
            out.push(' ');
            at += 1;
        } else if c == '\'' && chars.get(at + 2) == Some(&'\'') {
            out.push_str("   ");
            at += 3;
        } else if c == '\'' && next == Some('\\') {
            while at < chars.len() && chars[at] != '\'' || at == 0 {
                out.push(' ');
                at += 1;
                if chars.get(at) == Some(&'\'') && chars.get(at - 1) != Some(&'\\') {
                    break;
                }
            }
            out.push(' ');
            at += 1;
        } else {
            out.push(c);
            at += 1;
        }
    }
    out
}

/// **The body of the first item that starts with `header`**: the text between its
/// braces.
fn body_of<'a>(code: &'a str, header: &str) -> &'a str {
    let start = code
        .find(header)
        .unwrap_or_else(|| panic!("the source has no `{header}`"));
    let open = start + code[start..].find('{').expect("the item has a body");
    let mut depth = 0usize;
    for (offset, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &code[open + 1..open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("`{header}` never closes");
}

/// **The methods declared at the top level of a body**, each with whether it has a
/// body of its own (a default, in a trait).
fn methods(body: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut depth = 0usize;
    let bytes: Vec<char> = body.chars().collect();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            'f' if depth == 0
                && body[body.char_indices().nth(at).map_or(0, |(i, _)| i)..].starts_with("fn ")
                && (at == 0 || !(bytes[at - 1].is_alphanumeric() || bytes[at - 1] == '_')) =>
            {
                let rest: String = bytes[at + 3..].iter().collect();
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                // The signature ends at a `;` or a `{` outside the parentheses.
                let mut parens = 0i32;
                let mut has_body = false;
                for c in rest.chars() {
                    match c {
                        '(' | '<' | '[' => parens += 1,
                        ')' | '>' | ']' => parens -= 1,
                        ';' if parens <= 0 => break,
                        '{' if parens <= 0 => {
                            has_body = true;
                            break;
                        }
                        _ => {}
                    }
                }
                found.push((name, has_body));
                at += 3;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// **The methods of `trait_header` that have a default body and that the impl
/// under `impl_header` does not implement.**
fn left_to_the_default(
    port: &str,
    trait_header: &str,
    double: &str,
    impl_header: &str,
) -> Vec<String> {
    let port = code_only(port);
    let double = code_only(double);
    let defaults: Vec<String> = methods(body_of(&port, trait_header))
        .into_iter()
        .filter_map(|(name, has_body)| has_body.then_some(name))
        .collect();
    assert!(
        !defaults.is_empty(),
        "the scan found no default-bodied methods in `{trait_header}`, so it reads nothing"
    );
    let implemented: Vec<String> = methods(body_of(&double, impl_header))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        !implemented.is_empty(),
        "the scan found no methods in `{impl_header}`, so it reads nothing"
    );
    defaults
        .into_iter()
        .filter(|name| !implemented.contains(name))
        .collect()
}

#[test]
fn the_down_memory_forwards_every_default_bodied_method() {
    let left = left_to_the_default(
        MEMORY_PORT,
        "pub trait Memory: Send + Sync",
        DOWN_MEMORY,
        "impl Memory for DownMemory",
    );
    assert!(
        left.is_empty(),
        "DownMemory leaves these methods to the trait's default body, so a case built on it \
         tests the default and not the store: {left:?}"
    );
}

#[test]
fn the_down_mailboxes_answers_every_default_bodied_method() {
    let left = left_to_the_default(
        MAILBOX_PORT,
        "pub trait Mailboxes: Send + Sync",
        DOWN_MAILBOXES,
        "impl mailbox::Mailboxes for DownMailboxes",
    );
    assert!(
        left.is_empty(),
        "DownMailboxes leaves these methods to the trait's default body: {left:?}"
    );
}

/// **The doubles that wrap a store are held to the same rule as the Down ones.**
/// Each is meant to behave like the store beneath it apart from its one injected
/// difference, so a default left alone is a difference nobody meant.
#[test]
fn the_wrapping_doubles_forward_every_default_bodied_method() {
    let mut left = Vec::new();
    for (port, header, double, impl_header) in [
        (
            MEMORY_PORT,
            "pub trait Memory: Send + Sync",
            MEMORY_DOUBLES,
            "impl Memory for FoldBehindMemory",
        ),
        (
            MAILBOX_PORT,
            "pub trait Mailboxes: Send + Sync",
            DOWN_MAILBOXES,
            "impl mailbox::Mailboxes for UnopenableMailboxes",
        ),
        (
            MAILBOX_PORT,
            "pub trait Mailboxes: Send + Sync",
            DOWN_MAILBOXES,
            "impl mailbox::Mailboxes for CountingMailboxes",
        ),
        (
            MAILBOX_PORT,
            "pub trait Mailboxes: Send + Sync",
            DOWN_MAILBOXES,
            "impl mailbox::Mailboxes for RenamingMailboxes",
        ),
    ] {
        for name in left_to_the_default(port, header, double, impl_header) {
            left.push(format!("{impl_header}: {name}"));
        }
    }
    assert!(
        left.is_empty(),
        "these doubles leave a method to the trait's default body: {left:#?}"
    );
}

/// **The scan itself.** A double that leaves a default out is reported by name; one
/// that implements it is not; a brace or a word inside a string or a comment is not
/// read as code. Paired in one case so a scan that reported everything, or nothing,
/// would not pass.
#[test]
fn the_scan_names_a_default_a_double_leaves_out_and_only_that() {
    let port = r#"
        pub trait Port {
            // fn commented() { }
            async fn required(&self) -> u8;
            async fn kept(&self) -> String { "} fn in_a_string() {".to_string() }
            fn left_out(&self) -> Option<u64> { None }
        }
    "#;
    let leaves = "impl Port for Double { async fn required(&self) -> u8 { 1 } async fn kept(&self) -> String { String::new() } }";
    let forwards = "impl Port for Double { async fn required(&self) -> u8 { 1 } async fn kept(&self) -> String { String::new() } fn left_out(&self) -> Option<u64> { None } }";
    assert_eq!(
        left_to_the_default(port, "pub trait Port", leaves, "impl Port for Double"),
        ["left_out"]
    );
    assert!(
        left_to_the_default(port, "pub trait Port", forwards, "impl Port for Double").is_empty()
    );
}
