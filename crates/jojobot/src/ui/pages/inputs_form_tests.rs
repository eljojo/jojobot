//! The form a view's inputs make: one field to an input, typed as the input is.

use super::linkify_tests::root;
use super::*;

fn declared() -> Vec<view_inputs::InputDecl> {
    view_inputs::parse("project: kind project, before: date?, size: number?, note: text?")
        .unwrap_or_else(|refused| panic!("the declaration reads: {refused:?}"))
}

/// **Each declared type is its own kind of field, and only a required input is
/// required.** A handle of a kind is text offering the things of that kind and no
/// other, in handle order; a day is a date, a number is a number with any step, and
/// words are text. Paired: the three optional inputs carry no `required`, so a form
/// that required everything, or nothing, fails one half.
#[test]
fn each_declared_type_is_its_own_kind_of_field_and_only_a_required_input_is_required() {
    let known = [
        root("project:the-move"),
        root("project:atlas"),
        root("person:alpha"),
    ];
    let form = inputs_form(&declared(), &BTreeMap::new(), &known);
    for expected in [
        "name=\"project\" required list=\"inputs-project\" value=\"\"",
        "<datalist id=\"inputs-project\"><option value=\"project:atlas\"><option value=\"project:the-move\"></datalist>",
        "type=\"date\" name=\"before\" value=\"\"",
        "type=\"number\" name=\"size\" step=\"any\" value=\"\"",
        "type=\"text\" name=\"note\" value=\"\"",
    ] {
        assert!(form.contains(expected), "{expected} is missing: {form}");
    }
    assert!(
        !form.contains("person:alpha"),
        "only the things of the declared kind are offered: {form}"
    );
    assert_eq!(form.matches(" required").count(), 1, "{form}");
}

/// **The form comes back holding what the URL sent, and a relative day stays
/// text** because a date field cannot show it. Paired: a plain day is a date field.
#[test]
fn the_form_holds_what_the_url_sent_and_keeps_a_relative_day_as_text() {
    let sent = |before: &str| {
        BTreeMap::from([
            ("project".to_string(), "project:atlas".to_string()),
            ("before".to_string(), before.to_string()),
        ])
    };
    let relative = inputs_form(&declared(), &sent("today+7d"), &[]);
    assert!(
        relative
            .contains("name=\"project\" required list=\"inputs-project\" value=\"project:atlas\""),
        "{relative}"
    );
    assert!(
        relative.contains("type=\"text\" name=\"before\" value=\"today+7d\""),
        "{relative}"
    );
    let plain = inputs_form(&declared(), &sent("2026-03-05"), &[]);
    assert!(
        plain.contains("type=\"date\" name=\"before\" value=\"2026-03-05\""),
        "{plain}"
    );
}

/// **A view that takes no inputs has no form.**
#[test]
fn a_view_with_no_inputs_has_no_form() {
    assert_eq!(inputs_form(&[], &BTreeMap::new(), &[]), "");
}
