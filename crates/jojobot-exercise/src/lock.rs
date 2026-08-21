//! **A lock: what must be true of the room after a phase, written beside the
//! phase it belongs to.**
//!
//! A lock is a **query** and an **assertion** and a **sentence**, and the three
//! are deliberately different things:
//!
//! * the **query** is jojobot's own vocabulary — the verb and the arguments a
//!   session would send, or the name of a view. Nothing is translated;
//! * the **assertion** is this format's own, and it is three words that do not
//!   branch. Pushing it into jojobot would grow a test framework inside a query
//!   surface, which is worse than the cost it saves;
//! * the **sentence** is authored prose and is never generated. *The pump says
//!   `sent` for where the job has got to* is worth more than a boolean because
//!   somebody wrote it about this room.
//!
//! ```text
//! recall  {"kind": "rhythm", "overdue": {"as_of": "2026-06-14"}}
//! carries thing:floor-pump
//! say     the pump was not offered as due, so months passed and nothing noticed
//! ```
//!
//! # 🚨 The rule this format exists to hold
//!
//! **A lock whose only assertion is `lacks` is refused, at parse time.**
//!
//! A negative alone passes on an empty answer — a store that lost everything
//! satisfies *the wrong thing is not there* perfectly. It is the rule this
//! project repeats more than any other and the one a tired author breaks, so
//! the parser holds it rather than whoever is awake.
//!
//! # The hatch, and it is counted
//!
//! A lock may name a Rust check instead of a query. **The run reports how many
//! did and which**, because an escape nobody counts is an escape everybody
//! takes — and each one is a specific thing jojobot's query surface cannot say,
//! which is a finding rather than a footnote.

use anyhow::{Context, Result, bail};

/// What a lock asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asks {
    /// A verb and its arguments, exactly as a session would send them.
    Query { verb: String, args: String },
    /// **A named Rust check** — the hatch. Counted and reported.
    Check(String),
}

/// What must be so about the answer. **None of these branches**, which is what
/// keeps the vocabulary from becoming a language somebody has to learn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    Carries(String),
    Lacks(String),
    AtLeast(usize, String),
}

/// One lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub asks: Asks,
    pub expects: Vec<Expect>,
    /// The authored sentence a reader sees when this does not hold.
    pub say: String,
}

const OPENS: &str = "```locks";
const FENCE: &str = "```";

/// **Read the locks out of a document.** Blank lines separate one from the next.
pub fn read(document: &str) -> Result<Vec<Lock>> {
    let mut locks = Vec::new();
    for block in blocks(document) {
        for (at, lines) in block {
            locks.push(one(&lines).with_context(|| format!("reading the lock at line {at}"))?);
        }
    }
    Ok(locks)
}

/// Each `locks` block, split into the runs of lines that make one lock.
fn blocks(document: &str) -> Vec<Vec<(usize, Vec<String>)>> {
    let mut all = Vec::new();
    let mut inside = false;
    let mut current: Vec<(usize, Vec<String>)> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut began = 0;
    for (at, line) in document.lines().enumerate() {
        let trimmed = line.trim();
        if !inside && trimmed == OPENS {
            inside = true;
            continue;
        }
        if inside && trimmed.starts_with(FENCE) {
            if !lines.is_empty() {
                current.push((began, std::mem::take(&mut lines)));
            }
            all.push(std::mem::take(&mut current));
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if !lines.is_empty() {
                current.push((began, std::mem::take(&mut lines)));
            }
            continue;
        }
        if lines.is_empty() {
            began = at + 1;
        }
        lines.push(trimmed.to_string());
    }
    all
}

