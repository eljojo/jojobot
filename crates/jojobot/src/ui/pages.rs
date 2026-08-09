//! The pages themselves — HTML written by hand, because a directory listing is
//! a heading and a table and nothing that needs a template engine.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};

use jojobot_domain::memory::{Entity, EntityId, Fact};

use crate::AppState;
use crate::ui::tree;

/// `/` — the index of the roots: every entity that sits under nothing.
///
/// The roots are where the tree starts, so this is the top of the listing.
/// Descending is a link away and a level at a time, which is what the tree is
/// for: a page of everything jojobot knows is the silting the tree exists to
/// stop.
pub async fn index(State(state): State<AppState>) -> Response {
    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => {
            return unreadable(err);
        }
    };

    let mut roots: Vec<Entity> = entities
        .into_iter()
        .filter(|entity| entity.parent.is_none())
        .collect();
    roots.sort_by(|a, b| a.id.cmp(&b.id));

    let mut rows = String::new();
    for entity in &roots {
        rows.push_str(&row(entity, "/"));
    }
    if roots.is_empty() {
        rows.push_str("<tr><td colspan=\"2\">Nothing is filed here yet.</td></tr>");
    }

    html(&page(
        "Index of /",
        &format!(
            "<h1>Index of /</h1>\n<table>\n<tr><th>Name</th><th>Called</th></tr>\n{rows}</table>\n"
        ),
    ))
}

/// One entity as a row: its handle, linking to where it lives, and the name it
/// goes by.
fn row(entity: &Entity, at: &str) -> String {
    let handle = escape(entity.id.as_str());
    format!(
        "<tr><td><a href=\"{}{handle}/\">{handle}/</a></td><td>{}</td></tr>\n",
        escape(at),
        escape(&entity.name)
    )
}

/// `/{path}` — one node: what sits below it, and the facts held there.
///
/// **The path is the entity's ancestry, so there is one URL per entity.** A
/// handle reached by any other path is the same entity somewhere it does not
/// live, and the browser is sent to where it does — a second URL serving the
/// same node is how two readers come to disagree about where something is.
pub async fn node(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    let Some(handles) = tree::segments(&path) else {
        return not_found();
    };

    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => return unreadable(err),
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();

    let wanted = handles
        .last()
        .expect("segments never returns an empty path");
    let Some(entity) = by_id.get(wanted).copied() else {
        return not_found();
    };

    let canonical = tree::canonical_path(entity, &by_id);
    if canonical.trim_matches('/') != path.trim_matches('/') {
        return Redirect::to(&canonical).into_response();
    }

    let mut children: Vec<&Entity> = entities
        .iter()
        .filter(|candidate| candidate.parent.as_ref() == Some(&entity.id))
        .collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));

    let facts = match state.memory.recall(&entity.id).await {
        Ok(facts) => facts,
        Err(err) => return unreadable(err),
    };

    // Prose is the human half of the record — a charter, a portrait. Its
    // absence is ordinary, and a store that cannot be asked for it costs the
    // prose rather than the page.
    let prose = match state.memory.scan_entity(&entity.id).await {
        Ok(Some(scan)) => scan.prose,
        Ok(None) => String::new(),
        Err(err) => {
            tracing::debug!(error = %err, entity = %entity.id, "the listing could not read prose");
            String::new()
        }
    };

    let mut body = format!(
        "<h1>Index of {}</h1>\n{}",
        escape(&canonical),
        trail(&handles)
    );
    body.push_str(&about(entity));

    body.push_str("<h2>Below here</h2>\n");
    if children.is_empty() {
        body.push_str("<p>Nothing sits under this.</p>\n");
    } else {
        body.push_str("<table>\n<tr><th>Name</th><th>Called</th></tr>\n");
        for child in children {
            body.push_str(&row(child, &canonical));
        }
        body.push_str("</table>\n");
    }

    body.push_str(&facts_table(&facts, &by_id));

    if !prose.trim().is_empty() {
        body.push_str(&format!("<h2>Prose</h2>\n<pre>{}</pre>\n", escape(&prose)));
    }

    html(&page(&format!("Index of {canonical}"), &body))
}

/// The path as links, one per ancestor, so any level is a click away.
fn trail(handles: &[EntityId]) -> String {
    let mut out = String::from("<p><a href=\"/\">/</a>");
    let mut so_far = String::from("/");
    for handle in handles {
        so_far.push_str(handle.as_str());
        so_far.push('/');
        out.push_str(&format!(
            "<a href=\"{}\">{}/</a>",
            escape(&so_far),
            escape(handle.as_str())
        ));
    }
    out.push_str("</p>\n");
    out
}

