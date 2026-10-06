//! **Whether a model that was handed a fact a shipped type exists for wrote it
//! under that type's keys, read off the run's own call log.**
//!
//! The experiment this serves asks why a model writes its own key (`needed_by`)
//! over the one a shipped type names (`runs_out`). Four hypotheses say four
//! different things about what the model did BEFORE it wrote, so the reader
//! answers from the calls and never from the locks: a lock reports where the
//! store ended up, and two runs that ended in the same place can have got there
//! by opposite routes.
//!
//! **One row per run.** The reader decides nothing about whether a run was
//! right. It says what the calls were.

/// **One fact a room hands over, and the shipped type that exists for it.**
///
/// The type's name and the key it holds are both here because a model looks a
/// type up by either spelling, and the room's own lock asks about the key. The
/// day is how a write is told to belong to this fact: a call log carries no
/// subject that says which fact a record is about, and the day the entry gave
/// is the one thing both share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact {
    pub type_name: &'static str,
    pub key: &'static str,
    pub day: &'static str,
}

/// **Every key a shipped type holds a date under, other than a loop's.**
///
/// A loop's dates (`counts_from`, `last_check_in`) are a cadence and not a
/// deadline, so a model that used one for a deadline did not move toward a
/// shipped deadline type.
pub const SHIPPED_DATE_KEYS: [&str; 6] = [
    "runs_out",
    "decide_by",
    "valid_from",
    "valid_until",
    "leaves_on",
    "returns_on",
];

/// **The two facts the adoption rooms hand over**: a loan that has to end on a
/// day, and a decision that has to be made by a day.
pub const FACTS: [Fact; 2] = [
    Fact {
        type_name: "runs-out",
        key: "runs_out",
        day: "2026-11-03",
    },
    Fact {
        type_name: "decide-by",
        key: "decide_by",
        day: "2026-10-20",
    },
];

/// **What the model did about the type before its first write.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Looked {
    /// A call other than the boot named a shipped type or its key.
    Asked,
    /// The only sight of a type was the boot's own answer, which names every
    /// shipped type on every run.
    Door,
    /// Neither.
    No,
}

/// **How many of the room's facts were written under the keys asked about.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Used {
    Yes,
    Partly,
    No,
}

/// One run, as the table prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub condition: String,
    pub run: usize,
    pub looked: Looked,
    pub keys_written: Vec<String>,
    /// Each fact under the key ITS type holds.
    pub used: Used,
    /// Each fact under SOME shipped type's date key, which is the wider read:
    /// a model that moved a fact from an invented key onto another type's key
    /// still moved toward the shipped vocabulary.
    pub used_any: Used,
    pub corrected: bool,
}

/// The verb that opens a session. Its answer names every shipped type on every
/// boot, so it is the one call whose ANSWER is not evidence of a look.
const DOOR: &str = "start_here";

/// **One call the model made**, with the answer it got when there was one.
struct Made {
    verb: String,
    input: serde_json::Value,
    answer: String,
}

