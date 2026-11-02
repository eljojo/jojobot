//! **The search page: the box in every page's bar asks the search the bots use.**
//!
//! The words go to the query handle's `search`, which builds the query the
//! `search` verb builds, for the operator, who is shown everything. Looking
//! writes nothing and takes no delivery: mail is not searched here, because
//! reading a message is not looking at it.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    response::Response,
};
use jojobot_domain::actor::Actor;
use jojobot_domain::memory::search::Hit;
use jojobot_domain::memory::{EntityId, MemoryError};
use jojobot_query_api::{Failed, QueryHandle, SearchQuestion};

use crate::{pages, shell};

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

pub(crate) async fn results(
    State(node): State<crate::PageState>,
    Query(asked): Query<Asked>,
) -> Response {
    let queries = node.queries;
    let words = asked.q.trim();
    let mut body = format!("<h1>Search</h1>\n{}", shell::search_form(words));
    if words.is_empty() {
        return pages::html(&shell::page("Search", "", &body));
    }
    let searched = match queries
        .search(
            &Actor::Operator,
            SearchQuestion {
                words: words.to_string(),
            },
        )
        .await
    {
        Ok(searched) => searched,
        Err(err) => return pages::unreadable(err),
    };
    let unseen = (&searched.index_note, &searched.matching_note);
    let mut paths: HashMap<EntityId, Option<String>> = HashMap::new();
    let mut rows = String::new();
    let mut not_listed = 0usize;
    for hit in &searched.hits {
        match hit {
            Hit::Entity { entity, .. } => {
                let link = match link_to(&queries, &mut paths, &entity.id).await {
                    Ok(link) => link,
                    Err(err) => return pages::unreadable(err),
                };
                rows.push_str(&row(
                    &link,
                    &pages::escape(&entity.name),
                    entity.id.as_str(),
                ));
            }
            Hit::Fact { fact, subject, .. } => {
                let claim: String = fact.content.chars().take(CLAIM_CHARS).collect();
                let link = match link_to(&queries, &mut paths, &subject.id).await {
                    Ok(link) => link,
                    Err(err) => return pages::unreadable(err),
                };
                rows.push_str(&row(
                    &link,
                    &pages::escape(&claim),
                    &fact.address().to_string(),
                ));
            }
            _ => not_listed += 1,
        }
    }
    if rows.is_empty() {
        // Hits counted below are something found: only a search that counted none says so.
        if not_listed == 0 {
            body.push_str("<p id=\"nothing-found\">Nothing found.</p>\n");
        }
    } else {
        body.push_str(&format!(
            "<table id=\"found\">\n<tr><th>Where</th><th>What</th><th>Address</th></tr>\n{rows}</table>\n"
        ));
    }
    if not_listed > 0 {
        body.push_str(&format!(
            "<p id=\"not-listed\">{}</p>\n",
            pages::escape(&jojobot_query_api::words::not_listed_note(not_listed))
        ));
    }
    if let Some(note) = unseen.0 {
        body.push_str(&format!(
            "<p id=\"search-behind\">{}</p>\n",
            pages::escape(note)
        ));
    }
    if let Some(note) = unseen.1 {
        body.push_str(&format!(
            "<p id=\"search-matching\">{}</p>\n",
            pages::escape(note)
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
/// The path is the thing's ancestry as the query handle gives it, asked once per
/// thing. A thing the graph does not answer to is shown as its handle alone; a
/// read that failed is the page's failure, never a shorter path.
async fn link_to(
    queries: &QueryHandle,
    paths: &mut HashMap<EntityId, Option<String>>,
    id: &EntityId,
) -> Result<String, MemoryError> {
    if !paths.contains_key(id) {
        let path = canonical_path(queries, id).await?;
        paths.insert(id.clone(), path);
    }
    Ok(match &paths[id] {
        Some(path) => format!(
            "<a href=\"{}\">{}</a>",
            pages::escape(path),
            pages::escape(id.as_str())
        ),
        None => pages::escape(id.as_str()),
    })
}

/// The path of a thing, or `None` for a thing nobody answers to.
async fn canonical_path(
    queries: &QueryHandle,
    id: &EntityId,
) -> Result<Option<String>, MemoryError> {
    let answer = match queries
        .ask(
            &Actor::Operator,
            jojobot_query_api::Question::about(id.clone(), None, None),
        )
        .await
    {
        Ok(answer) => answer,
        Err(Failed::Selection(MemoryError::UnknownEntity { .. })) => return Ok(None),
        Err(failed) => return Err(unread(failed)),
    };
    let Some(thing) = answer.selected.subject_object(id) else {
        return Ok(None);
    };
    let chain =
        jojobot_query_api::ancestry(queries, &Actor::Operator, id, thing.entity.parent.as_ref())
            .await
            .map_err(unread)?;
    Ok(Some(format!(
        "/{}/",
        chain
            .iter()
            .map(EntityId::as_str)
            .collect::<Vec<_>>()
            .join("/")
    )))
}

fn unread(failed: Failed) -> MemoryError {
    match failed {
        Failed::Selection(error) | Failed::Readiness(error) | Failed::Preparation(_, error) => {
            error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_query_api::{Answered, Answerer, Question, Searched};
    use std::sync::Arc;

    /// A handle whose every ask fails: as a store outage does, or by saying the
    /// thing is unknown.
    struct Failing {
        unknown: bool,
    }

    #[async_trait::async_trait]
    impl Answerer for Failing {
        async fn ask(&self, _: &Actor, _: Question) -> Result<Answered, Failed> {
            Err(Failed::Selection(if self.unknown {
                MemoryError::UnknownEntity {
                    attempted: "work:sigma".into(),
                    nearest: Vec::new(),
                }
            } else {
                MemoryError::Store("the store is down".into())
            }))
        }

        async fn search(&self, _: &Actor, _: SearchQuestion) -> Result<Searched, MemoryError> {
            Err(MemoryError::Store("the store is down".into()))
        }

        async fn entity_index(
            &self,
            _: &Actor,
        ) -> Result<Vec<jojobot_domain::memory::Entity>, MemoryError> {
            Err(MemoryError::Store("the store is down".into()))
        }
    }

    fn handle(unknown: bool) -> QueryHandle {
        QueryHandle::from(Arc::new(Failing { unknown }) as Arc<dyn Answerer>)
    }

    #[tokio::test]
    async fn a_failed_read_of_a_hits_path_is_an_error_and_an_unknown_thing_has_none() {
        let id = EntityId("work:sigma".into());
        let failed = canonical_path(&handle(false), &id).await;
        assert!(
            matches!(failed, Err(MemoryError::Store(_))),
            "a failed read is the page's failure: {failed:?}"
        );
        let unknown = canonical_path(&handle(true), &id).await;
        assert!(
            matches!(unknown, Ok(None)),
            "a thing nobody answers to has no path: {unknown:?}"
        );
    }
}
