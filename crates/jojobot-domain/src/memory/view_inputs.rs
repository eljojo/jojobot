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
//! view's `where`, `parent` and `answers_type` is
//! [`super::filter::expression::with_inputs`] and the caller's.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::guard::EntityMatch;
use super::{Entity, EntityId, FormerHandle};

/// **The view's key that declares its inputs.**
pub const KEY: &str = "inputs";

/// **The names an input cannot have**, because the page of a view reads them for
/// itself: the key of a page's own query. A URL gives such a key to the page, so an
/// input of that name could never be filled from it.
pub const RESERVED_NAMES: &[&str] = &["history"];

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
    /// Bound values, by input name.
    pub fn new(values: BTreeMap<String, Option<Value>>) -> Bound {
        Bound { values }
    }

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

/// **What a write of a view's keys breaks**, as the key and what it wanted, or
/// nothing. A view's `inputs` has to read, every `$name` its `where` or its
/// `parent` uses has to be one it declares, and a `parent` that is an input has to
/// be a handle. **A view that declares no input is not held to this**: a `$word`
/// in its question is a word.
pub fn breaks_a_view(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    const WHERE: &str = "where";
    const PARENT: &str = "parent";
    const ANSWERS_TYPE: &str = "answers_type";
    let same = |key: &str| before.get(key) == after.get(key);
    if same(KEY) && same(WHERE) && same(PARENT) && same(ANSWERS_TYPE) {
        return None;
    }
    let declared = match parse(after.get(KEY).map_or("", String::as_str)) {
        Ok(declared) => declared,
        Err(refused) => return Some((KEY.to_string(), refused.words)),
    };
    if let Some(input) = declared
        .iter()
        .find(|d| RESERVED_NAMES.contains(&d.name.as_str()))
    {
        return Some((
            KEY.to_string(),
            format!(
                "'{}' cannot be the name of an input: the page of a view reads it for itself, \
                 so a link could never fill it. Reserved: {}",
                input.name,
                RESERVED_NAMES.join(", ")
            ),
        ));
    }
    let says = match declared.is_empty() {
        true => "it declares no inputs".to_string(),
        false => format!("It declares: {}", spelled(&declared)),
    };
    let mut uses: Vec<(&str, String)> = Vec::new();
    if let Some(line) = after.get(WHERE) {
        if let Some(name) = super::filter::expression::placeholders_in_key_position(line).first() {
            return Some((
                WHERE.to_string(),
                format!(
                    "the view puts ${name} where a key goes. An input stands for a value: \
                     write `key = ${name}`"
                ),
            ));
        }
        uses.extend(
            super::filter::expression::placeholders(line)
                .into_iter()
                .map(|name| (WHERE, name)),
        );
    }
    if let Some(name) = after
        .get(PARENT)
        .and_then(|parent| parent.trim().strip_prefix('$'))
        .filter(|name| is_a_name(name))
    {
        uses.push((PARENT, name.to_string()));
    }
    if let Some(name) = after
        .get(ANSWERS_TYPE)
        .and_then(|asked| asked.trim().strip_prefix('$'))
        .filter(|name| is_a_name(name))
    {
        uses.push((ANSWERS_TYPE, name.to_string()));
    }
    for (key, name) in uses {
        let Some(decl) = declared.iter().find(|d| d.name == name) else {
            return Some((
                key.to_string(),
                format!("the view uses ${name}, which its `inputs` do not declare. {says}"),
            ));
        };
        if key == PARENT && !matches!(decl.ty, InputType::Kind(_)) {
            return Some((
                key.to_string(),
                format!(
                    "the view's `parent` is the input ${name}, and a parent is a thing: declare \
                     it as `{name}: kind <kind>`"
                ),
            ));
        }
        if key == ANSWERS_TYPE && decl.ty != InputType::Text {
            return Some((
                key.to_string(),
                format!(
                    "the view's `answers_type` is the input ${name}, and a type is named in \
                     words: declare it as `{name}: text`"
                ),
            ));
        }
    }
    None
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
pub(crate) fn is_a_name(word: &str) -> bool {
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
            InputType::Number if super::filter::expression::is_a_number(sent) => Value {
                text: sent.to_string(),
                handle: None,
            },
            InputType::Number => return Err(wrong(format!("'{sent}' is not a number"))),
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
        let known = vec![entity("project:atlas", "k7h2mn")];
        let former = vec![FormerHandle {
            former: EntityId("project:the-move".into()),
            badge: "k7h2mn".into(),
            changed_at: TODAY,
        }];
        let view = decls("project: kind project?");
        let bound = bind(
            "view:ready",
            &view,
            &given(&[("project", "project:the-move")]),
            &known,
            &former,
            TODAY,
        )
        .expect("a handle the thing wore binds");
        let value = bound.get("project").flatten().expect("a value");
        assert_eq!(value.text, "project:atlas");
        assert_eq!(
            value.handle.as_ref().map(EntityId::as_str),
            Some("project:atlas")
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
            entity("project:atlas", "k7h2mn"),
            entity("person:bart", "b0bb0b"),
        ];
        let view = decls("project: kind project, due: date?, count: number?");
        let declared = "project: kind project, due: date?, count: number?";
        let refuse = |pairs: &[(&str, &str)]| {
            bind("view:ready", &view, &given(pairs), &known, &[], TODAY).expect_err("a refusal")
        };

        let missing = refuse(&[]);
        assert!(missing.words.contains("project"), "{}", missing.words);
        assert!(missing.words.contains(declared), "{}", missing.words);

        let unknown = refuse(&[("projekt", "project:atlas")]);
        assert!(unknown.words.contains("projekt"), "{}", unknown.words);
        assert!(unknown.words.contains(declared), "{}", unknown.words);

        let wrong_kind = refuse(&[("project", "person:bart")]);
        assert!(
            wrong_kind.words.contains("person:bart"),
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

        let nothing = refuse(&[("project", "project:kitchen-floor")]);
        assert!(
            nothing.words.contains("project:kitchen-floor"),
            "{}",
            nothing.words
        );

        let not_a_date = refuse(&[("project", "project:atlas"), ("due", "soon")]);
        assert!(not_a_date.words.contains("due"), "{}", not_a_date.words);
        let infinite = refuse(&[("project", "project:atlas"), ("count", "inf")]);
        assert!(infinite.words.contains("count"), "{}", infinite.words);
        let not_a_number = refuse(&[("project", "project:atlas"), ("count", "many")]);
        assert!(
            not_a_number.words.contains("count"),
            "{}",
            not_a_number.words
        );

        let fits = bind(
            "view:ready",
            &view,
            &given(&[
                ("project", "project:atlas"),
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

    fn view(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        given(pairs)
    }

    /// **A view may use only the inputs it declares, and its `inputs` has to read.**
    /// **Paired**: the same view with the name spelled as declared is accepted,
    /// and a view that declares nothing keeps its `$word` as a word.
    #[test]
    fn a_view_may_use_only_the_inputs_it_declares() {
        let none = view(&[]);
        let declares = [
            ("selects", "work"),
            ("inputs", "project: kind project?, due: date?"),
        ];
        let with = |extra: &[(&'static str, &'static str)]| {
            let mut pairs = declares.to_vec();
            pairs.extend_from_slice(extra);
            view(&pairs)
        };

        assert_eq!(
            breaks_a_view(
                &none,
                &with(&[("where", "project = $project && due <= $due")])
            ),
            None
        );
        assert_eq!(breaks_a_view(&none, &with(&[("parent", "$project")])), None);

        let typo = breaks_a_view(&none, &with(&[("where", "project = $projekt")]))
            .expect("an undeclared input is refused");
        assert_eq!(typo.0, "where");
        assert!(
            typo.1.contains("projekt") && typo.1.contains("project: kind project?"),
            "{}",
            typo.1
        );

        let parent_typo = breaks_a_view(&none, &with(&[("parent", "$projekt")]))
            .expect("an undeclared input is refused");
        assert_eq!(parent_typo.0, "parent");

        let not_a_handle = breaks_a_view(&none, &with(&[("parent", "$due")]))
            .expect("a parent has to be a handle");
        assert_eq!(not_a_handle.0, "parent");

        let unreadable = breaks_a_view(
            &none,
            &view(&[("selects", "work"), ("inputs", "project kind")]),
        )
        .expect("a list that does not read is refused");
        assert_eq!(unreadable.0, "inputs");

        // A `$5` is a price and not a name, so it stays a word.
        assert_eq!(
            breaks_a_view(
                &none,
                &view(&[("selects", "work"), ("where", "price = $5")])
            ),
            None
        );
        // An edit that leaves the three keys as they were is not asked again.
        let stored = with(&[("where", "project = $projekt")]);
        assert_eq!(breaks_a_view(&stored, &stored), None);
    }

    /// **A name the view does not declare is refused whether or not it declares
    /// any.** A view with no inputs and a `$x` in its question would read `$x` as a
    /// word and answer a question nobody wrote, and clearing a view's last input
    /// must not turn the `$x` that used it into one. **Paired**: a `$5` is a price,
    /// and a declared name is accepted.
    #[test]
    fn a_name_the_view_does_not_declare_is_refused_even_when_it_declares_none() {
        let none = view(&[]);
        let bare = view(&[("selects", "work"), ("where", "price = $x")]);
        let refused = breaks_a_view(&none, &bare).expect("a name nothing declares");
        assert_eq!(refused.0, "where");
        assert!(refused.1.contains("$x"), "{}", refused.1);

        let parent = view(&[("selects", "work"), ("parent", "$x")]);
        assert_eq!(
            breaks_a_view(&none, &parent)
                .expect("a parent nothing declares")
                .0,
            "parent"
        );

        // The last input cleared while the question still uses it.
        let with = view(&[
            ("selects", "work"),
            ("inputs", "x: text"),
            ("where", "price = $x"),
        ]);
        let cleared = view(&[("selects", "work"), ("where", "price = $x")]);
        assert_eq!(
            breaks_a_view(&with, &cleared)
                .expect("a name left without a declaration")
                .0,
            "where"
        );
        assert_eq!(breaks_a_view(&none, &with), None);
    }

    /// **An input stands for a value, so it is refused where a key goes**, since
    /// it is never filled there. **Paired**: the same name as a value is accepted.
    #[test]
    fn an_input_may_not_stand_where_a_key_goes() {
        let none = view(&[]);
        let keyed = view(&[
            ("selects", "work"),
            ("inputs", "x: text"),
            ("where", "$x = 1"),
        ]);
        let refused = breaks_a_view(&none, &keyed).expect("a name in the key place");
        assert_eq!(refused.0, "where");
        assert!(refused.1.contains("key"), "{}", refused.1);
        let nested = view(&[
            ("selects", "work"),
            ("inputs", "x: text"),
            ("where", "status != done && (a = 1 || !($x contains y))"),
        ]);
        assert_eq!(
            breaks_a_view(&none, &nested).expect("nested too").0,
            "where"
        );
        let valued = view(&[
            ("selects", "work"),
            ("inputs", "x: text"),
            ("where", "name = $x"),
        ]);
        assert_eq!(breaks_a_view(&none, &valued), None);
    }

    /// **A view's `answers_type` may be an input, and the input has to be text**,
    /// since a type is named in words. A name the view does not declare is refused
    /// as it is in a `where`. **Paired**: a declared text input is accepted, and a
    /// type written into the view is a type.
    #[test]
    fn an_answers_type_input_is_declared_and_holds_text() {
        let none = view(&[]);
        let typo = view(&[
            ("selects", "thing"),
            ("inputs", "type: text"),
            ("answers_type", "$tipe"),
        ]);
        assert_eq!(
            breaks_a_view(&none, &typo).expect("an undeclared input").0,
            "answers_type"
        );
        let not_text = view(&[
            ("selects", "thing"),
            ("inputs", "type: number"),
            ("answers_type", "$type"),
        ]);
        let refused = breaks_a_view(&none, &not_text).expect("a type is named in words");
        assert_eq!(refused.0, "answers_type");
        assert!(refused.1.contains("text"), "{}", refused.1);
        let fine = view(&[
            ("selects", "thing"),
            ("inputs", "type: text"),
            ("answers_type", "$type"),
        ]);
        assert_eq!(breaks_a_view(&none, &fine), None);
        let written = view(&[("selects", "thing"), ("answers_type", "bike")]);
        assert_eq!(breaks_a_view(&none, &written), None);
        // An edit that changes only the type is asked again.
        let declared = view(&[("selects", "thing"), ("inputs", "type: text")]);
        let edited = view(&[
            ("selects", "thing"),
            ("inputs", "type: text"),
            ("answers_type", "$tipe"),
        ]);
        assert_eq!(
            breaks_a_view(&declared, &edited)
                .expect("a type edit alone")
                .0,
            "answers_type"
        );
    }

    /// **An input cannot be named like a parameter of the page of a view**, which
    /// the URL would give to the page and never to the input. **Paired**: any
    /// other name is accepted.
    #[test]
    fn an_input_may_not_be_named_like_a_page_parameter() {
        let none = view(&[]);
        for reserved in RESERVED_NAMES {
            let named = view(&[
                ("selects", "work"),
                ("inputs", &format!("{reserved}: text?")),
            ]);
            let refused = breaks_a_view(&none, &named).expect("a reserved name");
            assert_eq!(refused.0, "inputs");
            assert!(refused.1.contains(reserved), "{}", refused.1);
        }
        let fine = view(&[("selects", "work"), ("inputs", "past: text?")]);
        assert_eq!(breaks_a_view(&none, &fine), None);
    }
}
