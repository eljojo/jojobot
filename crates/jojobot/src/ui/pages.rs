//! The pages themselves — HTML written by hand, because a directory listing is
//! a heading and a table and nothing that needs a template engine.

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use jojobot_domain::memory::Entity;

use crate::AppState;

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
            tracing::warn!(error = %err, "the listing could not read the store");
            return crate::ui::refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read its own store, so this page would be empty rather than \
                 accurate.",
            );
        }
    };

    let mut roots: Vec<Entity> = entities
        .into_iter()
        .filter(|entity| entity.parent.is_none())
        .collect();
    roots.sort_by(|a, b| a.id.cmp(&b.id));

    let mut rows = String::new();
    for entity in &roots {
        rows.push_str(&row(entity));
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

/// One entity as a row: its handle, linking to its own index, and the name it
/// goes by.
fn row(entity: &Entity) -> String {
    let handle = escape(entity.id.as_str());
    format!(
        "<tr><td><a href=\"/{handle}/\">{handle}/</a></td><td>{}</td></tr>\n",
        escape(&entity.name)
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
