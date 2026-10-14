//! **The expression a caller writes to say which things it wants.**
//!
//! One short line over keys, the way a filter on a task board reads:
//! `status != done && owner = bot:gamma`, `due <= today+7d`,
//! `tags contains kitchen`. It reads into the [`Filter`] tree and means exactly
//! what the tree means; this module adds a spelling and no meaning.
//!
//! **Words.** `=`, `!=`, `<`, `<=`, `>`, `>=`, `in (a, b)`, `contains`, a bare
//! key (it is carried), `&&`, `||`, `!` and parentheses. `!` binds tightest,
//! then `&&`, then `||`. There are no wildcards.
//!
//! **An ordering reads dates and numbers.** The operand says which: a date
//! spelled `2026-10-09` or relative to the asker's day, or a number. Whether
//! the key was declared to hold that is the tree's question
//! ([`Filter::validate`]), not this reader's.
//!
//! **A relative date is `today`, `today+3d` or `today-2w`**, whole days and
//! weeks, resolved here against the day it is handed: the text is kept as the
//! caller wrote it and only a read gives it a date. A quoted word is never
//! relative.
//!
//! **A mistake says where it is.** A refusal names the token, its position and
//! the words accepted there.

use jiff::civil::Date;

use super::{Filter, Op};
use crate::memory::MemoryError;
use crate::memory::types::ValueType;

/// **How many characters an expression may have.** A real filter is a line;
/// this is far above one. Past it the text is refused before it is read, naming
/// the first character over.
pub const MAX_LENGTH: usize = 4096;

/// **How deep an expression may nest**, counting each `(` and each `!` that
/// stands before a term. A real filter nests a few levels. The tree, its clone,
/// its drop and its evaluation all recurse over this depth, so it is bounded
/// where the tree is made.
pub const MAX_DEPTH: usize = 32;

/// Read an expression into a filter tree. `today` is the asker's day, which a
/// relative date is resolved against.
pub fn parse(text: &str, today: Date) -> Result<Filter, MemoryError> {
    read(text, today).map(|(tree, _)| tree)
}

/// **The values an expression compares keys against**, in the order written, or
/// the refusal the expression earns. A relative date is a value like any other
/// word: it names no thing.
pub fn value_words(text: &str) -> Result<Vec<ValueWord>, MemoryError> {
    // Whether it reads does not depend on the day, so a fixed one is read.
    read(text, Date::constant(2000, 1, 1)).map(|(_, values)| values)
}

/// **A day as a caller spells it, resolved**: a date written out, or `today` with
/// days or weeks added, against the day it is handed. The one place a day a call
/// sends is read, so it means what the same word means in an expression.
pub fn date_word(word: &str, today: Date) -> Result<String, MemoryError> {
    let reader = Reader {
        tokens: Vec::new(),
        at: 0,
        depth: 0,
        today,
        values: Vec::new(),
    };
    let token = Token {
        kind: Kind::Word(word.to_string()),
        position: 1,
        end: word.chars().count() + 1,
    };
    match reader.read(word, &token)? {
        Operand::Day(day) => Ok(day),
        Operand::Text(text) if text.parse::<Date>().is_ok() => Ok(text),
        Operand::Text(_) => Err(refused(format!(
            "'{word}' is not a date. Accepted: a date such as 2026-10-09, or today with days or \
             weeks added: today, today+3d, today-2w"
        ))),
    }
}

/// **Whether a word is a number**: one that parses and is finite. The one place
/// that is decided, for an ordering in an expression and for an input a call sends.
pub fn is_a_number(text: &str) -> bool {
    text.parse::<f64>().is_ok_and(f64::is_finite)
}

