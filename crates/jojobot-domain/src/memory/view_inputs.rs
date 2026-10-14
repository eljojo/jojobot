//! **The inputs a view declares, and what a call binds them to.**
//!
//! A view is a saved recall, and a saved recall with a hole in it is a question
//! somebody can ask by filling the hole: *the work that can start under this
//! project*. The view says what it takes, in its `inputs` key, as a comma list of
//! `name: type`, where the type is `kind <kind>` (a handle of that kind), `date`,
//! `number` or `text`, and a trailing `?` marks one that may be left out
//! (rule 473).
//!
//! **A value is checked as a write is checked.** A handle must exist and be of the
//! declared kind, and it is bound to the thing it names, so a handle sent after a
//! rename still reaches the thing; a date takes the relative days a filter takes;
//! a number must parse. A call that sends no value for a required input, a value
//! for an input the view does not declare, or one of the wrong type is refused,
//! and the refusal names every input the view declares with its type, because a
//! session meeting the view cold learns its shape from that refusal.
//!
//! This module reads and checks. It runs nothing: what a bound input does to the
//! view's `where` and `parent` is [`super::filter::expression::with_inputs`] and
//! the caller's.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::guard::EntityMatch;
use super::{Entity, EntityId, FormerHandle};

/// **The view's key that declares its inputs.**
pub const KEY: &str = "inputs";

/// What an input holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputType {
    /// A handle of this kind.
    Kind(String),
    /// A day, spelled as a date or relative to the asker's own.
    Date,
    /// A number.
    Number,
    /// Words.
    Text,
}

/// One declared input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDecl {
    /// The name `$name` uses in the view's question.
    pub name: String,
    /// What it holds.
    pub ty: InputType,
    /// Whether a call may leave it out.
    pub optional: bool,
}

/// What a declaration or a call was refused for, and the nearest things when a
/// handle named nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// The refusal, in words a caller can act on.
    pub words: String,
    /// The things nearest a handle that named nothing, as a write names them.
    pub nearest: Vec<EntityMatch>,
}

/// One input's value, as the question reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// The text that stands in the question: a day as a date, a number as sent, a
    /// handle as the thing wears it now, words as sent.
    pub text: String,
    /// The thing a `kind` input names, bound to the one it is.
    pub handle: Option<EntityId>,
}

/// **What a call bound the declared inputs to.** An input left out is present
/// with no value, so a question using it can tell *left out* from *not declared*.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bound {
    values: BTreeMap<String, Option<Value>>,
}

impl Bound {
    /// The value of a declared input: `Some(None)` when it was left out, `None`
    /// when no input of that name is declared.
    pub fn get(&self, name: &str) -> Option<Option<&Value>> {
        self.values.get(name).map(Option::as_ref)
    }

