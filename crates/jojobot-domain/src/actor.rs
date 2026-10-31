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

use crate::app::{AppId, AppStamp};
use crate::memory::EntityId;

/// **Who a write is attributed to.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// A bot, as its writer is filed (`authorship::stored_writer`).
    Bot(EntityId),
    /// An app, by its name. The mark the claim carries (the grant and the
    /// moment) belongs to the call, not to the actor, and is passed beside it.
    App(AppId),
    /// The operator. No write function takes it yet.
    Operator,
    /// A named action of jojobot itself. No write function takes it yet.
    System,
}

/// **Who a write is attributed to, and the mark the call adds.** The mark is
/// the app's grant and the moment the request came in; a bot's write has none.
#[derive(Debug, Clone, Copy)]
pub struct Attribution<'a> {
    pub actor: &'a Actor,
    pub app_stamp: Option<&'a AppStamp>,
}

impl<'a> Attribution<'a> {
    /// A write by a bot, which carries no app mark.
    pub fn bot(actor: &'a Actor) -> Self {
        Self {
            actor,
            app_stamp: None,
        }
    }
}

impl Actor {
    /// The bot, when the actor is one.
    pub fn bot(&self) -> Option<&EntityId> {
        match self {
            Actor::Bot(bot) => Some(bot),
            _ => None,
        }
    }

    /// The app, when the actor is one.
    pub fn app(&self) -> Option<&AppId> {
        match self {
            Actor::App(app) => Some(app),
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
            Actor::App(app) => Some(format!("app/{app}")),
            Actor::Operator | Actor::System => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_id() -> AppId {
        AppId::parse("springfield-ci").unwrap()
    }

    #[test]
    fn a_bot_is_its_handle_and_an_app_is_a_spelling_no_handle_can_take() {
        let bot = Actor::Bot(EntityId("bot:alpha".into()));
        assert_eq!(bot.sender().as_deref(), Some("bot:alpha"));
        assert_eq!(bot.bot().map(|b| b.as_str()), Some("bot:alpha"));
        assert!(bot.app().is_none());

        let app = Actor::App(app_id());
        let sender = app.sender().expect("an app sends");
        assert_eq!(sender, "app/springfield-ci");
        assert!(crate::memory::validate_shape(&EntityId(sender)).is_err());
        assert!(app.bot().is_none());
        assert_eq!(app.app(), Some(&app_id()));
        assert_eq!(
            app,
            Actor::App(app_id()),
            "one app is one actor, whatever the call"
        );
    }

    #[test]
    fn the_operator_and_the_system_send_no_message_yet() {
        assert_eq!(Actor::Operator.sender(), None);
        assert_eq!(Actor::System.sender(), None);
    }
}
