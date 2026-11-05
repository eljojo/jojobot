//! **The claims family on the event log.** Claim, field and identity events are
//! stored as the log envelope with a typed payload; [`codec`] is their stored
//! form. The domain keeps no wire format.

pub mod codec;

#[cfg(test)]
mod codec_tests;
