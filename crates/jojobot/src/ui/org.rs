//! **A bot's place in the org chart**, read from `view:colleagues`.
//!
//! The view says whom each bot reports to and nothing inward, so the chain up
//! and the reports down are folded here from its answer. The view is run where a
//! view's page runs it ([`pages::answer_view`]), so the chart means what the
//! view means and a bot is shown where `recall` of the view would put it.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::{extract::State, http::StatusCode, response::Response};
use jojobot_domain::memory::{Entity, EntityId};

use crate::AppState;
use crate::ui::{pages, shell, tree};

/// The address of the org chart. It is no entity's path, so the gate knows it
/// by name ([`crate::ui::require_browser`]).
pub const PATH: &str = "/ui/org/";

/// The view the chart is read from.
const VIEW: &str = "view:colleagues";

/// One bot the view lists, and the bot it says it reports to.
pub struct Colleague {
    entity: Entity,
    reports_to: Option<EntityId>,
}

/// **Every bot the colleagues view lists**, or the page text saying why it could
/// not be run.
async fn colleagues(state: &AppState) -> Result<Vec<Colleague>, String> {
    let view = EntityId(VIEW.to_string());
    let held =
        state.memory.fields(&view).await.map_err(|err| {
            pages::blind("The org chart", "the colleagues view", &err.to_string())
        })?;
    let facts =
        state.memory.recall(&view).await.map_err(|err| {
            pages::blind("The org chart", "the colleagues view", &err.to_string())
        })?;
    let answered = pages::answer_view(state, &view, &held, &facts, &BTreeMap::new()).await?;
    Ok(answered
        .found
        .into_iter()
        .map(|object| Colleague {
            reports_to: object
                .fields
                .get("reports_to")
                .filter(|boss| !boss.trim().is_empty())
                .map(|boss| EntityId(boss.trim().to_string())),
            entity: object.entity,
        })
        .collect())
}

/// **The chain a bot reports through, the head first and the bot last.** A line
/// that loops back on itself stops where it would repeat.
fn chain<'a>(of: &EntityId, all: &'a [Colleague]) -> Vec<&'a Colleague> {
    let by_id: HashMap<&EntityId, &Colleague> = all.iter().map(|c| (&c.entity.id, c)).collect();
    let mut up = Vec::new();
    let mut seen = HashSet::new();
    let mut at = by_id.get(of).copied();
    while let Some(colleague) = at {
        if !seen.insert(&colleague.entity.id) {
            break;
        }
        up.push(colleague);
        at = colleague
            .reports_to
            .as_ref()
            .and_then(|boss| by_id.get(boss).copied());
    }
    up.reverse();
    up
}

/// The bots that report to this one, in handle order.
fn reports<'a>(of: &EntityId, all: &'a [Colleague]) -> Vec<&'a Colleague> {
    let mut under: Vec<&Colleague> = all
        .iter()
        .filter(|colleague| colleague.reports_to.as_ref() == Some(of))
        .collect();
    under.sort_by(|a, b| a.entity.id.cmp(&b.entity.id));
    under
}

/// **Every bot with its depth, each under the bot it reports to.** A root is a
/// bot that reports to nobody the view lists. A bot whose line loops, so that no
/// root leads to it, is placed as a root rather than left off the chart.
fn forest(all: &[Colleague]) -> Vec<(usize, &Colleague)> {
    let listed: HashSet<&EntityId> = all.iter().map(|c| &c.entity.id).collect();
    let mut roots: Vec<&Colleague> = all
        .iter()
        .filter(|c| {
            c.reports_to
                .as_ref()
                .is_none_or(|boss| !listed.contains(boss))
        })
        .collect();
    roots.sort_by(|a, b| a.entity.id.cmp(&b.entity.id));
    let mut placed = HashSet::new();
    let mut out = Vec::new();
    fn walk<'a>(
        at: &'a Colleague,
        depth: usize,
        all: &'a [Colleague],
        placed: &mut HashSet<&'a EntityId>,
        out: &mut Vec<(usize, &'a Colleague)>,
    ) {
        if !placed.insert(&at.entity.id) {
            return;
        }
        out.push((depth, at));
        for under in reports(&at.entity.id, all) {
            walk(under, depth + 1, all, placed, out);
        }
    }
    for root in roots {
        walk(root, 0, all, &mut placed, &mut out);
    }
    let mut looped: Vec<&Colleague> = all
        .iter()
        .filter(|c| !placed.contains(&c.entity.id))
        .collect();
    looped.sort_by(|a, b| a.entity.id.cmp(&b.entity.id));
    for colleague in looped {
        walk(colleague, 0, all, &mut placed, &mut out);
    }
    out
}

