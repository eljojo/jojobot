//! **The search page: the box in every page's bar asks the search the bots use.**
//!
//! The words become a [`SearchQuery`] with every default the verb has, and no
//! asker, so the operator is shown everything. It goes to the same port the
//! `search` verb asks. Looking writes nothing and takes no delivery: mail is not
//! searched here, because reading a message is not looking at it.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    response::Response,
};
use jojobot_domain::memory::search::{Hit, SearchQuery};
use jojobot_domain::memory::{Entity, EntityId};

use crate::AppState;
use crate::ui::{pages, shell, tree};

/// Where the box sends its words.
pub const PATH: &str = "/ui/search";

/// The one field the box sends.
#[derive(Debug, serde::Deserialize)]
pub struct Asked {
    #[serde(default)]
    q: String,
}

/// A hit's claim, cut for a listing line.
const CLAIM_CHARS: usize = 160;

pub async fn results(State(state): State<AppState>, Query(asked): Query<Asked>) -> Response {
    let words = asked.q.trim();
    let mut body = format!("<h1>Search</h1>\n{}", shell::search_form(words));
    if words.is_empty() {
        return pages::html(&shell::page("Search", "", &body));
    }
    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => return pages::unreadable(err),
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();
    let hits = match state.search.search(&SearchQuery::text(words)).await {
        Ok(hits) => hits,
        Err(err) => return pages::unreadable(err),
    };
    let mut rows = String::new();
    let mut not_listed = 0usize;
    for hit in &hits {
        match hit {
            Hit::Entity { entity, .. } => rows.push_str(&row(
                &link_to(&entity.id, &by_id),
                &pages::escape(&entity.name),
                entity.id.as_str(),
            )),
            Hit::Fact { fact, subject, .. } => {
                let claim: String = fact.content.chars().take(CLAIM_CHARS).collect();
                rows.push_str(&row(
                    &link_to(&subject.id, &by_id),
                    &pages::escape(&claim),
                    &fact.address().to_string(),
                ));
            }
            _ => not_listed += 1,
        }
    }
    if rows.is_empty() {
        body.push_str("<p id=\"nothing-found\">Nothing found.</p>\n");
    } else {
        body.push_str(&format!(
            "<table id=\"found\">\n<tr><th>Where</th><th>What</th><th>Address</th></tr>\n{rows}</table>\n"
        ));
    }
    if not_listed > 0 {
        body.push_str(&format!(
            "<p id=\"not-listed\">{not_listed} more found in prose or mail, which this page does not list.</p>\n"
        ));
    }
    pages::html(&shell::page("Search", "", &body))
}

fn row(link: &str, what: &str, address: &str) -> String {
    format!(
        "<tr><td>{link}</td><td>{what}</td><td>{}</td></tr>\n",
        pages::escape(address)
    )
}

/// A link to the page of the thing a hit is about, labelled with its handle.
fn link_to(id: &EntityId, by_id: &HashMap<&EntityId, &Entity>) -> String {
    match by_id.get(id) {
        Some(entity) => format!(
            "<a href=\"{}\">{}</a>",
            pages::escape(&tree::canonical_path(entity, by_id)),
            pages::escape(id.as_str())
        ),
        None => pages::escape(id.as_str()),
    }
}