/// What the record itself says about this entity, beyond the facts on it.
fn about(entity: &Entity) -> String {
    let mut rows = format!(
        "<tr><td>name</td><td>{}</td></tr>\n<tr><td>kind</td><td>{}</td></tr>\n\
         <tr><td>source</td><td>{}</td></tr>\n",
        escape(&entity.name),
        escape(entity.kind.as_token()),
        escape(&entity.source),
    );
    if !entity.aliases.is_empty() {
        rows.push_str(&format!(
            "<tr><td>also called</td><td>{}</td></tr>\n",
            escape(&entity.aliases.join(", "))
        ));
    }
    if let Some(crm) = &entity.crm {
        rows.push_str(&format!("<tr><td>crm</td><td>{}</td></tr>\n", escape(crm)));
    }
    format!("<table>\n{rows}</table>\n")
}

/// The facts held at this node.
///
/// **Every claim arrives with what qualifies it.** Who backs it and how settled
/// it is are what tell a claim from a hypothesis, so they are columns rather
/// than something a reader has to go and ask for.
fn facts_table(facts: &[Fact], by_id: &HashMap<&EntityId, &Entity>) -> String {
    if facts.is_empty() {
        return "<h2>Facts</h2>\n<p>Nothing is recorded here.</p>\n".to_string();
    }

    let mut out = String::from(
        "<h2>Facts</h2>\n<table>\n<tr><th>Claim</th><th>Backed by</th><th>Standing</th>\
         <th>State</th><th>Date</th><th>Relation</th><th>Address</th></tr>\n",
    );
    for fact in facts {
        let claim = match &fact.details {
            Some(details) if !details.trim().is_empty() => format!(
                "{}<br><small>{}</small>",
                escape(&fact.content),
                escape(details)
            ),
            _ => escape(&fact.content),
        };
        out.push_str(&format!(
            "<tr><td>{claim}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td>{}</td></tr>\n",
            escape(fact.provenance.as_token()),
            escape(fact.standing.as_token()),
            escape(fact.status.as_token()),
            escape(&fact.date.to_string()),
            relation(fact, by_id),
            escape(&fact.address().to_string()),
        ));
    }
    out.push_str("</table>\n");
    out
}

/// The relation a fact draws, as a link to where that entity lives.
///
/// The shape is named beside the link, because the shape IS what the link
/// means — and `connection` in particular says a link is there and that how it
/// relates was not recorded, which is a different claim from any of the others.
fn relation(fact: &Fact, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let Some(edge) = &fact.edge else {
        return String::new();
    };
    let shape = escape(edge.shape.as_token());
    match by_id.get(&edge.object).copied() {
        Some(object) => format!(
            "{shape} → <a href=\"{}\">{}</a>",
            escape(&tree::canonical_path(object, by_id)),
            escape(edge.object.as_str())
        ),
        // The object of an edge must exist to be written, so a miss here is a
        // record edited outside jojobot. Naming it beats a dead link.
        None => format!("{shape} → {} (not found)", escape(edge.object.as_str())),
    }
}

/// Nothing is filed at that path. Public, because the gate answers with it too:
/// a path that is not a handle path is missing whether or not anybody is
/// logged in.
pub fn not_found() -> Response {
    crate::ui::refuse(
        StatusCode::NOT_FOUND,
        "No entity has that handle. Nothing is filed at this path.",
    )
}

fn unreadable(err: jojobot_domain::memory::MemoryError) -> Response {
    tracing::warn!(error = %err, "the listing could not read the store");
    crate::ui::refuse(
        StatusCode::BAD_GATEWAY,
        "jojobot could not read its own store, so this page would be empty rather than accurate.",
    )
}

/// A page with a sentence on it — a refusal, or anything else with nothing to
/// list.
pub fn plain_page(title: &str, message: &str) -> String {
    page(
        title,
        &format!("<h1>{}</h1>\n<p>{}</p>\n", escape(title), escape(message)),
    )
}

/// The document around a page's body.
fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(title)
    )
}

/// Enough style to be readable and no more. A directory listing that grew a
/// design would be the application this is deliberately not.
const STYLE: &str = "body{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;\
                     max-width:60rem}table{border-collapse:collapse}\
                     td,th{text-align:left;padding:.15rem 1.5rem .15rem 0}\
                     th{border-bottom:1px solid currentColor}a{text-decoration:none}\
                     a:hover{text-decoration:underline}";

/// Escape text for HTML. Handles are validated to a narrow alphabet, but a
/// name, a fact and a piece of prose are free text a person wrote.
pub fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for character in raw.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn html(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn a_name_a_person_wrote_cannot_close_a_tag() {
        assert_eq!(
            escape("<script>alert(\"x\")</script>"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
        );
        assert_eq!(escape("Ampersand & co"), "Ampersand &amp; co");
    }
}