/// **The inputs a saved expression names where a key goes.** An input stands for a
/// value, so a name there is never filled; a write that finds one refuses it.
pub fn placeholders_in_key_position(text: &str) -> Vec<String> {
    fn walk(node: &Node, found: &mut Vec<String>) {
        match node {
            Node::Term(words) => {
                if let Some(Token {
                    kind: Kind::Word(word),
                    ..
                }) = words.first()
                    && let Some(name) = word
                        .strip_prefix('$')
                        .filter(|name| crate::memory::view_inputs::is_a_name(name))
                {
                    found.push(name.to_string());
                }
            }
            Node::Not(inner) | Node::Group(inner) => walk(inner, found),
            Node::All(parts) | Node::Any(parts) => {
                for part in parts {
                    walk(part, found);
                }
            }
        }
    }
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    let mut shape = Shape {
        tokens,
        at: 0,
        depth: 0,
    };
    let mut found = Vec::new();
    if let Some(tree) = shape.any()
        && shape.at == shape.tokens.len()
    {
        walk(&tree, &mut found);
    }
    found
}

/// **The inputs a saved expression names**, as `$name` words, in the order
/// written.
pub fn placeholders(text: &str) -> Vec<String> {
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    tokens
        .iter()
        .filter_map(|token| match &token.kind {
            Kind::Word(word) => word
                .strip_prefix('$')
                .filter(|name| crate::memory::view_inputs::is_a_name(name))
                .map(str::to_string),
            _ => None,
        })
        .collect()
}

