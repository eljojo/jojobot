//! **Built-in kinds look like what they are**: a project as a board, in the
//! status columns it lists.
//!
//! Each reads the answer of a view the build ships, run where a view's page runs
//! it ([`pages::answer_named_view_with`]), so the board shows the work the
//! board view shows. The page around a rendering is the page of any thing.

use std::collections::{BTreeMap, HashMap};

use jojobot_domain::memory::{Entity, EntityId, kinds};

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
