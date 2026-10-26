//! Economy settings read from the same image for search and write receipts.

use crate::{Jojobot, caller::Caller};

pub(crate) const SETTING_UNREADABLE: &str =
    "Served the normal mode because the token-saving setting could not be read.";

pub(crate) struct TokenSaving {
    pub(crate) on: bool,
    readable: bool,
}

impl Jojobot {
    pub(crate) fn token_saving(&self, caller: Option<&Caller>) -> TokenSaving {
        let Some(caller) = caller else {
            return TokenSaving {
                on: false,
                readable: true,
            };
        };
        match self.memory.image_field(&caller.bot, "setting_token_saving") {
            Some(value) => TokenSaving {
                on: value.as_deref() == Some("on"),
                readable: true,
            },
            None => TokenSaving {
                on: false,
                readable: false,
            },
        }
    }
}

impl TokenSaving {
    pub(crate) fn explain(&self, body: &mut serde_json::Value) {
        if !self.readable {
            body["token_saving"] = serde_json::json!({
                "mode": "normal",
                "setting_read": false,
                "note": SETTING_UNREADABLE
            });
        }
    }
}