/// **A saved expression with each `$name` standing for a value**, the same length
/// as the name so every other place in the line stays where it was.
fn standing(text: &str) -> String {
    let Ok(tokens) = lex(text) else {
        return text.to_string();
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut cut = 0;
    for token in &tokens {
        let Kind::Word(word) = &token.kind else {
            continue;
        };
        if word
            .strip_prefix('$')
            .is_some_and(crate::memory::view_inputs::is_a_name)
        {
            out.extend(&chars[cut..token.position - 1]);
            out.extend(std::iter::repeat_n('0', word.chars().count()));
            cut = token.end - 1;
        }
    }
    out.extend(&chars[cut..]);
    out
}

/// **Whether a saved expression reads, with each `$name` standing for a value.**
/// A view's `where` is written before any call has bound its inputs, so it is
/// checked as it will read once they are: every `$name` is a value, and the rest
/// is the line the reader would take. Which value a name is bound to, and
/// whether it is the right type for the key, is a call's question.
pub fn reads_saved(text: &str) -> Result<(), MemoryError> {
    read(&standing(text), Date::constant(2000, 1, 1)).map(|_| ())
}

/// [`value_words`] of a saved expression, with each `$name` standing for a value.
/// A name is not a handle, so the places returned for it bind nothing.
pub fn saved_value_words(text: &str) -> Result<Vec<ValueWord>, MemoryError> {
    value_words(&standing(text))
}

/// **A saved expression with its inputs filled in.** A `$name` the view declared
/// becomes the value the call bound it to, and every term that uses an input the
/// call left out is dropped, with the `&&`, `||`, `!` and parentheses around it
/// closed up. Nothing is left to ask when every term was dropped.
///
/// **A `$word` the call bound nothing for is a word**, so a line with no inputs
/// reads as it always did. A line this cannot read the shape of is handed back as
/// written, and the reader that parses it next says what is wrong with it.
pub fn with_inputs(
    text: &str,
    bound: &crate::memory::view_inputs::Bound,
) -> Result<Option<String>, MemoryError> {
    if bound.is_empty() || text.chars().count() > MAX_LENGTH {
        return Ok(Some(text.to_string()));
    }
    let Ok(tokens) = lex(text) else {
        return Ok(Some(text.to_string()));
    };
    let mut shape = Shape {
        tokens,
        at: 0,
        depth: 0,
    };
    let Some(tree) = shape.any() else {
        return Ok(Some(text.to_string()));
    };
    if shape.at != shape.tokens.len() {
        return Ok(Some(text.to_string()));
    }
    filled(&tree, bound)
}

/// An expression's shape: its terms and what combines them, with the words of each
/// term kept as they were written.
enum Node {
    Term(Vec<Token>),
    Not(Box<Node>),
    Group(Box<Node>),
    All(Vec<Node>),
    Any(Vec<Node>),
}

struct Shape {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Shape {
    fn peek(&self) -> Option<&Kind> {
        self.tokens.get(self.at).map(|t| &t.kind)
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += token.is_some() as usize;
        token
    }

    fn any(&mut self) -> Option<Node> {
        let mut branches = vec![self.all()?];
        while self.peek() == Some(&Kind::Or) {
            self.at += 1;
            branches.push(self.all()?);
        }
        Some(match branches.len() {
            1 => branches.remove(0),
            _ => Node::Any(branches),
        })
    }

    fn all(&mut self) -> Option<Node> {
        let mut parts = vec![self.unary()?];
        while self.peek() == Some(&Kind::And) {
            self.at += 1;
            parts.push(self.unary()?);
        }
        Some(match parts.len() {
            1 => parts.remove(0),
            _ => Node::All(parts),
        })
    }

    fn unary(&mut self) -> Option<Node> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        let token = self.take()?;
        let node = match token.kind {
            Kind::Not => Node::Not(Box::new(self.unary()?)),
            Kind::Open => {
                let inner = self.any()?;
                (self.take()?.kind == Kind::Close).then_some(())?;
                Node::Group(Box::new(inner))
            }
            Kind::Word(_) => self.term(token)?,
            _ => return None,
        };
        self.depth -= 1;
        Some(node)
    }

    /// A key and what is asked of it, as the tokens it is written in.
    fn term(&mut self, key: Token) -> Option<Node> {
        let mut words = vec![key];
        let value = |shape: &mut Shape, words: &mut Vec<Token>| {
            let token = shape.take()?;
            matches!(token.kind, Kind::Word(_) | Kind::Quoted(_)).then(|| words.push(token))
        };
        match self.peek() {
            None | Some(Kind::And | Kind::Or | Kind::Close) => {}
            Some(
                Kind::Equals
                | Kind::NotEquals
                | Kind::Less
                | Kind::LessOrEqual
                | Kind::Greater
                | Kind::GreaterOrEqual,
            ) => {
                words.push(self.take()?);
                value(self, &mut words)?;
            }
            Some(Kind::Word(word)) if word == "contains" => {
                words.push(self.take()?);
                value(self, &mut words)?;
            }
            Some(Kind::Word(word)) if word == "in" => {
                words.push(self.take()?);
                (self.peek() == Some(&Kind::Open)).then_some(())?;
                words.push(self.take()?);
                value(self, &mut words)?;
                while self.peek() == Some(&Kind::Comma) {
                    words.push(self.take()?);
                    value(self, &mut words)?;
                }
                (self.peek() == Some(&Kind::Close)).then_some(())?;
                words.push(self.take()?);
            }
            Some(_) => return None,
        }
        Some(Node::Term(words))
    }
}

/// The line a node stands for with the call's inputs in it, or nothing when every
/// term in it was dropped.
fn filled(
    node: &Node,
    bound: &crate::memory::view_inputs::Bound,
) -> Result<Option<String>, MemoryError> {
    match node {
        Node::Term(words) => filled_term(words, bound),
        Node::Not(inner) => Ok(filled(inner, bound)?.map(|line| format!("!{line}"))),
        Node::Group(inner) => Ok(filled(inner, bound)?.map(|line| format!("({line})"))),
        Node::All(parts) | Node::Any(parts) => {
            let mut kept = Vec::new();
            for part in parts {
                kept.extend(filled(part, bound)?);
            }
            let glue = if matches!(node, Node::All(_)) {
                " && "
            } else {
                " || "
            };
            Ok((!kept.is_empty()).then(|| kept.join(glue)))
        }
    }
}

/// One term, with each `$name` in it replaced; nothing when it uses an input that
/// was left out.
fn filled_term(
    words: &[Token],
    bound: &crate::memory::view_inputs::Bound,
) -> Result<Option<String>, MemoryError> {
    let mut line = String::new();
    let mut previous: Option<&Kind> = None;
    for (at, word) in words.iter().enumerate() {
        let name = match &word.kind {
            Kind::Word(w) if at > 0 => w
                .strip_prefix('$')
                .filter(|name| crate::memory::view_inputs::is_a_name(name)),
            _ => None,
        };
        let spelled = match name.and_then(|name| bound.get(name)) {
            // Declared and left out: the term goes.
            Some(None) => return Ok(None),
            Some(Some(value)) => as_a_value(&value.text)?,
            // Not declared: a word like any other.
            None => word.spelled(),
        };
        let tight = matches!(word.kind, Kind::Comma | Kind::Close)
            || matches!(previous, Some(Kind::Open) | None);
        if !tight {
            line.push(' ');
        }
        line.push_str(&spelled);
        previous = Some(&word.kind);
    }
    Ok(Some(line))
}

/// A bound value as one value of an expression: a word as it is, or in quotes when
/// it holds a character that would end the word.
fn as_a_value(text: &str) -> Result<String, MemoryError> {
    if text.contains('"') {
        return Err(refused(
            "an input cannot hold a double quote, which is how an expression ends a quoted value",
        ));
    }
    if text.is_empty() || text.chars().any(ends_a_word) {
        return Ok(format!("\"{text}\""));
    }
    Ok(text.to_string())
}

fn read(text: &str, today: Date) -> Result<(Filter, Vec<ValueWord>), MemoryError> {
    // **Checked before anything is read**, so text a caller makes huge costs
    // one count and no tokens.
    if text.chars().count() > MAX_LENGTH {
        return Err(refused(format!(
            "the expression does not parse: it is longer than {MAX_LENGTH} characters, and the \
             first character over the limit is at position {}. Write a shorter line",
            MAX_LENGTH + 1
        )));
    }
    let tokens = lex(text)?;
    if tokens.is_empty() {
        return Err(refused(
            "the expression is empty. Write what the things must hold, for example \
             `status != done`",
        ));
    }
    let mut reader = Reader {
        tokens,
        at: 0,
        depth: 0,
        today,
        values: Vec::new(),
    };
    let tree = reader.or()?;
    match reader.peek() {
        None => Ok((tree, reader.values)),
        Some(token) => Err(reader.unexpected(&token, "&&, ||, ')' or the end")),
    }
}

fn refused(words: impl Into<String>) -> MemoryError {
    MemoryError::InvalidQuery(words.into())
}

/// One token and the character it starts at, counted from 1.
#[derive(Debug, Clone, PartialEq)]
struct Token {
    kind: Kind,
    position: usize,
    /// One past the last character, counted from 1 like `position`.
    end: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Open,
    Close,
    Comma,
    Not,
    And,
    Or,
    Equals,
    NotEquals,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    /// A word, as written.
    Word(String),
    /// A word in double quotes, without them.
    Quoted(String),
}

impl Token {
    /// The token as the caller wrote it, for a refusal to quote.
    fn spelled(&self) -> String {
        match &self.kind {
            Kind::Open => "(".into(),
            Kind::Close => ")".into(),
            Kind::Comma => ",".into(),
            Kind::Not => "!".into(),
            Kind::And => "&&".into(),
            Kind::Or => "||".into(),
            Kind::Equals => "=".into(),
            Kind::NotEquals => "!=".into(),
            Kind::Less => "<".into(),
            Kind::LessOrEqual => "<=".into(),
            Kind::Greater => ">".into(),
            Kind::GreaterOrEqual => ">=".into(),
            Kind::Word(word) => word.clone(),
            Kind::Quoted(word) => format!("\"{word}\""),
        }
    }
}

/// Whether a character ends a word.
fn ends_a_word(c: char) -> bool {
    c.is_whitespace() || "()!=<>&|\",".contains(c)
}

fn lex(text: &str) -> Result<Vec<Token>, MemoryError> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let position = i + 1;
        let next = chars.get(i + 1).copied();
        let (kind, width) = match (c, next) {
            (c, _) if c.is_whitespace() => {
                i += 1;
                continue;
            }
            ('(', _) => (Kind::Open, 1),
            (')', _) => (Kind::Close, 1),
            (',', _) => (Kind::Comma, 1),
            ('&', Some('&')) => (Kind::And, 2),
            ('|', Some('|')) => (Kind::Or, 2),
            ('!', Some('=')) => (Kind::NotEquals, 2),
            ('!', _) => (Kind::Not, 1),
            ('<', Some('=')) => (Kind::LessOrEqual, 2),
            ('<', _) => (Kind::Less, 1),
            ('>', Some('=')) => (Kind::GreaterOrEqual, 2),
            ('>', _) => (Kind::Greater, 1),
            ('=', _) => (Kind::Equals, 1),
            ('&', _) => {
                return Err(refused(format!(
                    "the expression does not parse: '&' at position {position} is not a word. \
                     Accepted: '&&' for and"
                )));
            }
            ('|', _) => {
                return Err(refused(format!(
                    "the expression does not parse: '|' at position {position} is not a word. \
                     Accepted: '||' for or"
                )));
            }
            ('"', _) => {
                let Some(close) = chars[i + 1..].iter().position(|c| *c == '"') else {
                    return Err(refused(format!(
                        "the expression does not parse: the quote at position {position} is \
                         never closed. A quoted value ends with a second '\"'"
                    )));
                };
                let word: String = chars[i + 1..i + 1 + close].iter().collect();
                (Kind::Quoted(word), close + 2)
            }
            _ => {
                let width = chars[i..]
                    .iter()
                    .position(|c| ends_a_word(*c))
                    .unwrap_or(chars.len() - i);
                (Kind::Word(chars[i..i + width].iter().collect()), width)
            }
        };
        tokens.push(Token {
            kind,
            position,
            end: position + width,
        });
        i += width;
    }
    Ok(tokens)
}

