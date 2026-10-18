//! **The one page style, and the HTML every page is built from.**
//!
//! One stylesheet, taken from the approved mockup, is served inside every page,
//! so a page is judged against the mockup and no page carries a look of its own.
//! The shell around a page's content is the header (the mark and the breadcrumbs
//! of the page's handle path) and the `main` the content sits in. The box grid,
//! the row table and the fold are the shapes a page's content is made of.
//!
//! **The helpers take HTML that is already escaped** and say so: the caller knows
//! which of its strings are text a person wrote ([`escape`]) and which are
//! markup it built, and a second escape here would turn the second into the
//! first.

use crate::ui::pages::escape;

/// The stylesheet of every page: the approved mockup's, unchanged. Indigo is
/// structure, vermilion is live, and detail sits behind a ▸ that the
/// stylesheet draws, so a fold's summary holds its label and no glyph.
pub const STYLESHEET: &str = include_str!("style.css");

/// The document around a page's content. `path` is the page's handle path
/// (`/` for the index of the roots); a page with no path of its own passes an
/// empty one and gets a link home.
pub fn page(title: &str, path: &str, content: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLESHEET}</style>\n</head>\n<body>\n\
         <header class=\"bar\"><span class=\"mark\">jojobot</span>\
         <span class=\"path\">{}</span></header>\n<main>\n{content}</main>\n</body>\n</html>\n",
        escape(title),
        breadcrumbs(path),
    )
}

/// **The way up from a page**: `/` and each handle on the path, every one a link
/// to its own page except the page being read.
fn breadcrumbs(path: &str) -> String {
    let steps: Vec<&str> = path.split('/').filter(|step| !step.is_empty()).collect();
    if steps.is_empty() {
        return if path.is_empty() {
            "<a href=\"/\">/</a>".to_string()
        } else {
            "/".to_string()
        };
    }
    let mut out = String::from("<a href=\"/\">/</a>");
    let mut href = String::from("/");
    for (index, step) in steps.iter().enumerate() {
        href.push_str(step);
        href.push('/');
        out.push_str(if index == 0 { " " } else { " / " });
        if index + 1 == steps.len() {
            out.push_str(&escape(step));
        } else {
            out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                escape(&href),
                escape(step)
            ));
        }
    }
    out
}

/// **The address of a view's page with its inputs filled**: `view` is the view's
/// handle, which the views the build supplies wear at the root, and each input
/// is sent under its own name, as the view's form sends it. The one place a page
/// spells a view's input URL (rule 498).
pub fn view_url(view: &str, inputs: &[(&str, &str)]) -> String {
    let mut url = format!("/{view}/");
    for (index, (name, value)) in inputs.iter().enumerate() {
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(&crate::ui::encode_component(name));
        url.push('=');
        url.push_str(&crate::ui::encode_component(value));
    }
    url
}

/// **Detail behind a ▸**: `label` is what the reader sees closed, `inside` is
/// what opens. Both are HTML. The ▸ is the stylesheet's, so an empty label is a
/// bare ▸.
pub fn fold(label: &str, inside: &str) -> String {
    format!("<details><summary>{label}</summary>{inside}</details>")
}

/// **One box of the grid**: a heading, a count or note beside it, and the content.
pub fn boxed(heading: &str, note: &str, inside: &str) -> String {
    format!(
        "<section class=\"box\"><h2>{heading} <span class=\"n\">{note}</span></h2>{inside}</section>"
    )
}

/// **The grid the boxes sit in.**
pub fn grid(boxes: &[String]) -> String {
    format!("<div class=\"grid\">{}</div>", boxes.concat())
}

/// **The row table**: a header of labels and one row per `rows` entry, a cell to
/// each label.
pub fn row_table(labels: &[&str], rows: &[Vec<String>]) -> String {
    let head: String = labels.iter().map(|l| format!("<th>{l}</th>")).collect();
    let body: String = rows
        .iter()
        .map(|cells| {
            let cells: String = cells.iter().map(|c| format!("<td>{c}</td>")).collect();
            format!("<tr>{cells}</tr>")
        })
        .collect();
    format!("<table class=\"t\"><tr>{head}</tr>{body}</table>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breadcrumbs_link_every_step_but_the_last() {
        assert_eq!(breadcrumbs("/"), "/");
        assert_eq!(breadcrumbs(""), "<a href=\"/\">/</a>");
        assert_eq!(
            breadcrumbs("/person:alpha/topic:widgets/"),
            "<a href=\"/\">/</a> <a href=\"/person:alpha/\">person:alpha</a> / topic:widgets"
        );
    }

    #[test]
    fn a_views_address_carries_each_input_under_its_name() {
        assert_eq!(view_url("view:ready", &[]), "/view:ready/");
        assert_eq!(
            view_url("view:ready", &[("project", "project:atlas")]),
            "/view:ready/?project=project%3Aatlas"
        );
        assert_eq!(
            view_url("view:due", &[("by", "today+3d"), ("note", "a & b")]),
            "/view:due/?by=today%2B3d&note=a%20%26%20b"
        );
    }

    #[test]
    fn the_shapes_hold_what_they_are_given_and_nothing_else() {
        assert_eq!(
            fold("", "<p>x</p>"),
            "<details><summary></summary><p>x</p></details>"
        );
        assert_eq!(
            grid(&[boxed("about", "2", "<p>x</p>")]),
            "<div class=\"grid\"><section class=\"box\"><h2>about <span class=\"n\">2</span></h2><p>x</p></section></div>"
        );
        assert_eq!(
            row_table(&["path"], &[vec!["a".into()], vec!["b".into()]]),
            "<table class=\"t\"><tr><th>path</th></tr><tr><td>a</td></tr><tr><td>b</td></tr></table>"
        );
    }
}
