//! Change notices use the same versions and histories as a held-change answer.
//! The cursor belongs to the harness, so reading it records nothing here.

use super::*;

impl Jojobot {
    pub(crate) async fn harness_changes(&self, position: Value) -> Result<Value, String> {
        let starting = position.is_null();
        let previous: Vec<(Held, Version)> = if starting {
            Vec::new()
        } else {
            serde_json::from_value(position).map_err(|error| error.to_string())?
        };
        let mut wanted: Vec<Held> = self
            .memory
            .list_entities(None)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|entity| Held::Entity(entity.id.to_string()))
            .collect();
        let mail = self
            .mailboxes
            .scan_messages()
            .await
            .map_err(|error| error.to_string())?;
        wanted.extend(
            mail.iter()
                .map(|message| Held::Message(message.id.to_string())),
        );
        for (what, _) in &previous {
            if !wanted.contains(what) {
                wanted.push(what.clone());
            }
        }
        let messages = HeldMessages(
            mail.into_iter()
                .map(|message| (message.id.0.clone(), message))
                .collect(),
        );
        let mut current = Vec::with_capacity(wanted.len());
        let mut notices = Vec::new();
        for what in wanted {
            let now = self
                .version_of(&what, &messages)
                .await
                .ok_or_else(|| format!("the observed version of {what:?} could not be read"))?;
            match previous.iter().find(|(held, _)| held == &what) {
                Some((_, was)) if was != &now => {
                    match self.what_changed(&what, was, &now, "").await {
                        Changed::Told(notice) => notices.push((what.clone(), notice)),
                        Changed::Own => {}
                        Changed::Unread => {
                            return Err(format!("the changes to {what:?} could not be read"));
                        }
                    }
                }
                None if !starting => {
                    let notice = match &what {
                        Held::Entity(handle) => serde_json::json!({"handle":handle,"created":true}),
                        Held::Message(id) => serde_json::json!({"message":id,"created":true}),
                        Held::Claim(_) => unreachable!("the cursor holds entities and mail"),
                    };
                    notices.push((what.clone(), notice));
                }
                _ => {}
            }
            current.push((what, now));
        }
        Ok(serde_json::json!({"position":current,"notices":notices_to_send(&notices)}))
    }
}