struct Reader {
    tokens: Vec<Token>,
    at: usize,
    /// How many `(` and `!` the term being read stands inside. See [`MAX_DEPTH`].
    depth: usize,
    today: Date,
    /// The values the expression compares against with `=`, `!=`, `in` and
    /// `contains`, where they sit.
    values: Vec<ValueWord>,
}

/// **A value an expression compares a key against, and where it sits.** What a
/// saved expression binds a handle in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueWord {
    /// The first character of the value, counted from 0. A quoted value starts
    /// after its quote.
    pub start: usize,
    /// One past the last character.
    pub end: usize,
    /// The value as written.
    pub text: String,
}

/// What a word was read as.
enum Operand {
    /// A value to compare as written.
    Text(String),
    /// A day, resolved from a relative one.
    Day(String),
}

impl Reader {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).cloned()
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.peek();
        if token.is_some() {
            self.at += 1;
        }
        token
    }

    /// The refusal for a token that is not accepted here.
    fn unexpected(&self, token: &Token, accepted: &str) -> MemoryError {
        refused(format!(
            "the expression does not parse: '{}' at position {} is not accepted here. \
             Accepted: {accepted}",
            token.spelled(),
            token.position,
        ))
    }

    /// The refusal for an expression that ended where it could not.
    fn ended(&self, accepted: &str) -> MemoryError {
        refused(format!(
            "the expression does not parse: it ends before the term is finished. \
             Accepted at the end: {accepted}"
        ))
    }

    /// **Step one level in**, refusing at the token that passes [`MAX_DEPTH`].
    /// Every level is a recursion here and in everything that later walks the
    /// tree, so the bound is taken where the tree is made.
    fn enter(&mut self, token: &Token) -> Result<(), MemoryError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(refused(format!(
                "the expression does not parse: '{}' at position {} nests deeper than \
                 {MAX_DEPTH} levels. Write a flatter line",
                token.spelled(),
                token.position,
            )));
        }
        Ok(())
    }

    fn or(&mut self) -> Result<Filter, MemoryError> {
        let mut branches = vec![self.and()?];
        while self.peek().is_some_and(|t| t.kind == Kind::Or) {
            self.at += 1;
            branches.push(self.and()?);
        }
        Ok(one_or(branches, Filter::Any))
    }

    fn and(&mut self) -> Result<Filter, MemoryError> {
        let mut parts = vec![self.unary()?];
        while self.peek().is_some_and(|t| t.kind == Kind::And) {
            self.at += 1;
            parts.push(self.unary()?);
        }
        Ok(one_or(parts, Filter::All))
    }

    fn unary(&mut self) -> Result<Filter, MemoryError> {
        let start = "a key, '!' or '('";
        let Some(token) = self.take() else {
            return Err(self.ended(start));
        };
        match &token.kind {
            Kind::Not => {
                self.enter(&token)?;
                let inner = self.unary()?;
                self.depth -= 1;
                Ok(Filter::negated(inner))
            }
            Kind::Open => {
                self.enter(&token)?;
                let inner = self.or()?;
                self.depth -= 1;
                match self.take() {
                    Some(Token {
                        kind: Kind::Close, ..
                    }) => Ok(inner),
                    Some(other) => Err(self.unexpected(&other, "&&, || or ')'")),
                    None => Err(self.ended("')' to close the '(' that opened the group")),
                }
            }
            Kind::Word(key) => self.term(key),
            _ => Err(self.unexpected(&token, start)),
        }
    }

    /// A key and what is asked of it.
    fn term(&mut self, key: &str) -> Result<Filter, MemoryError> {
        let after_a_key = "=, !=, <, <=, >, >=, in, contains, &&, ||, ')' or the end";
        let Some(token) = self.peek() else {
            return Ok(Filter::exists(key));
        };
        let op = match &token.kind {
            Kind::And | Kind::Or | Kind::Close => return Ok(Filter::exists(key)),
            Kind::Equals => {
                self.at += 1;
                return Ok(Filter::equals(key, &self.value()?));
            }
            Kind::NotEquals => {
                self.at += 1;
                return Ok(Filter::not_equals(key, &self.value()?));
            }
            Kind::Word(word) if word == "in" => {
                self.at += 1;
                let items = self.list()?;
                let items: Vec<&str> = items.iter().map(String::as_str).collect();
                return Ok(Filter::is_in(key, &items));
            }
            Kind::Word(word) if word == "contains" => {
                self.at += 1;
                return Ok(Filter::contains(key, &self.value()?));
            }
            Kind::Less => Op::Less,
            Kind::LessOrEqual => Op::LessOrEqual,
            Kind::Greater => Op::Greater,
            Kind::GreaterOrEqual => Op::GreaterOrEqual,
            _ => return Err(self.unexpected(&token, after_a_key)),
        };
        self.at += 1;
        let (holds, wanted) = self.operand(&token)?;
        Ok(Filter::ordering(key, op, holds, &wanted))
    }

    /// A value for an equality, an any-of or a `contains`.
    fn value(&mut self) -> Result<String, MemoryError> {
        let accepted = "a value: a word, or words in double quotes";
        let Some(token) = self.take() else {
            return Err(self.ended(accepted));
        };
        match &token.kind {
            Kind::Word(word) => {
                self.values.push(ValueWord {
                    start: token.position - 1,
                    end: token.end - 1,
                    text: word.clone(),
                });
                Ok(match self.read(word, &token)? {
                    Operand::Text(text) | Operand::Day(text) => text,
                })
            }
            Kind::Quoted(word) => {
                self.values.push(ValueWord {
                    start: token.position,
                    end: token.end - 2,
                    text: word.clone(),
                });
                Ok(word.trim().to_string())
            }
            // **A second `=` is the usual slip**: the word for equality in other
            // languages is `==`, and here it is one `=`.
            Kind::Equals => Err(refused(format!(
                "the expression does not parse: '=' at position {} follows an '=', and \
                 equality is written with one: status = done, not status == done",
                token.position,
            ))),
            _ => Err(self.unexpected(&token, accepted)),
        }
    }

    /// The `(a, b, c)` after `in`.
    fn list(&mut self) -> Result<Vec<String>, MemoryError> {
        match self.take() {
            Some(Token {
                kind: Kind::Open, ..
            }) => {}
            Some(other) => return Err(self.unexpected(&other, "'(' to open the list")),
            None => return Err(self.ended("'(' to open the list")),
        }
        let mut items = vec![self.value()?];
        loop {
            match self.take() {
                Some(Token {
                    kind: Kind::Comma, ..
                }) => items.push(self.value()?),
                Some(Token {
                    kind: Kind::Close, ..
                }) => return Ok(items),
                Some(other) => return Err(self.unexpected(&other, "',' or ')'")),
                None => return Err(self.ended("',' or ')' to close the list")),
            }
        }
    }

    /// The operand of an ordering, and whether it is a date or a number.
    fn operand(&mut self, op: &Token) -> Result<(ValueType, String), MemoryError> {
        let accepted = "a date, a number, or today with days or weeks added";
        let Some(token) = self.take() else {
            return Err(self.ended(accepted));
        };
        // **A value in quotes is literal text**, so an ordering cannot read it as a
        // date or a number, `today` included. Said apart from the generic refusal
        // because the quotes are what the caller meant as a safety.
        if let Kind::Quoted(word) = &token.kind {
            return Err(refused(format!(
                "the expression does not parse: a value in quotes is literal text, so \
                 \"{word}\" at position {} is not a date or a number. Write it without the \
                 quotes. Accepted: {accepted}",
                token.position,
            )));
        }
        let Kind::Word(word) = &token.kind else {
            return Err(self.unexpected(&token, accepted));
        };
        match self.read(word, &token)? {
            Operand::Day(day) => Ok((ValueType::Date, day)),
            Operand::Text(text) if text.parse::<Date>().is_ok() => Ok((ValueType::Date, text)),
            Operand::Text(text) if is_a_number(&text) => Ok((ValueType::Number, text)),
            Operand::Text(_) => Err(refused(format!(
                "the expression does not parse: '{}' compares dates and numbers, and '{word}' \
                 at position {} is neither. Accepted: {accepted}",
                op.spelled(),
                token.position,
            ))),
        }
    }

    /// A word, with a relative date turned into the day it names.
    fn read(&self, word: &str, token: &Token) -> Result<Operand, MemoryError> {
        let Some(rest) = word.strip_prefix("today") else {
            return Ok(Operand::Text(word.to_string()));
        };
        if rest.is_empty() {
            return Ok(Operand::Day(self.today.to_string()));
        }
        let sign: i64 = match rest.chars().next() {
            Some('+') => 1,
            Some('-') => -1,
            // A word that only starts like it, such as `todays`.
            _ => return Ok(Operand::Text(word.to_string())),
        };
        // The unit is the LAST CHARACTER, so the split is taken on a character
        // boundary: a byte split lands inside a wide character and panics.
        let digits = &rest[1..];
        let unit_at = digits.char_indices().next_back().map_or(0, |(at, _)| at);
        let (count, unit) = digits.split_at(unit_at);
        let days = match (count.parse::<u32>(), unit) {
            (Ok(count), "d") => Some(i64::from(count)),
            (Ok(count), "w") => Some(i64::from(count) * 7),
            _ => None,
        };
        // **Fallible at both steps**: the span holds a bounded number of days, and
        // a date a span reaches may be past the calendar. Either is no relative
        // date, never a panic.
        let moved = days.and_then(|days| {
            let span = jiff::Span::new().try_days(sign * days).ok()?;
            self.today.checked_add(span).ok()
        });
        match moved {
            Some(day) => Ok(Operand::Day(day.to_string())),
            None => Err(refused(format!(
                "the expression does not parse: '{word}' at position {} is not a relative \
                 date. Accepted: today, today+3d, today-2w (whole days 'd' and weeks 'w')",
                token.position,
            ))),
        }
    }
}

/// One branch stands for itself; more are combined.
fn one_or(mut parts: Vec<Filter>, combine: fn(Vec<Filter>) -> Filter) -> Filter {
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        combine(parts)
    }
}

#[cfg(test)]
mod tests;