/// A bot as a link to its page, or as its handle when the page's path is not
/// known.
fn link(colleague: &Colleague, by_id: &HashMap<&EntityId, &Entity>) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        pages::escape(&tree::canonical_path(&colleague.entity, by_id)),
        pages::escape(colleague.entity.id.as_str())
    )
}

/// **The org chart section of a bot's page**: the chain it reports through, a
/// link to the whole chart, and the bots that report to it. A bot that reports
/// to nobody is a chain of one, which is a root and not an error.
pub async fn bot_section(
    state: &AppState,
    bot: &EntityId,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let all = match colleagues(state).await {
        Ok(all) => all,
        Err(page) => return page,
    };
    let up = chain(bot, &all);
    let steps: Vec<String> = up
        .iter()
        .map(|colleague| {
            if &colleague.entity.id == bot {
                pages::escape(bot.as_str())
            } else {
                link(colleague, by_id)
            }
        })
        .collect();
    let mut out = format!(
        "<h2>Reports to</h2>\n<p id=\"chain\">{}</p>\n<p><a href=\"{PATH}\">Org chart</a></p>\n",
        if steps.is_empty() {
            pages::escape(bot.as_str())
        } else {
            steps.join(" › ")
        }
    );
    let under = reports(bot, &all);
    if under.is_empty() {
        out.push_str("<p id=\"reports\">Nobody reports to this bot.</p>\n");
    } else {
        out.push_str("<ul id=\"reports\">\n");
        for colleague in under {
            out.push_str(&format!("<li>{}</li>\n", link(colleague, by_id)));
        }
        out.push_str("</ul>\n");
    }
    out
}

/// `/ui/org/` — every bot, each under the bot it reports to.
pub async fn chart(State(state): State<AppState>) -> Response {
    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => {
            tracing::warn!(error = %err, "the org chart could not read the entities");
            return crate::ui::refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read its own store, so this page would be empty rather than accurate.",
            );
        }
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();
    let content = match colleagues(&state).await {
        Err(page) => page,
        Ok(all) => {
            let mut lines = String::from("<ul class=\"org\" style=\"list-style:none\">\n");
            for (depth, colleague) in forest(&all) {
                lines.push_str(&format!(
                    "<li data-depth=\"{depth}\" style=\"margin-left:{}em\">{} <span class=\"m\">{}</span></li>\n",
                    depth * 2,
                    link(colleague, &by_id),
                    pages::escape(&colleague.entity.name)
                ));
            }
            lines.push_str("</ul>\n");
            shell::grid(&[shell::boxed(
                "Org chart",
                &format!("{} bots", all.len()),
                &lines,
            )])
        }
    };
    pages::html(&shell::page("Org chart", "", &content))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(handle: &str, boss: Option<&str>) -> Colleague {
        Colleague {
            entity: serde_json::from_value(serde_json::json!({
                "id": handle, "kind": "bot", "name": handle, "source": "the test", "boot": "on-demand",
            }))
            .expect("an entity"),
            reports_to: boss.map(|boss| EntityId(boss.to_string())),
        }
    }

    #[test]
    fn a_chain_runs_from_the_head_down_and_stops_where_a_line_loops() {
        let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
        let all = [
            bot("bot:rho", None),
            bot("bot:psi", Some("bot:rho")),
            bot("bot:upsilon", Some("bot:psi")),
        ];
        let handles = |chain: Vec<&Colleague>| -> Vec<String> {
            chain.iter().map(|c| c.entity.id.to_string()).collect()
        };
        assert_eq!(
            handles(chain(&EntityId("bot:upsilon".into()), &all)),
            ["bot:rho", "bot:psi", "bot:upsilon"]
        );
        let loop_ = [
            bot("bot:beta", Some("bot:epsilon")),
            bot("bot:epsilon", Some("bot:beta")),
        ];
        assert_eq!(
            handles(chain(&EntityId("bot:beta".into()), &loop_)).len(),
            2
        );
        // A loop has no root, and still every bot is on the chart.
        assert_eq!(forest(&loop_).len(), 2);
    }
}
