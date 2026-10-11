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

fn read(text: &str, today: Date) -> Result<(Filter, Vec<ValueWord>), MemoryError> {
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
            Kind::Not => Ok(Filter::negated(self.unary()?)),
            Kind::Open => {
                let inner = self.or()?;
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
        let Kind::Word(word) = &token.kind else {
            return Err(self.unexpected(&token, accepted));
        };
        match self.read(word, &token)? {
            Operand::Day(day) => Ok((ValueType::Date, day)),
            Operand::Text(text) if text.parse::<Date>().is_ok() => Ok((ValueType::Date, text)),
            Operand::Text(text) if text.parse::<f64>().is_ok_and(f64::is_finite) => {
                Ok((ValueType::Number, text))
            }
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
        let (count, unit) = rest[1..].split_at(rest[1..].len().saturating_sub(1));
        let days = match (count.parse::<u32>(), unit) {
            (Ok(count), "d") => Some(i64::from(count)),
            (Ok(count), "w") => Some(i64::from(count) * 7),
            _ => None,
        };
        let moved = days.and_then(|days| {
            self.today
                .checked_add(jiff::Span::new().days(sign * days))
                .ok()
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
