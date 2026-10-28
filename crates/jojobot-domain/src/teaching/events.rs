//! **Teaching as events**: what a teaching event means, and what the state is
//! after any sequence of them. Pure: no store, no envelope, no writer.
//!
//! A mark says one run was taught one domain. [`TeachingEvent::Recorded`] adds a
//! mark and [`TeachingEvent::Cleared`] removes every active mark of a run; both
//! stay in the history. [`State::apply`] is the one interpreter: meaning is fixed
//! when an event is written, so a later change of meaning is a new kind of event
//! or a dated version this interpreter reads, never a second reading of an old
//! one. The `prepare` functions decide, from a state, which events a call
//! appends; the interpreter does not re-admit them.
//!
//! **Agreement.** Applying a prepared batch to a state gives the state that
//! replaying the same events from the same prefix gives, because both run
//! [`State::apply`].

use std::collections::BTreeSet;

use jiff::Timestamp;

use super::DOMAIN_WIDTH;

/// **The public name of a run**, valid before the run has a session card.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunId(pub String);

/// One fact about teaching, complete when written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeachingEvent {
    /// The run was told the domain at `taught_at`.
    Recorded {
        run: RunId,
        domain: String,
        taught_at: Timestamp,
    },
    /// Everything the run was told is forgotten, so its next contact with any
    /// domain is a first contact again.
    Cleared { run: RunId },
}

/// **What the events so far say**: the marks that are active, and every
/// `Recorded` event kept in order with its `taught_at`, cleared or not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    active: BTreeSet<(RunId, String)>,
    history: Vec<(RunId, String, Timestamp)>,
}

/// What a call decided: the events to append, possibly none, and its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared<T> {
    pub events: Vec<TeachingEvent>,
    pub answer: T,
}

/// Why a contact is refused before any event is made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// The domain is wider, in characters, than [`DOMAIN_WIDTH`].
    #[error("a teaching domain is at most {DOMAIN_WIDTH} characters; this one has {0}")]
    DomainTooWide(usize),
}

impl State {
    /// **The interpreter.** Structural only: it keeps what the event says and
    /// checks nothing against today's policy.
    pub fn apply(&mut self, event: &TeachingEvent) {
        match event {
            TeachingEvent::Recorded {
                run,
                domain,
                taught_at,
            } => {
                self.active.insert((run.clone(), domain.clone()));
                self.history.push((run.clone(), domain.clone(), *taught_at));
            }
            TeachingEvent::Cleared { run } => self.active.retain(|(held, _)| held != run),
        }
    }

    /// The state after `events`, from nothing.
    pub fn replay<'a>(events: impl IntoIterator<Item = &'a TeachingEvent>) -> Self {
        let mut state = Self::default();
        for event in events {
            state.apply(event);
        }
        state
    }

    /// Whether the run holds a mark for the domain.
    pub fn has_contact(&self, run: &RunId, domain: &str) -> bool {
        self.active.contains(&(run.clone(), domain.to_string()))
    }

    /// Every active `(run, domain)` mark, in order.
    pub fn taught(&self) -> impl Iterator<Item = &(RunId, String)> {
        self.active.iter()
    }

    /// Every `Recorded` event seen, in the order it was applied.
    pub fn history(&self) -> &[(RunId, String, Timestamp)] {
        &self.history
    }

    /// **A contact**: `true` with one `Recorded` event when the run was not yet
    /// taught the domain, `false` with none when it was.
    pub fn first_contact(
        &self,
        run: &RunId,
        domain: &str,
        at: Timestamp,
    ) -> Result<Prepared<bool>, Refused> {
        let width = domain.chars().count();
        if width > DOMAIN_WIDTH {
            return Err(Refused::DomainTooWide(width));
        }
        if self.has_contact(run, domain) {
            return Ok(Prepared {
                events: Vec::new(),
                answer: false,
            });
        }
        Ok(Prepared {
            events: vec![TeachingEvent::Recorded {
                run: run.clone(),
                domain: domain.to_string(),
                taught_at: at,
            }],
            answer: true,
        })
    }

    /// **A forget**: one `Cleared` event when the run holds a mark, none when it
    /// holds none, which is not an error.
    pub fn forget(&self, run: &RunId) -> Prepared<()> {
        let held = self.active.iter().any(|(held, _)| held == run);
        Prepared {
            events: held
                .then(|| TeachingEvent::Cleared { run: run.clone() })
                .into_iter()
                .collect(),
            answer: (),
        }
    }
}

#[cfg(test)]
mod tests;
