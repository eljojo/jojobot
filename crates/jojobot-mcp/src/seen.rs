//! **What a session was shown, kept in this process and consulted once.**
//!
//! The near-miss guard compares a creation with the STORE, so it can never see
//! that the thing a model is about to create sits beside one its own search
//! returned a few calls ago, when the two share no spelling the guard reads.
//! This remembers, per session, which things the read verbs returned, and lets
//! a creation that landed ask one question about one of them: *is this the same
//! thing?*
//!
//! **It surfaces and never decides** (rule 246). Nothing is refused, no answer
//! of any read changes, and the question rides the receipt of the creation that
//! raised it, once per thing shown (rule 252).
//!
//! **Nothing is stored.** The ledger lives in the process, keyed by the session
//! handle the registry already shares across connections. A restart empties it
//! and the next creation simply gets no question: a missed note, never a wrong
//! one.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use jojobot_domain::memory::EntityKind;

/// **How many things one session remembers having been shown.** The oldest
/// shown goes first. Real sittings were shown a median of 7 distinct things and
/// at most 42 over a whole run, so this holds every one of them with room.
pub(crate) const MAX_SHOWN_PER_SESSION: usize = 200;

/// **How many sessions the ledger holds at once.** The session that has gone
/// longest without a new thing shown goes first.
pub(crate) const MAX_SESSIONS: usize = 512;

/// The shortest word that can be shared. Shorter words are articles and
/// connectives, which two unrelated things share.
const MIN_WORD: usize = 4;

/// One entity a read returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) handle: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    /// The handle this thing sits under, as the read served it. `None` for a
    /// root, and for a read that carried no `parent` key.
    pub(crate) parent: Option<String>,
}

/// What a creation is asked about: the thing shown, how long ago, and the word
/// the two share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Noticed {
    pub(crate) handle: String,
    pub(crate) name: String,
    pub(crate) calls_ago: u64,
    pub(crate) word: String,
}

#[derive(Debug, Default)]
struct Session {
    /// Calls this session has made, counted as answers it was handed.
    calls: u64,
    /// What it was shown, oldest first, each with the call it was last shown on.
    shown: VecDeque<(Shown, u64)>,
    /// Things it has already been asked about. **Once per thing shown**: a
    /// second creation beside the same thing is not asked again.
    asked: HashSet<String>,
}

/// **The whole ledger.**
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    sessions: HashMap<String, Session>,
    /// Sessions by when something was last shown to them, oldest first.
    order: VecDeque<String>,
}

impl Ledger {
    /// Count one call this session made.
    pub(crate) fn tick(&mut self, sid: &str) {
        if let Some(session) = self.sessions.get_mut(sid) {
            session.calls += 1;
        }
    }

    /// Remember what a read returned.
    pub(crate) fn record(&mut self, sid: &str, returned: Vec<Shown>) {
        if returned.is_empty() {
            return;
        }
        if !self.sessions.contains_key(sid) {
            self.sessions.insert(sid.to_string(), Session::default());
        }
        self.order.retain(|held| held != sid);
        self.order.push_back(sid.to_string());
        while self.order.len() > MAX_SESSIONS {
            if let Some(oldest) = self.order.pop_front() {
                self.sessions.remove(&oldest);
            }
        }
        let Some(session) = self.sessions.get_mut(sid) else {
            return;
        };
        for entity in returned {
            session
                .shown
                .retain(|(held, _)| held.handle != entity.handle);
            session.shown.push_back((entity, session.calls));
        }
        while session.shown.len() > MAX_SHOWN_PER_SESSION {
            session.shown.pop_front();
        }
    }

    /// **Forget a handle that was merged away, in every session.** The handle
    /// answers as a status now and is no thing to ask about, and a merge
    /// suggested for it would come back already done.
    pub(crate) fn forget(&mut self, handle: &str) {
        for session in self.sessions.values_mut() {
            session.shown.retain(|(held, _)| held.handle != handle);
        }
    }

