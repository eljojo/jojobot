//! **The actor of a write: who the claim is attributed to.**
//!
//! The operator, a bot, an app and jojobot itself are all actors, and a write
//! function takes the actor as data (rule 731: one kind of actor, one permission
//! check). An app is no entity, so it has no bot handle to write as: an app's
//! claim records no writer and no session, and carries the app's mark instead.
//!
//! Only `Bot` and `App` are passed by a caller today. `Operator` and `System`
//! are named so the type is the event log's one actor; a write function refuses
//! them until a card needs them.

use crate::app::AppStamp;
use crate::memory::EntityId;

/// **Who a write is attributed to.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// A bot, as its writer is filed (`authorship::stored_writer`).
    Bot(EntityId),
    /// An app. The stamp names the app, the operation its grant allowed and the
    /// moment the request was received, and is what the claim carries.
    App(AppStamp),
    /// The operator. No write function takes it yet.
    Operator,
    /// A named action of jojobot itself. No write function takes it yet.
    System,
}

impl Actor {
    /// The bot, when the actor is one.
    pub fn bot(&self) -> Option<&EntityId> {
        match self {
            Actor::Bot(bot) => Some(bot),
            _ => None,
        }
    }

    /// The app's mark, when the actor is an app.
    pub fn app_stamp(&self) -> Option<&AppStamp> {
        match self {
            Actor::App(stamp) => Some(stamp),
            _ => None,
        }
    }

    /// **How a message names its sender.** A bot is its handle. An app is
    /// `app/NAME`: a slash cannot be in a handle, so no bot or person can take
    /// the spelling and a reply to it finds no mailbox. `None` for an actor that
    /// sends no message yet.
    pub fn sender(&self) -> Option<String> {
        match self {
            Actor::Bot(bot) => Some(bot.as_str().to_string()),
            Actor::App(stamp) => Some(format!("app/{}", stamp.app)),
            Actor::Operator | Actor::System => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> AppStamp {
        AppStamp {
            app: "springfield-ci".into(),
            grant: "capture".into(),
            received_at: "2026-10-30T16:00:00Z".into(),
        }
    }

    #[test]
    fn a_bot_is_its_handle_and_an_app_is_a_spelling_no_handle_can_take() {
        let bot = Actor::Bot(EntityId("bot:alpha".into()));
        assert_eq!(bot.sender().as_deref(), Some("bot:alpha"));
        assert_eq!(bot.bot().map(|b| b.as_str()), Some("bot:alpha"));
        assert!(bot.app_stamp().is_none());

        let app = Actor::App(stamp());
        let sender = app.sender().expect("an app sends");
        assert_eq!(sender, "app/springfield-ci");
        assert!(crate::memory::validate_shape(&EntityId(sender)).is_err());
        assert!(app.bot().is_none());
        assert_eq!(app.app_stamp(), Some(&stamp()));
    }

    #[test]
    fn the_operator_and_the_system_send_no_message_yet() {
        assert_eq!(Actor::Operator.sender(), None);
        assert_eq!(Actor::System.sender(), None);
    }
}