    /// Whether any input is declared.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// **The words a type is spelled with.**
fn spelling(ty: &InputType) -> String {
    match ty {
        InputType::Kind(kind) => format!("kind {kind}"),
        InputType::Date => "date".to_string(),
        InputType::Number => "number".to_string(),
        InputType::Text => "text".to_string(),
    }
}

fn refusal(words: impl Into<String>) -> Refusal {
    Refusal {
        words: words.into(),
        nearest: Vec::new(),
    }
}

/// Whether a word is an input's name: a lowercase letter, then lowercase letters,
/// digits and `_`, which is what `$name` reads.
fn is_a_name(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether a word is a kind's name, as a handle writes it.
fn is_a_kind(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// **Read a view's `inputs`.** An empty list declares none.
pub fn parse(list: &str) -> Result<Vec<InputDecl>, Refusal> {
    let mut read: Vec<InputDecl> = Vec::new();
    for piece in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let Some((name, ty)) = piece.split_once(':') else {
            return Err(refusal(format!(
                "the input '{piece}' is not written `name: type`. Write each input as a name, a \
                 colon and a type: `project: kind project?`, `due: date`"
            )));
        };
        let name = name.trim();
        if !is_a_name(name) {
            return Err(refusal(format!(
                "'{name}' is not a name an input can have. A name is lowercase letters, digits \
                 and _, and starts with a letter"
            )));
        }
        let ty = ty.trim();
        let (ty, optional) = match ty.strip_suffix('?') {
            Some(rest) => (rest.trim(), true),
            None => (ty, false),
        };
        let ty = match ty {
            "date" => InputType::Date,
            "number" => InputType::Number,
            "text" => InputType::Text,
            other => match other.strip_prefix("kind").map(str::trim) {
                Some(kind) if is_a_kind(kind) => InputType::Kind(kind.to_string()),
                Some(_) => {
                    return Err(refusal(format!(
                        "the input '{name}' is a `kind` and does not say which: write `kind \
                         project`, the kind of thing it takes"
                    )));
                }
                None => {
                    return Err(refusal(format!(
                        "'{other}' is not a type for the input '{name}'. A type is `kind <kind>`, \
                         `date`, `number` or `text`, with a trailing `?` when it may be left out"
                    )));
                }
            },
        };
        if read.iter().any(|d| d.name == name) {
            return Err(refusal(format!(
                "the input '{name}' is declared twice. Each input has one name and one type"
            )));
        }
        read.push(InputDecl {
            name: name.to_string(),
            ty,
            optional,
        });
    }
    Ok(read)
}

/// **The inputs as a reader is told them**: `project: kind project?, due: date`.
pub fn spelled(decls: &[InputDecl]) -> String {
    decls
        .iter()
        .map(|d| {
            format!(
                "{}: {}{}",
                d.name,
                spelling(&d.ty),
                if d.optional { "?" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// **The sentence that tells a caller what the view takes.**
fn takes(view: &str, decls: &[InputDecl]) -> String {
    match decls.is_empty() {
        true => format!("{view} takes no inputs"),
        false => format!("{view} takes these inputs: {}", spelled(decls)),
    }
}

/// **Check what a call sent against what the view declares, and bind it.**
/// `today` is the asker's day, which a relative date is read against.
pub fn bind(
    view: &str,
    decls: &[InputDecl],
    given: &BTreeMap<String, String>,
    known: &[Entity],
    former: &[FormerHandle],
    today: Date,
) -> Result<Bound, Refusal> {
    if let Some(name) = given
        .keys()
        .find(|name| !decls.iter().any(|d| &d.name == *name))
    {
        return Err(refusal(format!(
            "Nothing was read: '{name}' is not an input of {view}. {}",
            takes(view, decls)
        )));
    }
    let mut values = BTreeMap::new();
    for decl in decls {
        let Some(sent) = given.get(&decl.name).map(|v| v.trim()) else {
            if decl.optional {
                values.insert(decl.name.clone(), None);
                continue;
            }
            return Err(refusal(format!(
                "Nothing was read: {view} needs a value for its input '{}'. {}",
                decl.name,
                takes(view, decls)
            )));
        };
        let wrong = |why: String| {
            refusal(format!(
                "Nothing was read: the input '{}' of {view} is {}, and {why}. {}",
                decl.name,
                spelling(&decl.ty),
                takes(view, decls)
            ))
        };
        let value = match &decl.ty {
            InputType::Text if sent.is_empty() => {
                return Err(wrong("it was sent empty".to_string()));
            }
            InputType::Text => Value {
                text: sent.to_string(),
                handle: None,
            },
            InputType::Number => match sent.parse::<f64>() {
                Ok(number) if number.is_finite() => Value {
                    text: sent.to_string(),
                    handle: None,
                },
                _ => return Err(wrong(format!("'{sent}' is not a number"))),
            },
            InputType::Date => match super::filter::expression::date_word(sent, today) {
                Ok(day) => Value {
                    text: day,
                    handle: None,
                },
                Err(super::MemoryError::InvalidQuery(why)) => return Err(wrong(why)),
                Err(other) => return Err(wrong(other.to_string())),
            },
            InputType::Kind(kind) => {
                let id = EntityId(sent.to_string());
                if id.kind().is_none_or(|k| k.as_token() != kind) {
                    return Err(wrong(format!("'{sent}' is not a handle of kind {kind}")));
                }
                let Some(entity) = super::resolve_handle(&id, known, former) else {
                    let mut refused = wrong(format!("'{sent}' names nothing"));
                    refused.nearest = super::guard::screen(&id, &[], known);
                    return Err(refused);
                };
                if entity.id.kind().is_none_or(|k| k.as_token() != kind) {
                    return Err(wrong(format!(
                        "'{}' is not a thing of kind {kind}",
                        entity.id.as_str()
                    )));
                }
                Value {
                    text: entity.id.to_string(),
                    handle: Some(entity.id.clone()),
                }
            }
        };
        values.insert(decl.name.clone(), Some(value));
    }
    Ok(Bound { values })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(handle: &str, badge: &str) -> Entity {
        crate::memory::kinds::load_shipped();
        Entity {
            id: EntityId(handle.to_string()),
            kind: EntityId(handle.to_string())
                .kind()
                .expect("a handle of a shipped kind"),
            name: handle.to_string(),
            aliases: Vec::new(),
            source: "test".to_string(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: Some(badge.to_string()),
            archived: None,
        }
    }

    fn decls(list: &str) -> Vec<InputDecl> {
        parse(list).expect("a list that reads")
    }

    fn given(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const TODAY: Date = Date::constant(2026, 10, 14);

    /// **The declaration reads, and reads back as written.** Every type, with and
    /// without the `?`, and the list spelled the way a reader is told it.
    #[test]
    fn a_declaration_reads_every_type_and_spells_itself_back() {
        let read = decls("project: kind project?, due: date, count: number?, note: text");
        assert_eq!(
            read,
            vec![
                InputDecl {
                    name: "project".into(),
                    ty: InputType::Kind("project".into()),
                    optional: true
                },
                InputDecl {
                    name: "due".into(),
                    ty: InputType::Date,
                    optional: false
                },
                InputDecl {
                    name: "count".into(),
                    ty: InputType::Number,
                    optional: true
                },
                InputDecl {
                    name: "note".into(),
                    ty: InputType::Text,
                    optional: false
                },
            ]
        );
        assert_eq!(
            spelled(&read),
            "project: kind project?, due: date, count: number?, note: text"
        );
        assert!(decls("").is_empty());
        assert!(decls("  ,  ").is_empty());
    }

    /// **A declaration that does not read is refused, naming what it found.**
    /// **Paired** with the case above: the lists that read are the ones refused
    /// here with one word changed.
    #[test]
    fn a_declaration_that_does_not_read_is_refused() {
        for (list, why) in [
            ("project kind project", "name: type"),
            ("project: place?", "type"),
            ("Project: date", "name"),
            ("when: date, when: number", "twice"),
            ("owner: kind", "kind"),
        ] {
            let refused = parse(list).expect_err(list);
            assert!(
                refused.words.contains(why),
                "{list:?} was refused for the wrong reason: {}",
                refused.words
            );
        }
    }

    /// **A handle input is bound to the thing it names**, even when the call sent
    /// the handle the thing wore before a rename. **Paired**: the same input left
    /// out is bound to nothing when the view allows it.
    #[test]
    fn a_handle_binds_to_the_thing_and_follows_a_rename() {
        let known = vec![entity("project:krusty-burger-app", "k7h2mn")];
        let former = vec![FormerHandle {
            former: EntityId("project:krusty-app".into()),
            badge: "k7h2mn".into(),
            changed_at: TODAY,
        }];
        let view = decls("project: kind project?");
        let bound = bind(
            "view:ready",
            &view,
            &given(&[("project", "project:krusty-app")]),
            &known,
            &former,
            TODAY,
        )
        .expect("a handle the thing wore binds");
        let value = bound.get("project").flatten().expect("a value");
        assert_eq!(value.text, "project:krusty-burger-app");
        assert_eq!(
            value.handle.as_ref().map(EntityId::as_str),
            Some("project:krusty-burger-app")
        );

        let left_out = bind("view:ready", &view, &given(&[]), &known, &former, TODAY)
            .expect("an optional input may be left out");
        assert_eq!(left_out.get("project"), Some(None));
        assert_eq!(left_out.get("projekt"), None);
    }

    /// **A call is refused when it sends no value for a required input, a value
    /// for an input nobody declared, or a value of the wrong type**, and each
    /// refusal names every declared input with its type. **Paired** with the case
    /// above: the same view takes the right call.
    #[test]
    fn a_call_that_does_not_fit_is_refused_naming_every_input() {
        let known = vec![
            entity("project:krusty-burger-app", "k7h2mn"),
            entity("person:bob", "b0bb0b"),
        ];
        let view = decls("project: kind project, due: date?, count: number?");
        let declared = "project: kind project, due: date?, count: number?";
        let refuse = |pairs: &[(&str, &str)]| {
            bind("view:ready", &view, &given(pairs), &known, &[], TODAY).expect_err("a refusal")
        };

        let missing = refuse(&[]);
        assert!(missing.words.contains("project"), "{}", missing.words);
        assert!(missing.words.contains(declared), "{}", missing.words);

        let unknown = refuse(&[("projekt", "project:krusty-burger-app")]);
        assert!(unknown.words.contains("projekt"), "{}", unknown.words);
        assert!(unknown.words.contains(declared), "{}", unknown.words);

        let wrong_kind = refuse(&[("project", "person:bob")]);
        assert!(
            wrong_kind.words.contains("person:bob"),
            "{}",
            wrong_kind.words
        );
        assert!(wrong_kind.words.contains(declared), "{}", wrong_kind.words);

        // A handle of the wrong kind is refused for its kind even when nothing
        // wears it, and a word that is no handle is refused for being none.
        let wrong_and_absent = refuse(&[("project", "person:nelson")]);
        assert!(
            wrong_and_absent
                .words
                .contains("not a handle of kind project"),
            "{}",
            wrong_and_absent.words
        );
        let no_handle = refuse(&[("project", "banana")]);
        assert!(
            no_handle.words.contains("not a handle of kind project"),
            "{}",
            no_handle.words
        );

        let nothing = refuse(&[("project", "project:homers-tree-house")]);
        assert!(
            nothing.words.contains("project:homers-tree-house"),
            "{}",
            nothing.words
        );

        let not_a_date = refuse(&[("project", "project:krusty-burger-app"), ("due", "soon")]);
        assert!(not_a_date.words.contains("due"), "{}", not_a_date.words);
        let not_a_number = refuse(&[("project", "project:krusty-burger-app"), ("count", "many")]);
        assert!(
            not_a_number.words.contains("count"),
            "{}",
            not_a_number.words
        );

        let fits = bind(
            "view:ready",
            &view,
            &given(&[
                ("project", "project:krusty-burger-app"),
                ("due", "today+3d"),
                ("count", "12"),
            ]),
            &known,
            &[],
            TODAY,
        )
        .expect("the right call binds");
        assert_eq!(
            fits.get("due").flatten().map(|v| v.text.as_str()),
            Some("2026-10-17")
        );
        assert_eq!(
            fits.get("count").flatten().map(|v| v.text.as_str()),
            Some("12")
        );
    }
}
