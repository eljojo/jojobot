//! Port error classes for the single writer's uncertainty scope.

use crate::writer::LOCK_HELD;
use crate::writer::recovery::StoreError;
use jojobot_domain::mailbox::MailboxError;
use jojobot_domain::memory::MemoryError;
use jojobot_domain::push::PushError;
use jojobot_domain::request::RequestError;
use jojobot_domain::session::SessionError;
use jojobot_domain::teaching::TeachingError;

macro_rules! classify {
    ($error:ty, $native:expr) => {
        impl StoreError for $error {
            fn store(why: String) -> Self {
                Self::Store(why)
            }
            fn store_native(why: String) -> Self {
                let public: fn(String) -> String = $native;
                Self::Store(public(why))
            }
            fn indeterminate() -> Self {
                Self::Indeterminate
            }
            fn unknown(&self) -> bool {
                matches!(self, Self::Store(_) | Self::Indeterminate)
            }
        }
    };
}

/// **The public sentence of a store that did not answer.** A write that gave up
/// waiting for the log's lock says another writer holds the log, since the store
/// answered and the log is not its to write; every other cause is a store that
/// could not be reached. The cause may arrive wrapped by the layers it crossed.
fn unreached(why: &str, store: &str) -> String {
    if why.contains(LOCK_HELD) {
        format!("{LOCK_HELD}, so the {store} store was not written")
    } else {
        format!("the {store} store could not be reached")
    }
}

classify!(MemoryError, |why| unreached(&why, "memory"));
classify!(MailboxError, |why| unreached(&why, "mailbox"));
classify!(SessionError, |why| unreached(&why, "session"));
classify!(RequestError, |why| unreached(&why, "request"));
classify!(TeachingError, |why| why);
classify!(jojobot_domain::log::LogError, |why| why);
classify!(PushError, |why| why);

#[cfg(test)]
mod tests {
    use super::*;

    /// What a port error holds after the classifier, whichever of the four classes.
    fn said(class: &str, why: &str) -> String {
        let why = why.to_string();
        match class {
            "memory" => format!("{:?}", MemoryError::store_native(why)),
            "mailbox" => format!("{:?}", MailboxError::store_native(why)),
            "session" => format!("{:?}", SessionError::store_native(why)),
            _ => format!("{:?}", RequestError::store_native(why)),
        }
    }

    /// **A lock held elsewhere is named; every other cause keeps the sentence for a
    /// store that could not be reached.** The cause also arrives wrapped by the
    /// layers it crossed, and a pool that was only busy is not the lock.
    #[test]
    fn a_held_lock_is_named_and_every_other_cause_keeps_the_unreached_sentence() {
        let wrapped = format!(
            "encountered unexpected or invalid data: the ledger could not reach the log: {LOCK_HELD}"
        );
        for class in ["memory", "mailbox", "session", "request"] {
            for held in [LOCK_HELD.to_string(), wrapped.clone()] {
                let said = said(class, &held);
                assert!(said.contains(LOCK_HELD), "{class}: {said}");
                assert!(!said.contains("could not be reached"), "{class}: {said}");
            }
            for other in [
                "pool timed out while waiting for an open connection",
                "connection reset by peer",
                "",
            ] {
                let said = said(class, other);
                assert!(
                    said.contains(&format!("the {class} store could not be reached")),
                    "{class}: {other}: {said}"
                );
                assert!(!said.contains(LOCK_HELD), "{class}: {other}: {said}");
            }
        }
    }
}
