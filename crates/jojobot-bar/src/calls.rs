//! **A person's judgement of a red, kept beside the runs it is about.**
//!
//! A test that failed is later called a flake (it passes on the same commit
//! when run again) or a defect (the code was wrong). The call is one file under
//! `calls/` in the history directory, written under a temporary name and
//! renamed like every other file there. A second call on the same test is a
//! second file: nothing is replaced, so the history of a judgement stays
//! readable. The call never changes a verdict.

use std::io;
use std::path::Path;

use crate::recording::{json_object, json_string, write_whole};

/// What `bar call` is run with, said when it is run wrong.
pub const CALL_USAGE: &str = "usage: bar call --test <name> --call flake|defect --by <handle> \
                              [--run <run id>] [--head <commit>] [--note <text>]";

/// **One judgement.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The test's full path, as the run printed it.
    pub test: String,
    /// `flake` or `defect`.
    pub call: String,
    /// Who made the call, as a handle.
    pub by: String,
    /// The run whose red this is about, when the caller names one.
    pub run_id: Option<String>,
    /// The commit the red was seen at, when the caller names one: a call about
    /// a red needs a run or a commit to be about.
    pub head: Option<String>,
    pub note: Option<String>,
}

impl Call {
    fn to_json(&self, at: &str) -> String {
        let optional = |value: &Option<String>| value.as_deref().map_or("null".into(), json_string);
        json_object(&[
            ("test", json_string(&self.test)),
            ("call", json_string(&self.call)),
            ("by", json_string(&self.by)),
            ("run_id", optional(&self.run_id)),
            ("head", optional(&self.head)),
            ("at", json_string(at)),
            ("note", optional(&self.note)),
        ])
    }
}

/// **Read the arguments after `bar call`.** `--test`, `--call` and `--by` are
/// required; `--call` is `flake` or `defect`; anything else is refused, naming
/// what was wrong.
pub fn parse_call(args: &[String]) -> Result<Call, String> {
    let (mut test, mut call, mut by, mut run_id, mut head, mut note) =
        (None, None, None, None, None, None);
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let slot = match flag.as_str() {
            "--test" => &mut test,
            "--call" => &mut call,
            "--by" => &mut by,
            "--run" => &mut run_id,
            "--head" => &mut head,
            "--note" => &mut note,
            other => return Err(format!("unrecognised argument '{other}'")),
        };
        match rest.next() {
            Some(value) if !value.trim().is_empty() => *slot = Some(value.clone()),
            _ => return Err(format!("{flag} needs a value")),
        }
    }
    let need = |value: Option<String>, flag: &str| value.ok_or(format!("{flag} is required"));
    let call = need(call, "--call")?;
    if call != "flake" && call != "defect" {
        return Err(format!("--call is '{call}': it must be flake or defect"));
    }
    Ok(Call {
        test: need(test, "--test")?,
        call,
        by: need(by, "--by")?,
        run_id,
        head,
        note,
    })
}

/// **Write the call as a file of its own** under `calls/` in the history
/// directory. The name is the moment and the process, so two calls made by two
/// processes never share one.
pub fn write_call(root: &Path, call: &Call, at: &str, pid: u32) -> io::Result<()> {
    let dir = root.join("calls");
    std::fs::create_dir_all(&dir)?;
    let moment: String = at.chars().filter(char::is_ascii_digit).collect();
    write_whole(&dir, &format!("{moment}-{pid}.json"), &call.to_json(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// **The arguments make a call, the optional ones may be left off, and a
    /// call that is neither word is refused by name.**
    #[test]
    fn the_arguments_make_a_call_and_a_wrong_one_is_refused_by_name() {
        let call = parse_call(&args(&[
            "--test",
            "a::b",
            "--call",
            "defect",
            "--by",
            "bot:gamma",
            "--note",
            "n",
        ]))
        .expect("a call");
        assert_eq!(
            call,
            Call {
                test: "a::b".into(),
                call: "defect".into(),
                by: "bot:gamma".into(),
                run_id: None,
                head: None,
                note: Some("n".into()),
            }
        );
        assert!(
            parse_call(&args(&["--test", "a::b", "--call", "maybe", "--by", "x"]))
                .expect_err("maybe")
                .contains("maybe")
        );
        assert!(
            parse_call(&args(&["--test", "a::b", "--call", "flake"]))
                .expect_err("no caller")
                .contains("--by")
        );
        assert!(
            parse_call(&args(&["--test"]))
                .expect_err("no value")
                .contains("--test")
        );
    }

    /// **The stored line, pinned.** The spelling of a stored key is declared
    /// nowhere else.
    #[test]
    fn the_stored_call_is_pinned() {
        let call = Call {
            test: "a::b".into(),
            call: "flake".into(),
            by: "bot:gamma".into(),
            run_id: Some("r1".into()),
            head: Some("abc123".into()),
            note: None,
        };
        assert_eq!(
            call.to_json("2026-10-08T23:41:09Z"),
            "{\"test\":\"a::b\",\"call\":\"flake\",\"by\":\"bot:gamma\",\"run_id\":\"r1\",\
             \"head\":\"abc123\",\"at\":\"2026-10-08T23:41:09Z\",\"note\":null}"
        );
    }
}