    /// **Ask, once, about the thing this creation sits beside.**
    ///
    /// The thing must be of the same kind, must have been shown to this session
    /// by a read, and must share a word with the creation. Of the things that
    /// qualify, the one sharing the most words is asked about, the most recently
    /// shown breaking a tie. A thing this session was already asked about is
    /// passed over, so the question is never repeated.
    ///
    /// **A word the parent itself gives is not shared between two things under
    /// that parent.** Every handle under a project begins with the project's
    /// slug, so two things side by side would share it by construction. For a
    /// shown thing with the same parent as the creation, the words of that
    /// parent's slug come out of both word sets before they are compared.
    /// A thing under another parent, or none, is compared whole.
    pub(crate) fn consult(
        &mut self,
        sid: &str,
        handle: &str,
        kind: &str,
        name: &str,
        aliases: &[String],
        parent: Option<&str>,
    ) -> Option<Noticed> {
        let session = self.sessions.get_mut(sid)?;
        let slug = handle.split_once(':').map_or(handle, |(_, slug)| slug);
        let mut incoming = words(name);
        incoming.extend(words(slug));
        incoming.extend(aliases.iter().flat_map(|alias| words(alias)));
        let parents_words = parent.map(|p| words(p.split_once(':').map_or(p, |(_, slug)| slug)));

        let mut best: Option<(usize, usize, &Shown, String)> = None;
        for (at, (shown, _)) in session.shown.iter().enumerate() {
            if shown.kind != kind || session.asked.contains(&shown.handle) {
                continue;
            }
            let mut theirs = words(&shown.name);
            theirs.extend(words(shown.handle.split_once(':').map_or("", |(_, s)| s)));
            theirs.extend(shown.aliases.iter().flat_map(|alias| words(alias)));
            // Equal parents, both present: the parent's words are the parent's.
            let own_parent_words = match (&parents_words, &shown.parent) {
                (Some(words), Some(held)) if Some(held.as_str()) == parent => Some(words),
                _ => None,
            };
            let shared: Vec<&String> = incoming
                .intersection(&theirs)
                .filter(|word| own_parent_words.is_none_or(|parents| !parents.contains(*word)))
                .collect();
            let Some(word) = shared.first() else {
                continue;
            };
            let better = match &best {
                None => true,
                Some((count, seen_at, _, _)) => (shared.len(), at) > (*count, *seen_at),
            };
            if better {
                best = Some((shared.len(), at, shown, (*word).clone()));
            }
        }
        let (_, at, shown, word) = best?;
        let calls_ago = session.calls.saturating_sub(session.shown[at].1);
        let noticed = Noticed {
            handle: shown.handle.clone(),
            name: shown.name.clone(),
            calls_ago,
            word,
        };
        session.asked.insert(noticed.handle.clone());
        Some(noticed)
    }
}

impl Noticed {
    /// **The question, in the words the receipt carries.** It names the thing
    /// shown and how long ago, the word the two share, the verb that folds one
    /// into the other, and what the archive verb would leave behind. It asks;
    /// nothing was refused.
    pub(crate) fn question(&self, created: &str) -> String {
        format!(
            "This session was shown {} ({}) {} ago, and {created} shares the word '{}' with it. \
             If they are the same thing, merge_entities folds one into the other. \
             archive_entity would leave its claims where they are.",
            self.handle,
            self.name,
            crate::answer::counted(self.calls_ago as usize, "call", "calls"),
            self.word,
        )
    }
}

