//! Attention — what deserves the user's eyes: watches, decay items, rhythms,
//! quiet plants, the focus surface. Consumes the graph; decides what surfaces
//! at boot and in shapes.
//!
//! TODO: skeleton only — no types defined yet.
//!
//! # A late check-in names its own date, and there is no default
//!
//! When a check-in is recorded after the moment it was due, the caller says
//! which date the rhythm advances from: the date it was due, or the date the
//! check-in happened. Neither is the default. Omitting the choice is refused
//! with a way forward, rather than resolved quietly.
//!
//! This is a deliberate exception to convention over configuration, which
//! otherwise requires every default to work unconfigured. The operator set it
//! **for now**, and it may become a default in a later version — so a default
//! added here without the operator saying so is a bug rather than a
//! convenience.
//!
//! Why it cannot be guessed: pick wrong, and a backdated check-in never
//! clears the reminder. It fails silently and keeps failing, because each
//! late check-in re-arms the thing it was meant to settle. The reducer and
//! the procedure text take the answer from one place, or they disagree about
//! which date a rhythm counts from.
