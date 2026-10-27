//! Port error classes for the single writer's uncertainty scope.

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

classify!(MemoryError, |_| "the memory store could not be reached"
    .into());
classify!(MailboxError, |_| "the mailbox store could not be reached"
    .into());
classify!(SessionError, |_| "the session store could not be reached"
    .into());
classify!(RequestError, |_| "the request store could not be reached"
    .into());
classify!(TeachingError, |why| why);
classify!(PushError, |why| why);