/// **Every call in the stream, in the order it was made.**
fn calls(stream: &str) -> Vec<Made> {
    let events: Vec<serde_json::Value> = stream
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let blocks = |event: &serde_json::Value| -> Vec<serde_json::Value> {
        event["message"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let mut answers = std::collections::HashMap::new();
    for block in events.iter().flat_map(blocks) {
        if block["type"] == "tool_result"
            && let Some(id) = block["tool_use_id"].as_str()
        {
            let text = match &block["content"] {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            answers.insert(id.to_string(), text);
        }
    }
    events
        .iter()
        .flat_map(blocks)
        .filter(|block| block["type"] == "tool_use")
        .map(|block| Made {
            verb: block["name"]
                .as_str()
                .unwrap_or_default()
                .rsplit("__")
                .next()
                .unwrap_or_default()
                .to_string(),
            input: block["input"].clone(),
            answer: answers
                .get(block["id"].as_str().unwrap_or_default())
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

/// **The key and value pairs a call writes**, when it writes any. A write
/// carries `fields` as a map; `declare_type` carries a list under the same
/// name and writes no record, so it is not one.
fn written(made: &Made) -> Vec<(String, String)> {
    match made.input["fields"].as_object() {
        Some(fields) => fields
            .iter()
            .map(|(key, held)| (key.clone(), held.as_str().unwrap_or_default().to_string()))
            .collect(),
        None => Vec::new(),
    }
}

/// **How many of `total` came out true, as the table says it.**
fn tally(held: usize, total: usize) -> Used {
    if held == 0 {
        Used::No
    } else if held == total {
        Used::Yes
    } else {
        Used::Partly
    }
}

/// Whether a call takes keys off a record.
fn clears(made: &Made) -> bool {
    made.input["clear_fields"]
        .as_array()
        .is_some_and(|keys| !keys.is_empty())
}

/// **Read one run's raw stream.** `facts` is what the room handed over.
pub fn read(condition: &str, run: usize, stream: &str, facts: &[Fact]) -> Row {
    let made = calls(stream);
    let names = |text: &str| {
        facts
            .iter()
            .any(|fact| text.contains(fact.type_name) || text.contains(fact.key))
    };

    // **Before the first write, or over the whole run when there is none.**
    // A look after the write is a check on it, and answers a different
    // question.
    let first_write = made.iter().position(|call| !written(call).is_empty());
    let before = &made[..first_write.unwrap_or(made.len())];
    let asked = before.iter().any(|call| names(&call.input.to_string()));
    let door = before
        .iter()
        .filter(|call| call.verb == DOOR)
        .any(|call| names(&call.answer));
    let looked = if asked {
        Looked::Asked
    } else if door {
        Looked::Door
    } else {
        Looked::No
    };

    let pairs: Vec<(String, String)> = made.iter().flat_map(written).collect();
    let mut keys_written: Vec<String> = Vec::new();
    for (key, _) in &pairs {
        if !keys_written.contains(key) {
            keys_written.push(key.clone());
        }
    }
    // **A write belongs to the fact whose day it carries.** The call log names
    // no subject, and a key alone cannot say which fact it was for.
    let held = |keys: &dyn Fn(&Fact) -> Vec<&'static str>| {
        facts
            .iter()
            .filter(|fact| {
                pairs
                    .iter()
                    .any(|(key, value)| value == fact.day && keys(fact).contains(&key.as_str()))
            })
            .count()
    };
    let used = tally(held(&|fact| vec![fact.key]), facts.len());
    let used_any = tally(held(&|_| SHIPPED_DATE_KEYS.to_vec()), facts.len());

    // **Corrected: it took keys off a record.** A model that revises by writing
    // a second record under the shipped key leaves both key sets in
    // `keys_written`, which is where that reads.
    Row {
        condition: condition.to_string(),
        run,
        looked,
        keys_written,
        used,
        used_any,
        corrected: made.iter().any(clears),
    }
}

fn used_word(used: Used) -> &'static str {
    match used {
        Used::Yes => "yes",
        Used::Partly => "partly",
        Used::No => "no",
    }
}

/// **The table: a header and one row per run.**
pub fn table(rows: &[Row]) -> String {
    let mut out = String::from(
        "condition | run | looked? | keys written | used the fact's own key? | used any shipped date key? | corrected?\n",
    );
    for row in rows {
        out.push_str(&format!(
            "{} | {} | {} | {} | {} | {} | {}\n",
            row.condition,
            row.run,
            match row.looked {
                Looked::Asked => "asked",
                Looked::Door => "door",
                Looked::No => "no",
            },
            row.keys_written.join(","),
            used_word(row.used),
            used_word(row.used_any),
            if row.corrected { "yes" } else { "no" },
        ));
    }
    out
}

/// **Every run kept in a directory, as rows.** A run's raw stream is named
/// `<condition>-<run>.jsonl`, which is what the experiment's runner writes.
/// A file that is not named that way is not a run and is refused by name,
/// because a skipped file would read as a run that never happened.
pub fn rows_in(dir: &std::path::Path, facts: &[Fact]) -> anyhow::Result<Vec<Row>> {
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let (condition, run) = stem
            .rsplit_once('-')
            .and_then(|(condition, run)| Some((condition, run.parse::<usize>().ok()?)))
            .ok_or_else(|| {
                anyhow::anyhow!("{} is not named <condition>-<run>.jsonl", path.display())
            })?;
        rows.push(read(
            condition,
            run,
            &std::fs::read_to_string(&path)?,
            facts,
        ));
    }
    rows.sort_by(|a, b| (&a.condition, a.run).cmp(&(&b.condition, b.run)));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Recorded from real runs, cut down** — see `calls.rs`'s `fixture` for
    /// why none is regenerated by pasting a fresh capture in.
    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the fixture is test material: {}: {e}", path.display()))
    }

    /// One fact, on the day the recorded run wrote. The recorded runs come
    /// from the vault room, so their days are the vault's own and not the
    /// adoption rooms'.
    fn only(key: &'static str, type_name: &'static str, day: &'static str) -> [Fact; 1] {
        [Fact {
            type_name,
            key,
            day,
        }]
    }

    /// **The yes: a model that asked about the type, then wrote under its key.**
    #[test]
    fn a_run_that_asked_and_wrote_the_shipped_key_reads_as_yes() {
        let row = read(
            "E0",
            1,
            &fixture("adoption-adopted.jsonl"),
            &only("decide_by", "decide-by", "2026-10-10"),
        );
        assert_eq!(row.looked, Looked::Asked, "{row:?}");
        assert_eq!(row.keys_written, vec!["decide_by".to_string()], "{row:?}");
        assert_eq!(row.used, Used::Yes, "{row:?}");
        assert_eq!(row.used_any, Used::Yes, "{row:?}");
        assert!(!row.corrected, "{row:?}");
    }

    /// **The no: a model that never asked and wrote its own key.**
    ///
    /// The same run is a yes for one fact and a no for the other, which is what
    /// makes `Partly` a third value rather than a rounding.
    #[test]
    fn a_run_that_never_asked_and_wrote_its_own_key_reads_as_no() {
        let row = read(
            "E0",
            2,
            &fixture("adoption-invented.jsonl"),
            &only("runs_out", "runs-out", "2027-03-08"),
        );
        assert_eq!(
            row.looked,
            Looked::Door,
            "the boot listed the types and nothing else did: {row:?}",
        );
        assert_eq!(
            row.keys_written,
            vec!["guarantee_expires".to_string()],
            "a refused write and its retry name one key once: {row:?}",
        );
        assert_eq!(row.used, Used::No, "{row:?}");
        assert_eq!(row.used_any, Used::No, "{row:?}");
    }

    /// **Which keys count depends on the facts the room handed over.**
    ///
    /// The adopted run wrote `decide_by` against a day. For a room whose fact
    /// on that day is a loan, that is another type's key on the fact: not the
    /// intended key, and still a shipped one.
    #[test]
    fn a_shipped_key_on_the_wrong_fact_counts_only_as_some_shipped_key() {
        let row = read(
            "E0",
            1,
            &fixture("adoption-adopted.jsonl"),
            &only("runs_out", "runs-out", "2026-10-10"),
        );
        assert_eq!(row.used, Used::No, "{row:?}");
        assert_eq!(row.used_any, Used::Yes, "{row:?}");
        assert_eq!(row.keys_written, vec!["decide_by".to_string()], "{row:?}");
    }

    /// **One of two facts under its shipped key is partly.**
    #[test]
    fn one_fact_of_two_under_its_shipped_key_reads_as_partly() {
        let facts = [
            only("decide_by", "decide-by", "2026-10-10")[0],
            only("runs_out", "runs-out", "2027-03-08")[0],
        ];
        let row = read("E0", 1, &fixture("adoption-adopted.jsonl"), &facts);
        assert_eq!(row.used, Used::Partly, "{row:?}");
        assert_eq!(row.used_any, Used::Partly, "{row:?}");
    }

    /// **A write that takes keys off is a correction.**
    #[test]
    fn a_write_that_clears_keys_reads_as_corrected() {
        let row = read(
            "E1",
            3,
            &fixture("adoption-revised.jsonl"),
            &only("decide_by", "decide-by", "2026-10-10"),
        );
        assert!(row.corrected, "{row:?}");
        assert_eq!(
            row.keys_written,
            vec!["tickets_needed_by".to_string()],
            "{row:?}",
        );
        assert_eq!(row.looked, Looked::No, "no boot, no look: {row:?}");
    }

    /// **A look after the write is not a look before it.**
    ///
    /// The adopted run's own events with the write moved ahead of the two
    /// searches: the same calls, the other order.
    #[test]
    fn a_look_after_the_first_write_does_not_count_as_a_look_before_it() {
        let recorded = fixture("adoption-adopted.jsonl");
        let lines: Vec<&str> = recorded.lines().collect();
        // Door pair, search pair, search pair, write pair.
        assert_eq!(lines.len(), 8, "the recorded run changed shape: {recorded}");
        let reordered = [&lines[0..2], &lines[6..8], &lines[2..6]]
            .concat()
            .join("\n");
        let row = read(
            "E0",
            1,
            &reordered,
            &only("decide_by", "decide-by", "2026-10-10"),
        );
        assert_eq!(row.looked, Looked::Door, "{row:?}");
        assert_eq!(row.used, Used::Yes, "the write is still read: {row:?}");
    }

    /// **`declare_type` carries a list under `fields`, and it writes no record.**
    ///
    /// The call is the shape a real run made (a type declared under the
    /// shipped type's own name); it writes nothing, so it is no write and
    /// names no key.
    #[test]
    fn declaring_a_type_writes_no_key() {
        let declared = r#"{"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "id-1", "name": "mcp__jojobot-room__declare_type", "input": {"fields": [{"key": "runs_out"}]}}]}}"#;
        let row = read(
            "E0",
            1,
            declared,
            &only("runs_out", "runs-out", "2027-03-08"),
        );
        assert_eq!(row.keys_written, Vec::<String>::new(), "{row:?}");
        assert_eq!(row.used, Used::No, "{row:?}");
    }

    /// **Only the boot's own answer is the door.**
    ///
    /// The invented run with its boot call renamed to another verb: the answer
    /// still names the types, and no longer comes from the call that always
    /// does.
    #[test]
    fn a_type_named_in_another_verbs_answer_is_not_the_door() {
        let renamed = fixture("adoption-invented.jsonl").replace("start_here", "search");
        let row = read(
            "E0",
            2,
            &renamed,
            &only("runs_out", "runs-out", "2027-03-08"),
        );
        assert_eq!(row.looked, Looked::No, "{row:?}");
    }

    /// **The table says one thing per run, in order.**
    ///
    /// Read through the directory the runner writes, so the file naming is
    /// exercised as well as the printing: two conditions, written out of order.
    #[test]
    fn a_directory_of_runs_prints_one_row_per_run_in_order() {
        let dir = std::env::temp_dir().join(format!("jojobot-adoption-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        std::fs::write(dir.join("E1-2.jsonl"), fixture("adoption-invented.jsonl")).unwrap();
        std::fs::write(dir.join("E0-10.jsonl"), fixture("adoption-adopted.jsonl")).unwrap();
        std::fs::write(dir.join("E0-2.jsonl"), fixture("adoption-revised.jsonl")).unwrap();
        // The readable transcript sits beside each raw stream and is not a run.
        std::fs::write(dir.join("E0-2.md"), "the readable transcript").unwrap();

        let rows = rows_in(&dir, &FACTS).expect("the directory reads");
        let printed = table(&rows);
        let _ = std::fs::remove_dir_all(&dir);

        let ids: Vec<(String, usize)> = rows.iter().map(|r| (r.condition.clone(), r.run)).collect();
        assert_eq!(
            ids,
            vec![
                ("E0".to_string(), 2),
                ("E0".to_string(), 10),
                ("E1".to_string(), 2),
            ],
            "runs sort by condition and then by number, not by name: {printed}",
        );
        assert_eq!(
            printed.lines().count(),
            4,
            "a header and three rows: {printed}"
        );
        assert!(
            printed.contains("E0 | 10 | asked | decide_by | no | no | no"),
            "the adopted run's row: {printed}",
        );
        assert!(
            printed.contains("E1 | 2 | door | guarantee_expires | no | no | no"),
            "the invented run's row: {printed}",
        );
    }

    /// **A jsonl that is not named as a run is refused, never skipped.**
    #[test]
    fn a_file_that_is_not_named_as_a_run_is_refused() {
        let dir = std::env::temp_dir().join(format!("jojobot-adoption-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        std::fs::write(dir.join("notes.jsonl"), "{}").unwrap();
        let refused = rows_in(&dir, &FACTS);
        let _ = std::fs::remove_dir_all(&dir);
        let why = refused
            .expect_err("a file nobody can place was read as a run")
            .to_string();
        assert!(
            why.contains("notes.jsonl"),
            "the refusal names the file: {why}"
        );
    }

    /// **A write belongs to the fact whose day it carries.**
    ///
    /// The adopted run wrote its shipped key against one day. For a room whose
    /// fact is on another day, that write is about something else and scores
    /// nothing.
    #[test]
    fn a_write_carrying_another_facts_day_scores_nothing() {
        let row = read(
            "E0",
            1,
            &fixture("adoption-adopted.jsonl"),
            &only("decide_by", "decide-by", "2026-12-25"),
        );
        assert_eq!(row.used, Used::No, "{row:?}");
        assert_eq!(row.used_any, Used::No, "{row:?}");
        assert_eq!(row.keys_written, vec!["decide_by".to_string()], "{row:?}");
    }
}
