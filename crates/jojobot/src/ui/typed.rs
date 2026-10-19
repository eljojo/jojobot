//! **Built-in kinds look like what they are**: a project as a board, in the
//! status columns it lists; a piece of work as a task; a rhythm as a loop; a
//! thread with its standing decisions first.
//!
//! Each reads the answer of a view the build ships, run where a view's page runs
//! it ([`pages::answer_named_view_with`]), so the board shows the work the
//! board view shows. The page around a rendering is the page of any thing.

use std::collections::{BTreeMap, HashMap};

use jojobot_domain::attention;
use jojobot_domain::memory::{Entity, EntityId, EntityKind, Fact, FactStatus, kinds};

use crate::AppState;
use crate::ui::{pages, shell, tree};

/// The view a project's board is read from.
const BOARD_VIEW: &str = "view:board";

/// A handle as a link to its page, or as text when nothing by that handle stands.
fn linked(handle: &str, by_id: &HashMap<&EntityId, &Entity>) -> String {
    match by_id.get(&EntityId(handle.to_string())) {
        Some(entity) => format!(
            "<a href=\"{}\">{}</a>",
            pages::escape(&tree::canonical_path(entity, by_id)),
            pages::escape(&entity.name)
        ),
        None => pages::escape(handle),
    }
}

/// **The board of a project**: a column for each status word it accepts (the five
/// the build ships, then the words the instance and the project list), and after
/// them a column for each word its work holds that nobody listed, so no piece of
/// work is dropped. A card names the work and says what it needs and who owns it.
pub async fn board(
    state: &AppState,
    project: &Entity,
    held: &BTreeMap<String, String>,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let inputs = BTreeMap::from([("project".to_string(), project.id.to_string())]);
    let answer = match pages::answer_named_view_with(state, BOARD_VIEW, &inputs).await {
        Ok(answer) => answer,
        Err(page) => return page,
    };
    let instance =
        jojobot_mcp::orientation::instance_zone::instance_fields_of(&*state.memory).await;
    let mut words = kinds::accepted_words(&instance, Some(held));
    for object in &answer.found {
        let word = object
            .fields
            .get(kinds::STATUS)
            .cloned()
            .unwrap_or_default();
        if !words.contains(&word) {
            words.push(word);
        }
    }
    let mut done = 0;
    let columns: String = words
        .iter()
        .map(|word| {
            let cards: Vec<String> = answer
                .found
                .iter()
                .filter(|object| {
                    object.fields.get(kinds::STATUS).map_or("", String::as_str) == word
                })
                .map(|object| {
                    let finished = word == kinds::FINISHED;
                    if finished {
                        done += 1;
                    }
                    let mut card = format!(
                        "<div class=\"card{}\">{}",
                        if finished { " done" } else { "" },
                        linked(object.entity.id.as_str(), by_id)
                    );
                    if let Some(needs) = object
                        .fields
                        .get("depends_on")
                        .filter(|n| !n.trim().is_empty())
                    {
                        let links: Vec<String> = needs
                            .split(',')
                            .map(str::trim)
                            .filter(|handle| !handle.is_empty())
                            .map(|handle| linked(handle, by_id))
                            .collect();
                        card.push_str(&format!(
                            "<span class=\"m\">needs: {}</span>",
                            links.join(", ")
                        ));
                    }
                    if let Some(owner) = object.fields.get("owner").filter(|o| !o.trim().is_empty())
                    {
                        card.push_str(&format!(
                            "<span class=\"m\">{}</span>",
                            pages::escape(owner)
                        ));
                    }
                    card.push_str("</div>");
                    card
                })
                .collect();
            let label = if word.is_empty() { "no status" } else { word };
            format!(
                "<div class=\"col\" data-status=\"{}\"><h3>{} <span>{}</span></h3>{}</div>",
                pages::escape(word),
                pages::escape(label),
                cards.len(),
                cards.concat()
            )
        })
        .collect();
    shell::boxed_as(
        "board",
        false,
        &pages::escape(&project.name),
        &format!("{} work · {done} done", answer.found.len()),
        &format!(
            "<div class=\"board\" style=\"grid-template-columns: repeat({}, 1fr)\">{columns}</div>",
            words.len()
        ),
    )
}

/// The most writes of a key a task's history shows.
const HISTORY_SHOWN: usize = 5;

/// A value the thing holds, when it holds one that says something.
fn held_value<'a>(held: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    held.get(key)
        .map(String::as_str)
        .filter(|v| !v.trim().is_empty())
}

