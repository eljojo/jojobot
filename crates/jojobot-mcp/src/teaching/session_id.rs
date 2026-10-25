//! The teaching about a session id.

/// **The sentence for [`SESSION_ID_DOMAIN`]**, naming the run's name and the
/// record the answer calls `session`, so a caller sees its own run reflected
/// back and has nothing to work out. The name is what a resume offer lists;
/// the key is what every call takes, and is never written anywhere.
pub(crate) fn session_id_teaching(name: &str, session: &str) -> String {
    format!(
        "Your run is called {name}, and the `session` {session} in this answer is its stored \
         record. Cite the run by its name, which is what a later boot offers when it asks you \
         to resume. Your `sid` is the run's key: it is what every call takes, it is yours \
         alone, and nothing you write should hold it."
    )
}
