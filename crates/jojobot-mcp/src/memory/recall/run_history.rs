//! **Cutting the history of a run under the answer ceiling.**

/// **Cut each run's history under the answer ceiling, newest kept, and name
/// what it left out** (rule 466). A part keeps the newest records it carries
/// that fit with the rest of its object, never cutting a record, and always
/// keeps one, so reading on always moves. A single record larger than the
/// ceiling still comes back whole.
///
/// `paged` is the `offset` the caller sent when the call named a run alone: the
/// block that names what was left out then says to repeat the call with the
/// next offset. In a call that selected several objects `offset` counts objects,
/// so the block names the call that reads the run alone.
pub(super) fn fit_run_histories(body: &mut serde_json::Value, traced: &str, paged: Option<usize>) {
    use jojobot_domain::text::ANSWER_CEILING;
    let Some(objects) = body.get_mut("objects").and_then(|o| o.as_array_mut()) else {
        return;
    };
    let mut objects = std::mem::take(objects);
    let rest = crate::answer::served_len(body)
        + crate::answer::STATUS_BAR_ROOM
        + crate::answer::widest_not_shown("objects");
    let skip = paged.unwrap_or(0);
    let what = if traced == jojobot_domain::session::history::JOURNAL {
        "wordings"
    } else {
        "changes"
    };
    for object in &mut objects {
        let Some(history) = object.get("history").cloned() else {
            continue;
        };
        let id = object["id"].as_str().unwrap_or_default().to_string();
        let count = history["count"].as_u64().unwrap_or(0) as usize;
        // Every record of the part, in the order it is served, with the entry it
        // belongs to for the journal.
        let mut records: Vec<(Option<usize>, serde_json::Value)> = Vec::new();
        let mut skeleton = history.clone();
        if let Some(entries) = skeleton.get_mut("entries").and_then(|e| e.as_array_mut()) {
            for (at, entry) in entries.iter_mut().enumerate() {
                let wordings = entry["wordings"].as_array_mut().map(std::mem::take);
                for wording in wordings.unwrap_or_default() {
                    records.push((Some(at), wording));
                }
            }
        } else if let Some(writes) = skeleton.get_mut("writes").and_then(|w| w.as_array_mut()) {
            for write in std::mem::take(writes) {
                records.push((None, write));
            }
        }
        let pointer = |left: usize, next: usize| match paged {
            Some(_) => crate::answer::not_shown(left, next, what),
            None => serde_json::json!({
                "count": left,
                "offset": next,
                "how_to_proceed": format!(
                    "these {what} are older than the ones above: recall subject: {id} with \
                     history: {traced} and offset: {next} reads them"
                ),
            }),
        };
        object["history"] = skeleton.clone();
        let reserve = crate::answer::under_key("not_shown", &pointer(usize::MAX, usize::MAX));
        let mut room =
            ANSWER_CEILING.saturating_sub(rest + crate::answer::served_len(object) + 1 + reserve);
        // Newest first: the newest write is the one written last, so fill in that
        // order and drop the oldest writes when the room runs out. The part is
        // served in the run's order, so the order of writing is read off each
        // record, with the run's order breaking a tie. **The moment is read as an
        // instant**: a moment prints its fraction only when it is not zero, so
        // as text `…:04Z` sorts after `…:04.5Z`.
        let mut newest_first: Vec<usize> = (0..records.len()).collect();
        newest_first.sort_by(|a, b| {
            let written = |at: &usize| {
                records[*at].1["written_at"]
                    .as_str()
                    .and_then(|moment| moment.parse::<jiff::Timestamp>().ok())
            };
            written(b).cmp(&written(a)).then(b.cmp(a))
        });
        let mut kept_at = std::collections::BTreeSet::new();
        for at in newest_first {
            let cost = crate::answer::served_len(&records[at].1) + 1;
            if !kept_at.is_empty() && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            kept_at.insert(at);
        }
        let kept = kept_at.len();
        let mut history = skeleton;
        for (at, (entry, record)) in records.into_iter().enumerate() {
            if !kept_at.contains(&at) {
                continue;
            }
            match entry {
                Some(at) => {
                    if let Some(wordings) = history["entries"][at]["wordings"].as_array_mut() {
                        wordings.push(record);
                    }
                }
                None => {
                    if let Some(writes) = history["writes"].as_array_mut() {
                        writes.push(record);
                    }
                }
            }
        }
        if let Some(entries) = history.get_mut("entries").and_then(|e| e.as_array_mut()) {
            entries.retain(|entry| entry["wordings"].as_array().is_some_and(|w| !w.is_empty()));
        }
        history["shown"] = kept.into();
        let left = count.saturating_sub(skip + kept);
        if left > 0 {
            history["not_shown"] = pointer(left, skip + kept);
        }
        object["history"] = history;
    }
    body["objects"] = objects.into();
}
