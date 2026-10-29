//! **How far along a list is by its own ticks, and the day it was last ticked**,
//! worked out in one place.
//!
//! A list read carries these two blocks on `recall`, and the page of a view that
//! lists lists shows the same two values. Both call [`TickRead::of`] and neither
//! keeps a second count (rule 498). Nothing is stored: the blocks are worked out
//! from the list's own records at each read.

use jojobot_domain::memory::Fact;
use jojobot_domain::memory::entries::{self, Setting, Ticked};

/// **What the ticks of one list come to.**
pub struct TickRead {
    ticked: Option<Ticked>,
    last_ticked: Option<jiff::civil::Date>,
}

impl TickRead {
    /// **Read the ticks of a list** from the records it holds and the setting
    /// its fields name. A list with no entry that stands has no count, and one
    /// with no tick that stands has no day.
    pub fn of(held: &[Fact], setting: Setting) -> TickRead {
        TickRead {
            ticked: entries::ticked(held, setting),
            last_ticked: entries::last_ticked(held),
        }
    }

    /// **The blocks a `recall` of the list carries**: `ticked` and `last_ticked`,
    /// each only when there is something to say.
    pub fn write_into(&self, read: &mut serde_json::Map<String, serde_json::Value>) {
        if let Some(ticked) = self.ticked {
            read.insert("ticked".into(), ticked_block(ticked));
        }
        if let Some(day) = self.last_ticked {
            read.insert("last_ticked".into(), day.to_string().into());
        }
    }

    /// **The count as a reader says it**, empty when the list holds no count.
    pub fn ticked_in_words(&self) -> String {
        self.ticked.map(ticked_words).unwrap_or_default()
    }

    /// **The day of the newest tick that stands**, empty when none does.
    pub fn last_ticked_day(&self) -> String {
        self.last_ticked
            .map(|day| day.to_string())
            .unwrap_or_default()
    }
}

/// **The count as a reader says it**: a checklist says done out of all, a queue
/// says done and open, a rotation says its turns.
fn ticked_words(ticked: Ticked) -> String {
    match ticked {
        Ticked::Checklist { done, of } => format!("{done} of {of} done"),
        Ticked::Queue { done, open } => format!("{done} done, {open} open"),
        Ticked::Rotation { turns } => format!("{turns} turns"),
    }
}

/// **The block a list read carries for its ticks**: a checklist says done out of
/// all and reads as a sentence, a queue says done and open, a rotation says turns.
fn ticked_block(ticked: Ticked) -> serde_json::Value {
    match ticked {
        Ticked::Checklist { done, of } => {
            serde_json::json!({"done": done, "of": of, "reads": ticked_words(ticked)})
        }
        Ticked::Queue { done, open } => serde_json::json!({"done": done, "open": open}),
        Ticked::Rotation { turns } => serde_json::json!({"turns": turns}),
    }
}