/// **The words a name is made of that two things could share.** Lowercased, at
/// least [`MIN_WORD`] letters, and never a bare number.
///
/// There is deliberately no stoplist and no rarity test. Over 188 real
/// creations neither removed a single pair, so a threshold would be invented. A
/// run showing a common word that fires a false question is what would justify
/// adding rarity (how many other things of the kind carry the word), and it
/// should be added then, with the run as its sample.
///
/// **The one word removed is not a rarity rule.** A run did show a false
/// question: two things under one project shared the project's own slug. That
/// word is not common and it is not rare; it is given by the parent, so it says
/// nothing about whether the two are the same thing. [`Ledger::consult`] removes
/// the parent's slug words for a shown thing under the same parent and for
/// nothing else, so no word is dropped for how often it occurs.
fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_WORD)
        .filter(|word| word.chars().any(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

/// **Every entity a read answer returned**, read off the body before it is sent.
///
/// An entity is an object with an `id` that is a handle of a kind this build
/// knows, wherever it sits: a hit's `entity`, a fact hit's `about` and `home`,
/// a recall's objects and the things they are connected to. Anything else with
/// an `id` — a message, a claim address — is not a handle and is passed over.
pub(crate) fn entities_in(body: &serde_json::Value) -> Vec<Shown> {
    fn walk(value: &serde_json::Value, out: &mut Vec<Shown>) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(handle) = fields.get("id").and_then(serde_json::Value::as_str)
                    && let Some((kind, slug)) = handle.split_once(':')
                    && EntityKind::ALL.iter().any(|known| known.as_token() == kind)
                    && !slug.is_empty()
                    && !slug.contains('#')
                    && let Some(name) = fields.get("name").and_then(serde_json::Value::as_str)
                    // A merged-away handle answers as a status naming its
                    // survivor. It is no thing, so it is not shown.
                    && fields.get("status").and_then(serde_json::Value::as_str) != Some("merged")
                {
                    out.push(Shown {
                        handle: handle.to_string(),
                        kind: kind.to_string(),
                        name: name.to_string(),
                        aliases: fields
                            .get("alternateName")
                            .and_then(serde_json::Value::as_array)
                            .map(|all| {
                                all.iter()
                                    .filter_map(serde_json::Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        parent: fields
                            .get("parent")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    });
                }
                for held in fields.values() {
                    walk(held, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(body, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(handle: &str, name: &str) -> Shown {
        Shown {
            handle: handle.into(),
            kind: "place".into(),
            name: name.into(),
            aliases: Vec::new(),
            parent: None,
        }
    }

    fn work(handle: &str, name: &str, parent: Option<&str>) -> Shown {
        Shown {
            handle: handle.into(),
            kind: "work".into(),
            name: name.into(),
            aliases: Vec::new(),
            parent: parent.map(str::to_string),
        }
    }

    /// **Words are four letters or more and never a bare number.**
    #[test]
    fn a_word_is_four_letters_or_more_and_never_a_bare_number() {
        let found = words("The Old Mill 2026 Road 12b");
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            vec!["mill".to_string(), "road".to_string()],
            "short words and bare numbers are not words two things share",
        );
    }

    /// **Of the things that share a word, the one sharing the most is asked
    /// about, and the most recently shown breaks a tie.**
    #[test]
    fn the_thing_sharing_the_most_words_is_the_one_asked_about() {
        let mut ledger = Ledger::default();
        ledger.record(
            "s1",
            vec![
                place("place:atlas", "Old Mill Road"),
                place("place:bet", "Mill Pond"),
            ],
        );
        let noticed = ledger
            .consult(
                "s1",
                "place:springfield-mall",
                "place",
                "Mill Road Bridge",
                &[],
                None,
            )
            .expect("a thing shared words with the creation");
        assert_eq!(noticed.handle, "place:atlas", "{noticed:?}");

        // A tie goes to the more recently shown.
        let mut ledger = Ledger::default();
        ledger.record("s2", vec![place("place:atlas", "Aaa Wharf")]);
        ledger.record("s2", vec![place("place:bet", "Bbb Wharf")]);
        let noticed = ledger
            .consult("s2", "place:wharf-road", "place", "Wharf Road", &[], None)
            .expect("a thing shared a word");
        assert_eq!(noticed.handle, "place:bet", "{noticed:?}");
    }

    /// **A session remembers at most `MAX_SHOWN_PER_SESSION` things, newest
    /// kept.** The thing shown first goes when the cap is passed; a thing shown
    /// again is moved to the newest end rather than counted twice.
    #[test]
    fn a_session_keeps_the_newest_things_it_was_shown() {
        let handle = |n: usize| format!("place:{n}");
        let mut ledger = Ledger::default();
        let many: Vec<Shown> = (0..MAX_SHOWN_PER_SESSION + 5)
            .map(|n| place(&handle(n), &format!("Wharf {n}")))
            .collect();
        ledger.record("s1", many);
        ledger.record("s1", vec![place(&handle(7), "Wharf 7")]);
        let session = ledger.sessions.get("s1").expect("the session is held");
        assert_eq!(session.shown.len(), MAX_SHOWN_PER_SESSION, "the cap held");
        let handles: Vec<&str> = session
            .shown
            .iter()
            .map(|(s, _)| s.handle.as_str())
            .collect();
        assert!(
            !handles.contains(&handle(0).as_str()),
            "the oldest was kept"
        );
        assert_eq!(
            handles.last(),
            Some(&handle(7).as_str()),
            "a thing shown again is newest"
        );
        assert_eq!(
            handles.iter().filter(|h| **h == handle(7)).count(),
            1,
            "a thing shown twice was held twice",
        );
    }

    /// **The ledger holds at most `MAX_SESSIONS` sessions, the one that went
    /// longest without a new thing shown going first.**
    #[test]
    fn the_session_quiet_the_longest_is_forgotten_first() {
        let mut ledger = Ledger::default();
        for n in 0..=MAX_SESSIONS {
            ledger.record(
                &format!("s{n}"),
                vec![place("place:wharf-road", "Wharf Road")],
            );
        }
        assert_eq!(ledger.sessions.len(), MAX_SESSIONS, "the cap held");
        assert!(
            !ledger.sessions.contains_key("s0"),
            "the oldest session was kept"
        );
        assert!(
            ledger.sessions.contains_key(&format!("s{MAX_SESSIONS}")),
            "the newest session was dropped",
        );
    }

    /// **An id that is not a handle is not a thing shown**: a claim's address
    /// and a message's id carry an `id` or an address and are passed over.
    #[test]
    fn only_a_handle_with_a_name_is_a_thing_shown() {
        let body = serde_json::json!({
            "results": [
                {"id": "place:wonder-wharf", "name": "Wonder Wharf", "alternateName": ["The Wharf"]},
                {"id": "place:atlas#f3", "name": "a claim address"},
                {"id": "fxjy1w", "name": "a message"},
                {"id": "mystery:thing", "name": "an unknown kind"},
                {"id": "place:bet"},
            ]
        });
        let found = entities_in(&body);
        assert_eq!(
            found,
            vec![Shown {
                handle: "place:wonder-wharf".into(),
                kind: "place".into(),
                name: "Wonder Wharf".into(),
                aliases: vec!["The Wharf".into()],
                parent: None,
            }],
        );
    }

    /// **A word only the parent gives is not a word two siblings share.** Under a
    /// project every handle begins with the project's own slug, so two things
    /// beside each other would be asked about on that word alone. The question
    /// stays for a word the parent does not give, in the case that follows.
    #[test]
    fn a_word_only_the_parent_gives_is_not_a_word_two_siblings_share() {
        let mut ledger = Ledger::default();
        ledger.record(
            "s1",
            vec![work(
                "work:atlas-x-donut-stand",
                "X Donut Stand",
                Some("project:atlas"),
            )],
        );
        let asked = ledger.consult(
            "s1",
            "work:atlas-kwik-e-mart",
            "work",
            "Kwik E Mart",
            &[],
            Some("project:atlas"),
        );
        assert_eq!(
            asked, None,
            "the parent's own word was read as a shared one"
        );

        // The same thing is still there to be asked about: a sibling that shares
        // a word the parent does not give is asked, so the case above did not
        // pass by the ledger being empty or the thing having been spent.
        let asked = ledger
            .consult(
                "s1",
                "work:atlas-donut-stand-two",
                "work",
                "Donut Stand Two",
                &[],
                Some("project:atlas"),
            )
            .expect("a sibling that shares a word the parent does not give is asked about");
        assert_eq!(asked.handle, "work:atlas-x-donut-stand");
        assert_eq!(asked.word, "donut", "the word shared is not the parent's");
    }

    /// **Two siblings that share a word the parent does not give are asked
    /// about, and the question names that word.** Paired with the case above:
    /// a rule that dropped every word under a parent would pass that one and
    /// fail this one.
    #[test]
    fn siblings_sharing_a_word_the_parent_does_not_give_are_asked_about_it() {
        let mut ledger = Ledger::default();
        ledger.record(
            "s1",
            vec![work(
                "work:atlas-x-donut-stand",
                "X Donut Stand",
                Some("project:atlas"),
            )],
        );
        let asked = ledger
            .consult(
                "s1",
                "work:atlas-donut-stand-two",
                "work",
                "Donut Stand Two",
                &[],
                Some("project:atlas"),
            )
            .expect("two siblings sharing leaf and preview are asked about");
        assert_eq!(asked.handle, "work:atlas-x-donut-stand");
        assert_ne!(asked.word, "atlas", "{asked:?}");
    }

    /// **A thing under another parent, or under none, shares the word with the
    /// creation, and is asked about it.** The same word in two places that have
    /// nothing in common is evidence; under one parent it is the parent's.
    #[test]
    fn a_thing_under_another_parent_or_none_still_shares_the_word() {
        let shown = |parent: Option<&str>| {
            let mut ledger = Ledger::default();
            ledger.record(
                "s1",
                vec![work("work:atlas-x-donut-stand", "X Donut Stand", parent)],
            );
            ledger
        };
        let created = |ledger: &mut Ledger, parent: Option<&str>| {
            ledger.consult(
                "s1",
                "work:atlas-kwik-e-mart",
                "work",
                "Kwik E Mart",
                &[],
                parent,
            )
        };
        // The shown thing is under another parent.
        let asked = created(&mut shown(Some("project:the-shed")), Some("project:atlas"))
            .expect("another parent: asked");
        assert_eq!(asked.word, "atlas");
        // The creation is under none.
        let asked = created(&mut shown(Some("project:atlas")), None).expect("no parent: asked");
        assert_eq!(asked.word, "atlas");
        // The shown thing is under none.
        let asked =
            created(&mut shown(None), Some("project:atlas")).expect("shown under none: asked");
        assert_eq!(asked.word, "atlas");
    }

    /// **The parent is read off the shape a read answer actually carries.** The
    /// body is what `entity_json` emits, which every read verb serves its
    /// entities through, not a shape written here to fit the reader.
    #[test]
    fn the_parent_is_read_from_what_a_read_serves() {
        let entity = |id: &str, parent: Option<&str>| jojobot_domain::memory::Entity {
            id: jojobot_domain::memory::EntityId(id.into()),
            kind: EntityKind::WORK,
            name: "Atlas Thing".into(),
            aliases: Vec::new(),
            source: "the roster".into(),
            crm: None,
            parent: parent.map(|p| jojobot_domain::memory::EntityId(p.into())),
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        };
        let body = serde_json::json!({
            "entities": [
                crate::memory::wire::entity_json(&entity("work:atlas-child", Some("project:atlas"))),
                crate::memory::wire::entity_json(&entity("work:atlas-root", None)),
            ]
        });
        let found = entities_in(&body);
        let parent_of = |handle: &str| {
            found
                .iter()
                .find(|shown| shown.handle == handle)
                .unwrap_or_else(|| panic!("{handle} was not read: {found:?}"))
                .parent
                .clone()
        };
        assert_eq!(
            parent_of("work:atlas-child").as_deref(),
            Some("project:atlas")
        );
        assert_eq!(parent_of("work:atlas-root"), None);
    }
}