fn one(lines: &[String]) -> Result<Lock> {
    let (verb, rest) = lines[0]
        .split_once(char::is_whitespace)
        .with_context(|| "a lock opens with what it asks: a verb and its arguments, or a check")?;
    let asks = match verb {
        "check" => Asks::Check(rest.trim().to_string()),
        _ => Asks::Query {
            verb: verb.to_string(),
            args: rest.trim().to_string(),
        },
    };
    let mut expects = Vec::new();
    let mut say = None;
    for line in &lines[1..] {
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .with_context(|| format!("{line:?} is not a word and then what it is about"))?;
        let rest = rest.trim().to_string();
        match word {
            "carries" => expects.push(Expect::Carries(rest)),
            "lacks" => expects.push(Expect::Lacks(rest)),
            "at" => {
                let counted = rest
                    .strip_prefix("least ")
                    .with_context(|| "the counting assertion reads `at least 3 of <text>`")?;
                let (how_many, what) = counted
                    .split_once(" of ")
                    .with_context(|| "the counting assertion reads `at least 3 of <text>`")?;
                expects.push(Expect::AtLeast(
                    how_many
                        .trim()
                        .parse()
                        .with_context(|| format!("{how_many:?} is no count"))?,
                    what.trim().to_string(),
                ));
            }
            "say" => say = Some(rest),
            other => bail!(
                "{other:?} asserts nothing — a lock says carries, lacks, at least N of, and say",
            ),
        }
    }
    // **The sentence is what makes a failure readable**, and a lock without one
    // reports a boolean to somebody who then has to open the room to find out
    // what it meant.
    let say = say.context("a lock says what a reader sees when it does not hold: `say …`")?;
    if expects.is_empty() && !matches!(asks, Asks::Check(_)) {
        bail!("a lock that asserts nothing holds always — say what must be so");
    }
    // 🚨 **The rule the format exists to hold.**
    if !expects.is_empty() && expects.iter().all(|e| matches!(e, Expect::Lacks(_))) {
        bail!(
            "this lock only says what is ABSENT, so it passes on an answer that came back empty \
             — a store that lost everything satisfies it perfectly. Say what must be THERE as \
             well, in the same lock",
        );
    }
    Ok(Lock { asks, expects, say })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A lock is a query, an assertion and a sentence**, and the query is
    /// jojobot's own vocabulary with nothing translated.
    #[test]
    fn a_lock_carries_a_query_an_assertion_and_a_sentence() {
        let read = read(
            "```locks\n\
             recall {\"kind\": \"rhythm\", \"overdue\": {\"as_of\": \"2026-06-14\"}}\n\
             carries thing:floor-pump\n\
             say     the pump was not offered as due\n\
             ```\n",
        )
        .expect("the lock reads");
        assert_eq!(read.len(), 1);
        assert_eq!(
            read[0].asks,
            Asks::Query {
                verb: "recall".into(),
                args: "{\"kind\": \"rhythm\", \"overdue\": {\"as_of\": \"2026-06-14\"}}".into(),
            },
            "the query is sent as written, because that is what a session sends",
        );
        assert_eq!(
            read[0].expects,
            vec![Expect::Carries("thing:floor-pump".into())]
        );
        assert_eq!(read[0].say, "the pump was not offered as due");
    }

    /// 🚨 **A lock that only says what is absent is refused, and the refusal
    /// says why.**
    ///
    /// **Both halves in one case**: the bare negative is refused, and the same
    /// negative paired with a positive lands. Without the second, a parser that
    /// refused every lock would pass.
    #[test]
    fn a_lock_that_only_says_what_is_absent_is_refused_and_the_pair_lands() {
        let refused = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             lacks   person:ralph\n\
             say     ralph was invented\n\
             ```\n",
        )
        .expect_err("a negative alone passes on an empty answer");
        let said = format!("{refused:#}");
        assert!(
            said.contains("empty") && said.contains("THERE"),
            "the refusal says why a bare negative is worthless: {said}",
        );

        let paired = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             lacks   person:ralph\n\
             carries person:milhouse\n\
             say     ralph was invented, and milhouse should still be there\n\
             ```\n",
        )
        .expect("a negative with its positive is a real lock");
        assert_eq!(paired[0].expects.len(), 2);
    }

    /// **A lock with no sentence is refused.** A boolean sends a reader to the
    /// room to find out what it meant.
    #[test]
    fn a_lock_with_no_sentence_is_refused() {
        let refused = read("```locks\nrecall {}\ncarries person:milhouse\n```\n")
            .expect_err("a lock says what a reader sees");
        assert!(format!("{refused:#}").contains("does not hold"));
    }

    /// **The hatch is a lock like any other**, so a room reaching for Rust is
    /// visible in the same list rather than somewhere else.
    #[test]
    fn a_lock_may_name_a_rust_check_instead_of_a_query() {
        let read = read(
            "```locks\n\
             check   the_cost_reads_as_a_number\n\
             say     what the pump cost did not normalise\n\
             ```\n",
        )
        .expect("the hatch reads");
        assert_eq!(
            read[0].asks,
            Asks::Check("the_cost_reads_as_a_number".into())
        );
    }

    /// **Blank lines separate locks**, so a phase carrying three is three.
    #[test]
    fn a_block_may_carry_more_than_one_lock() {
        let read = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             carries person:milhouse\n\
             say     milhouse is gone\n\
             \n\
             recall {\"kind\": \"thing\"}\n\
             carries thing:jukebox\n\
             say     the jukebox is gone\n\
             ```\n",
        )
        .expect("both read");
        assert_eq!(read.len(), 2, "two locks, not one run together: {read:?}");
    }
}