/// **A comma list of handles as links**, or a dash when there is none.
fn handles(value: Option<&str>, by_id: &HashMap<&EntityId, &Entity>) -> String {
    value.map_or("—".to_string(), |all| {
        all.split(',')
            .map(str::trim)
            .filter(|handle| !handle.is_empty())
            .map(|handle| linked(handle, by_id))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// **A piece of work as a task**: its status, what it needs, who owns it, and the
/// last writes of its status, newest first.
pub async fn work(
    state: &AppState,
    work: &Entity,
    held: &BTreeMap<String, String>,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let history = match state.memory.history(&work.id, kinds::STATUS).await {
        Ok(writes) => writes,
        Err(err) => return pages::blind("Status", "its history", &err.to_string()),
    };
    let writes: String = history
        .iter()
        .rev()
        .take(HISTORY_SHOWN)
        .map(|write| {
            format!(
                "<li><span class=\"t\">{}</span><span class=\"m\">{}</span></li>",
                write
                    .value
                    .as_deref()
                    .map_or("(taken off)".to_string(), pages::escape),
                write.recorded_at
            )
        })
        .collect();
    shell::boxed_as(
        "work",
        false,
        &pages::escape(&work.name),
        "",
        &format!(
            "<dl class=\"kv\"><dt>status</dt><dd>{}</dd><dt>needs</dt><dd>{}</dd>\
             <dt>owner</dt><dd>{}</dd></dl><ul id=\"status-history\">{writes}</ul>",
            held_value(held, kinds::STATUS).map_or("—".to_string(), pages::escape),
            handles(held_value(held, "depends_on"), by_id),
            handles(held_value(held, "owner"), by_id),
        ),
    )
}

/// **A rhythm as a loop**: how often it comes round, when it was last checked in,
/// when it falls due next, and how late it is. The due day is the one the domain
/// computes from the loop's keys ([`attention::owed`]), and today is the day in
/// the instance's zone.
pub async fn rhythm(state: &AppState, rhythm: &Entity, held: &BTreeMap<String, String>) -> String {
    let frame = jojobot_mcp::orientation::instance_zone::unzoned_frame(&*state.memory).await;
    let today = state.clock.today_in(&frame);
    let carriers = attention::shipped();
    let carriers: Vec<&dyn attention::Carrier> =
        carriers.iter().map(std::convert::AsRef::as_ref).collect();
    let (next, late) = match attention::owed(&carriers, held) {
        attention::Due::On(day) => (
            format!("next due {day}"),
            attention::Due::On(day)
                .days_overdue(today)
                .filter(|days| *days > 0)
                .map(|days| format!("{days} days overdue (fell due {day})")),
        ),
        attention::Due::NotYetOpened => ("not checked in yet".to_string(), None),
        attention::Due::Unreadable => (
            "next due cannot be worked out from its keys".to_string(),
            None,
        ),
        attention::Due::Never => ("nothing falls due".to_string(), None),
    };
    let mut rows = format!(
        "<dt>cadence</dt><dd>{}</dd><dt>last check-in</dt><dd>{}</dd><dd>{}</dd>",
        held_value(held, "cadence_days").map_or("—".to_string(), |days| format!(
            "every {} days",
            pages::escape(days)
        )),
        held_value(held, "last_check_in").map_or("—".to_string(), pages::escape),
        pages::escape(&next),
    );
    if let Some(late) = late {
        rows.push_str(&format!("<dd class=\"late\">{}</dd>", pages::escape(&late)));
    }
    shell::boxed_as(
        "rhythm",
        late_is_live(&rows),
        &pages::escape(&rhythm.name),
        "",
        &format!("<dl class=\"kv\">{rows}</dl>"),
    )
}

/// A loop that is late is the page's live box (vermilion).
fn late_is_live(rows: &str) -> bool {
    rows.contains("class=\"late\"")
}

/// **A thread's standing decisions**, each a link to the decision with the latest
/// thing it says: the children of the thread that are decisions and are not
/// archived. A decision that was replaced is archived with its reason, so it is
/// not among them.
pub async fn decisions(
    state: &AppState,
    thread: &Entity,
    entities: &[Entity],
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let mut standing: Vec<&Entity> = entities
        .iter()
        .filter(|e| {
            e.browsable() && e.kind == EntityKind::DECISION && e.parent.as_ref() == Some(&thread.id)
        })
        .collect();
    if standing.is_empty() {
        return String::new();
    }
    standing.sort_by(|a, b| a.id.cmp(&b.id));
    let mut list = String::from("<ul>");
    for decision in &standing {
        let said = match state.memory.recall(&decision.id).await {
            Ok(facts) => latest_claim(&facts).unwrap_or_default(),
            Err(err) => return pages::blind("Decisions", "a decision", &err.to_string()),
        };
        list.push_str(&format!(
            "<li><span class=\"t\"><a href=\"{}\">{}</a> {}</span></li>",
            pages::escape(&tree::canonical_path(decision, by_id)),
            pages::escape(&decision.name),
            pages::escape(&said)
        ));
    }
    list.push_str("</ul>");
    shell::boxed_as(
        "decisions",
        false,
        "decisions",
        &standing.len().to_string(),
        &list,
    )
}

/// What the newest standing claim of a thing says.
fn latest_claim(facts: &[Fact]) -> Option<String> {
    facts
        .iter()
        .filter(|fact| fact.status == jojobot_domain::memory::FactStatus::Active)
        .max_by_key(|fact| fact.inserted_at)
        .map(|fact| fact.content.clone())
}

/// How many claims a recent list shows before it counts the rest.
const RECENT_SHOWN: usize = 10;

/// **A list of the claims that stand, newest first**: the newest `RECENT_SHOWN`
/// of them with the day beside each, and a line counting the older ones.
fn claim_list(
    id: &str,
    heading: &str,
    facts: &[Fact],
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let mut live: Vec<&Fact> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .collect();
    live.sort_by_key(|fact| std::cmp::Reverse(fact.recorded_at));
    if live.is_empty() {
        return String::new();
    }
    let lines: String = live
        .iter()
        .take(RECENT_SHOWN)
        .map(|fact| {
            format!(
                "<li><span class=\"t\">{}</span><span class=\"m\">{}</span></li>",
                pages::linkify(&fact.content, by_id),
                fact.recorded_at
            )
        })
        .collect();
    let left = live.len().saturating_sub(RECENT_SHOWN);
    let more = if left > 0 {
        format!("<p class=\"m\">{left} older, in the fold below.</p>")
    } else {
        String::new()
    };
    shell::boxed_as(id, false, heading, "", &format!("<ul>{lines}</ul>{more}"))
}

/// **What a project is**: what it holds, one line to a key, and behind a ▸ who
/// wrote each value and how they know — the provenance of the newest claim that
/// carries the key. The board's own `columns` key is not a value of the project.
fn about_project(
    held: &BTreeMap<String, String>,
    facts: &[Fact],
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let keys: Vec<(&String, &String)> = held
        .iter()
        .filter(|(key, value)| key.as_str() != kinds::COLUMNS && !value.trim().is_empty())
        .collect();
    if keys.is_empty() {
        return String::new();
    }
    let mut values = String::new();
    let mut writers = String::new();
    for (key, value) in keys {
        values.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd>",
            pages::escape(key),
            handles(Some(value), by_id)
        ));
        let writer = facts
            .iter()
            .filter(|fact| fact.status == FactStatus::Active && fact.fields.contains_key(key))
            .max_by_key(|fact| fact.inserted_at);
        if let Some(fact) = writer {
            writers.push_str(&format!(
                "<dt>{}</dt><dd>{} · {}</dd>",
                pages::escape(key),
                pages::escape(fact.provenance.as_token()),
                fact.recorded_at
            ));
        }
    }
    shell::boxed_as(
        "about",
        false,
        "about",
        "",
        &format!(
            "<dl class=\"kv\">{values}</dl>{}",
            shell::fold(
                "who wrote each value",
                &format!("<dl class=\"kv\">{writers}</dl>")
            )
        ),
    )
}

/// **Everything under the project**, work included, a row each with its kind.
fn contents(children: &[&Entity], by_id: &HashMap<&EntityId, &Entity>) -> String {
    if children.is_empty() {
        return String::new();
    }
    let rows: Vec<Vec<(&str, String)>> = children
        .iter()
        .map(|child| {
            vec![
                (
                    "p",
                    format!(
                        "<a href=\"{}\">{}</a>",
                        pages::escape(&tree::canonical_path(child, by_id)),
                        pages::escape(child.id.as_str())
                    ),
                ),
                ("k", pages::escape(child.kind.as_token())),
            ]
        })
        .collect();
    shell::boxed_as(
        "contents",
        false,
        "contents",
        &children.len().to_string(),
        &shell::row_table("contents-list", "fs", &["path", "kind"], &rows),
    )
}

/// **The rest of a project's page** after its board: what it is, what happened
/// lately, and what sits in it.
pub fn project_rest(
    held: &BTreeMap<String, String>,
    facts: &[Fact],
    children: &[&Entity],
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    format!(
        "{}{}{}",
        about_project(held, facts, by_id),
        claim_list("recent", "recent", facts, by_id),
        contents(children, by_id)
    )
}

/// **A thread's own claims** after its decisions, newest first.
pub fn claims(facts: &[Fact], by_id: &HashMap<&EntityId, &Entity>) -> String {
    claim_list("claims", "claims", facts, by_id)
}
