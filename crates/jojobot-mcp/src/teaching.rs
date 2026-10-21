//! **Teaching a session, once, the first time it touches a domain.**
//!
//! The problem: a rule an agent needs is sometimes true only at the moment it
//! decides whether to write — "a further claim does not destroy the one
//! already there" has to reach a session before it decides not to write a
//! second, contradicting claim. A footer on every read reaches that session
//! too, but reaches every other one forever. A boot-time essay never reaches
//! a session already mid-run. What is wanted is once, on the call that
//! actually raised the question — see [`jojobot_domain::teaching`].
//!
//! **The mechanism is generic; the content is the caller's.** `first_contact`
//! knows nothing about domains or wording — a caller names a domain string
//! and gets back whether this is the first time. Composing that into the
//! answer, and deciding WHAT counts as "touching" a domain, is each verb's
//! own call: a search that matched nothing never touched claims; a search
//! that surfaced a fact did.

use super::*;

/// The first domain this slice teaches. A caller-declared string, not a
/// compiled set — a second domain is a second constant beside this one, not
/// a variant added here.
pub(crate) const CLAIMS_DOMAIN: &str = "claims";

/// **Ships in the binary, composed into the answer — never stored as a
/// row.** An upgrade improves the wording for every instance with nothing to
/// migrate.
pub(crate) const CLAIMS_TEACHING: &str = "A further claim does not destroy the one already \
    there — capture a second claim about the same thing and it does not erase the first, even \
    when the two contradict each other. jojobot runs no inference and settles nothing, so two \
    accounts that disagree are both allowed to stand: recording the new one is not a judgment \
    that it is the true one, and deciding between them was never the job. Reach for capture when \
    a new thing happened; reach for update_fact only to correct what the record already says — \
    it rewrites a claim in place, and recall with history_record set to the claim's address \
    reads the earlier wording back.\n\n\
    Three corrections, three different moves. Mistyped it just now? update_fact in place — the \
    record becomes what it should have said, and the wrong wording stays readable through \
    history_record. It changed, or it was never true at all? Archive the old claim with update_fact \
    (status: archived, details saying why) and capture the new one, naming the archived claim as \
    the new one's derived_from when there is a direct replacement. A claim about a past event \
    that never happened? retract it, which leaves a dated record. \
    Archive it, never negate it: rewriting a claim into its own denial — 'the club does NOT meet \
    on Tuesdays' replacing 'the club meets on Tuesdays' — leaves a sentence about what is not so \
    where a claim about what is true belongs. A negative is still an ordinary fact when it is not \
    correcting anything: 'he did not attend' stands on its own.";

/// **The third domain — what a search can and cannot reach.** The matcher is
/// loose and reaches only a claim's current wording; a session learns that once.
/// A thin or empty answer carries it again, where the gap shows.
pub(crate) const SEARCH_CAVEATS_DOMAIN: &str = "search-caveats";

/// **The second domain — a convention, not a rule about claims themselves.**
/// A different string from [`CLAIMS_DOMAIN`], so a session already taught one
/// has not been taught the other: they are independent rows on the same
/// ledger, and the mechanism did not have to change to add this one.
pub(crate) const CLAIM_SUBJECT_DOMAIN: &str = "claim-subject";

/// **Ships in the binary, exactly as [`CLAIMS_TEACHING`] does.** No column,
/// no migration, no verb: `fields` already takes any key a caller writes, so
/// what was missing was agreement on the key, not a place to hold it.
///
/// **`purpose`'s values are open, deliberately.** No fixed list ships beside
/// it and none is named here — a closed set is a later move, made only if a
/// vocabulary actually stabilizes from use, not proposed in advance. The
/// loop is the design: this teaching names the key and says what it is for,
/// `answers_type`/`fits_type` already read back whatever values are in use,
/// and the convention settles by use rather than by enforcement.
pub(crate) const CLAIM_SUBJECT_TEACHING: &str = "A claim's fields may carry two keys nothing \
    enforces. `subject` is a one-line label, the way an email has a subject. `purpose` says \
    what the claim is FOR — its value is open, read back from whatever is already in use rather \
    than chosen from a fixed list, so it settles by use rather than by a list somebody \
    maintains. Both are written by whoever makes the claim. Together they let a later read \
    tell what a claim is about and why it was written without opening it, and keep two \
    sessions from inventing two different names for the same idea.";

/// **The third domain — a rhythm that already has a history when it is
/// made.** `add_entity` takes no `fields`, so nothing about a schedule can be
/// sent through it: the mistake this teaches against happens on the `capture`
/// that follows, once the caller reaches for the only keys it can see and
/// types them by hand. `capture` on a subject that does not exist is refused
/// (`Blocked::MustExist`), so the entity always precedes that capture — this
/// domain is taught at `add_entity` instead, which is a teaching landing
/// before the mistake rather than after it.
///
/// **Shared with the check-in-date question, deliberately.** A rhythm
/// created with history behind it and a check-in whose `happened_at`
/// disagrees with `recorded_at` used to be two domains stating the same
/// underlying fact — that a check-in's own `recorded_at` is what a
/// schedule counts from — one implicitly, one by field name. A session
/// that creates a loop and then backdates its first check-in with the
/// wrong field meets both, and telling it twice is the defect this domain
/// now closes by construction: whichever site reaches the session first
/// spends the one row, and the other notes nothing.
pub(crate) const RHYTHM_HISTORY_DOMAIN: &str = "rhythm-history";

/// **Ships in the binary, exactly as the other teachings do.** The fuller
/// of the domain's two texts, for the site a caller usually meets first:
/// creating the rhythm, before any check-in has been sent. States the
/// opening mechanism AND which field dates it, so a caller who reads this
/// has nothing left to get wrong on the check-in that follows.
pub(crate) const RHYTHM_HISTORY_TEACHING: &str = "A rhythm's `counts_from` and `due_on` are jojobot's arithmetic, never a caller's to type in — \
    even for the first cycle. A loop whose last run already happened, before this session opened it, is \
    opened the same way an ordinary cycle is closed: capture a check-in on it, and jojobot works \
    the rest of the schedule out from there. The day that check-in dates the schedule from is \
    recorded_at, never happened_at — send the day the turn actually happened as recorded_at, and \
    jojobot derives the rest.";

/// **The fourth domain — which entity a claim with an edge hangs on.** Named
/// on the call that actually raises the question: a `capture` drawing an
/// edge, never one that does not, because a claim naming no other entity has
/// no side to get wrong.
pub(crate) const CLAIM_DIRECTION_DOMAIN: &str = "claim-direction";

/// **Ships in the binary, exactly as the other teachings do.** The two
/// entities an edge touches are not interchangeable: one carries the claim's
/// content, the other is named by the edge, and nothing about the verb
/// stops a caller filing either side as the subject.
pub const CLAIM_DIRECTION_TEACHING: &str = "A claim hangs on the thing it is ABOUT; the \
    edge names the other party. Ask which entity the sentence itself describes, and put the \
    claim there — the edge then points at whichever other entity it also involves. Filing it on \
    the wrong side stores a claim that is true and finds nothing: a read scoped to the entity \
    the claim should have been about never sees it, because that read returns what it was \
    asked for and an inbound edge grants the other party nothing.";

/// **The ledger domain of a skill a session has read**, `skill-read:` and the
/// skill's name. Written by the fetch that returns the skill.
pub(crate) const SKILL_READ_PREFIX: &str = "skill-read:";

/// **The ledger domain of a skill a session has been offered**, `skill-offered:`
/// and the skill's name. Written by the act that offers it, so the offer is made
/// once per session whatever happens next.
pub(crate) const SKILL_OFFERED_PREFIX: &str = "skill-offered:";

/// **The one place that says which writes are about a project's work**: the
/// kind of the thing written about, or created. `capture` and `add_entity` both
/// ask it, so a narrower ruling is a change to this function and nothing else.
pub(crate) fn is_project_work(kind: Option<EntityKind>) -> bool {
    matches!(kind, Some(EntityKind::PROJECT | EntityKind::WORK))
}

/// **Ships in the binary, one line, exactly as the other teachings do.** It
/// names the skill and what it holds; the procedure is fetched, never sent.
pub(crate) const RHYTHMS_SKILL_TEACHING: &str = "You are checking in on a loop: start_here with \
    skill: \"rhythms\" has what a loop holds, how a check-in closes one three ways, and how to \
    look ahead and back at what falls due.";

/// **The line that offers the `projects` skill**, with [`RHYTHMS_SKILL_TEACHING`].
pub(crate) const PROJECTS_SKILL_TEACHING: &str = "You are writing about a project's work: \
    start_here with skill: \"projects\" has the routes for a date that moves, a decision, an \
    open question and what you worked out from what was said. It also holds the keys that say \
    where a piece of work stands (`status`) and whose it is (`owner`).";

/// **The domain of the line that names the reads a dependency makes useful.**
/// Spent by the first write in a session that sets `depends_on`, whichever verb
/// carries it: a session that writes dependencies and was never shown the reads
/// has nothing to ask them with.
pub(crate) const DEPENDS_ON_READ_DOMAIN: &str = "depends-on-read";

/// **Ships in the binary, exactly as the other teachings do.** It names the
/// three reads and what `ready` checks; the procedure stays in the skill.
pub(crate) const DEPENDS_ON_READ_TEACHING: &str = "This write sets `depends_on`. Three reads \
    answer from it. `recall` with view: \"ready\" and `parent` set to the project lists \
    the work not finished, whose dependencies are all finished and that waits on no one. It leaves \
    out finished and archived work and keeps work already under way, so narrow it with `where` \
    (status = next, say) to list only what has not been started. The same view with \
    `subject` set to one piece of work answers ready: false and `blocked_by`, a list of what \
    stops it. `recall` with `subject` and follow: {\"relation\": \"depends_on\", \"direction\": \
    \"in\"} lists the work that waits on it. A `waiting_on` handle also keeps work out of ready. \
    start_here with skill: \"projects\" has the examples.";

/// **Quoting the operator — taught once, on a session's first post into their
/// box.** The one moment a bot is about to say what the operator said.
pub(crate) const OPERATOR_QUOTE_DOMAIN: &str = "operator-quote";

/// **Ships in the binary, exactly as the other teachings do.**
pub(crate) const OPERATOR_QUOTE_TEACHING: &str = "To quote the operator, write the address of the \
    claim that holds their words, `@kind:slug#fN`, and jojobot writes those words into the \
    message beside the address. The claim must be one they said (testimony); an inference is \
    refused. Quotation marks you type yourself are left alone and prove nothing. A record's \
    content is a claim, not a transcript, so a quote by address does not prove verbatim speech.";

/// **The sixth domain — a creation forced past a near-miss refusal.** Named on
/// the call that makes the deliberate near-duplicate, which is the one moment
/// the repair can be told before it is needed. It rides a creation that was
/// sent with an `override_token`, once per session; an ordinary creation never
/// spends it.
pub(crate) const DUPLICATE_REPAIR_DOMAIN: &str = "duplicate-repair";

/// **Ships in the binary, exactly as the other teachings do.**
pub(crate) const DUPLICATE_REPAIR_TEACHING: &str = "You created this past a near-miss refusal. If \
    it later turns out to be the same thing as one of those, merge_entities folds one into the \
    other: the claims, the edges drawn at it and the history under its keys move to the \
    survivor. A recall of the handle you merge away answers status merged, names the survivor \
    in merged_into and holds nothing itself, so read the survivor instead. archive_entity is \
    not that repair. It takes an entity out of default reads and leaves its claims where they are.";

/// **The fifth domain — archiving a rhythm's claim does not stop its
/// cadence.** Named on the call that actually raises the question: an
/// update_fact that archives a claim on a rhythm AND the rhythm still shows
/// a due moment afterward. Archiving most things has nothing to do with a
/// schedule, and archiving a rhythm claim that was never carrying an open
/// one leaves nothing standing to warn about — the gate checks the
/// aftermath rather than the act, so neither spends the one teaching.
pub(crate) const RHYTHM_ARCHIVE_DOMAIN: &str = "rhythm-archive";

/// **Ships in the binary, exactly as the other teachings do.** A thing's
/// fields are its writes, the newest write of each key winning, from
/// whichever active record made it — archiving one claim about a rhythm
/// takes only that record's own writes out of the running, and a schedule
/// key last written on a different, still-active record keeps counting.
pub(crate) const RHYTHM_ARCHIVE_TEACHING: &str = "Archiving a claim does not stop a rhythm's \
    cadence. The schedule is keys set on the loop — cadence_days, counts_from — and a thing's fields are \
    the writes that set them, the newest write of each key winning, from whichever active \
    record made it. \
    Archiving one claim takes only that record's own writes out of the running; a schedule key \
    last written on a different, still-active record keeps counting. To silence it, clear \
    counts_from with update_fact: an overdue read leaves the rhythm out as not_yet_opened and nothing computes a next due \
    date, while the loop itself, its name and its history stay exactly as they were. Archiving \
    is still the right move when a claim about the rhythm was wrong or is no longer worth \
    keeping — it is simply not what stops the cadence.";

/// **The narrower half of [`RHYTHM_HISTORY_DOMAIN`], for the site a caller
/// reaches when the fuller text never landed first.** A check-in that also
/// sends `happened_at`, and the two disagree, raises the same question the
/// fuller text already answers for whoever met it at rhythm-creation — this
/// is what a session gets when a check-in is its FIRST contact with the
/// domain instead: a resumed session, one that never created the rhythm
/// itself, or a rhythm old enough to predate the fuller text. A check-in
/// with no `happened_at` at all has asked nothing; one where both name the
/// same day already got it right, whichever field the caller believed was
/// doing the work — neither spends this domain's one row.
pub(crate) const CHECK_IN_DATE_TEACHING: &str = "A check-in's schedule is dated by recorded_at, \
    never by happened_at — happened_at never reaches the scheduling engine at all. This check-in \
    named both, for different days: to record a turn that happened on an earlier day, send that \
    day as recorded_at.";

/// **The domain of the sentence that says a session's two ids are one run.**
/// Spent by the first journal or amend receipt a session gets, whichever
/// comes first. A receipt names the stored run as `session` while every call
/// carries the `sid`, and a caller who reads two ids cannot tell which one a
/// handover should quote.
pub(crate) const SESSION_ID_DOMAIN: &str = "session-id";

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

/// **A field key that shadows one of the same verb's own arguments** — a
/// caller who meant `update_fact` with `status: archived` and instead sent
/// `fields: {status: "archived"}` has written data, not the write they meant.
/// Named on the call that raises the question: `capture` or `update_fact`
/// carrying a field key that is also one of that verb's own argument names.
pub(crate) const FIELD_SHADOWS_ARGUMENT_DOMAIN: &str = "field-shadows-argument";

/// **The sentence for [`FIELD_SHADOWS_ARGUMENT_DOMAIN`]**, naming the actual
/// key and verb rather than a generic warning — a caller who sees their own
/// word reflected back trusts the answer; one told about "a field" has to
/// work out which one meant them.
pub(crate) fn field_shadows_argument_teaching(key: &str, verb: &str) -> String {
    format!(
        "The field \"{key}\" is stored as ordinary data — jojobot never reads a record's fields \
         back into how it treats the claim itself. It changes nothing: {verb}'s own {key} \
         argument is what does that. Call {verb} again naming {key} as an argument, not a field, \
         to change it."
    )
}

tokio::task_local! {
    /// **The domains a bulk capture has found untaught and not yet spent.** Every item
    /// of a batch is its own capture, and a capture spends a once-per-session domain
    /// when it tells a note. Inside a batch the spend is held back: a domain not in
    /// the ledger is told the first time an item reaches it and the batch remembers
    /// it, and the batch writes the rows only when it delivers its receipt. A batch
    /// that stops partway writes none, so it spends nothing it did not deliver.
    pub(crate) static BATCH_SPENDS: std::sync::Arc<std::sync::Mutex<Vec<String>>>;
}

/// **A ledger key made of a fixed prefix and a part a caller's words decide**, kept
/// within the width of the store's `domain` column. A key that fits is the prefix and
/// the part as they are, so a session already taught keeps its row. A longer one is the
/// prefix and a short stable digest of the part, because the store refuses a domain over
/// its width and [`Jojobot::first_contact`] answers `false` for a refusal, so a wide key
/// would silently never teach. Every key built from a caller's words comes through here.
pub(crate) fn ledger_key(prefix: &str, part: &str) -> String {
    use sha2::{Digest, Sha256};
    let whole = format!("{prefix}{part}");
    if whole.chars().count() <= jojobot_domain::teaching::DOMAIN_WIDTH {
        return whole;
    }
    let digest: String = Sha256::digest(part.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{prefix}~{digest}")
}

/// **The domain of the line that names `where` on a read of a whole kind**, one for each
/// kind. Spent by the first such read of that kind in a session that has enough objects
/// and a key to build an example from, so a session is taught once for each kind it reads
/// whole and the example always comes from the kind in front of it. A read that narrows,
/// or that has too few objects, spends nothing.
pub(crate) fn where_on_a_broad_read_domain(kind: &str) -> String {
    ledger_key("where-on-a-broad-read:", kind)
}

/// **The fewest objects a read of a whole kind must return to be taught `where`.**
/// Under six, reading them all is as cheap as asking and the teaching would be noise.
pub(crate) const WHERE_ON_A_BROAD_READ_MIN: usize = 6;

/// **Whether a `recall` reads a whole kind**: it names a kind and nothing that narrows
/// it or asks something else. Judged on the arguments as the caller sent them, before a
/// view fills any in.
pub(crate) fn is_a_whole_read(args: &crate::memory::RecallArgs) -> bool {
    args.kind.is_some()
        && args.expression.is_none()
        && args.fields.is_none()
        && args.subject.is_none()
        && args.parent.is_none()
        && args.view.is_none()
        && args.records.is_none()
        && args.answers_type.is_none()
        && args.overdue.is_none()
        && args.unreviewed.is_none()
        && args.near.is_none()
        && args.follow.is_none()
        && args.built_on.is_none()
        && args.history_record.is_none()
        && args.values.is_none()
}

/// **A key and a value to build the example from**, out of the objects the read just
/// returned. The key is held by more than half of them, by at least two values, and one
/// value is shared by two or more, so `key = value` asks a real question of this answer.
/// The most held key wins, then the one with more values, then the name; the value is
/// the commonest, then the first by name. A value with a space, a quote or more than 40
/// characters cannot be written bare, so it is not offered. `None` when no key fits, so
/// nothing is taught that the answer cannot back.
pub(crate) fn where_example(objects: &[serde_json::Value]) -> Option<(String, String)> {
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for object in objects {
        let Some(fields) = object["fields"].as_object() else {
            continue;
        };
        for (key, value) in fields {
            let Some(value) = value.as_str() else {
                continue;
            };
            let bare = !value.is_empty()
                && value.len() <= 40
                && value
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'));
            if bare {
                *seen.entry(key).or_default().entry(value).or_default() += 1;
            }
        }
    }
    seen.into_iter()
        .filter(|(_, values)| {
            let holders: usize = values.values().sum();
            holders * 2 > objects.len()
                && values.len() >= 2
                && values.values().any(|count| *count >= 2)
        })
        .max_by(|(a_key, a), (b_key, b)| {
            let held = |values: &BTreeMap<&str, usize>| values.values().sum::<usize>();
            held(a)
                .cmp(&held(b))
                .then(a.len().cmp(&b.len()))
                .then(b_key.cmp(a_key))
        })
        .and_then(|(key, values)| {
            values
                .into_iter()
                .max_by(|(a_value, a), (b_value, b)| a.cmp(b).then(b_value.cmp(a_value)))
                .map(|(value, _)| (key.to_string(), value.to_string()))
        })
}

/// **The sentence for [`where_on_a_broad_read_domain`]**, naming the kind the session just
/// read, how many it got back and an example from their own keys, so it is a line about
/// this answer and not about a language.
pub(crate) fn where_on_a_broad_read_teaching(
    kind: &str,
    count: usize,
    key: &str,
    value: &str,
) -> String {
    format!(
        "This read returned {count} objects of kind {kind}. To ask for only some of them, send one \
         expression in `where` over their keys: recall with kind: \"{kind}\" and where: \
         \"{key} = {value}\" returns the ones that match, and `&&`, `||`, `!=` and \
         `in (…)` combine more."
    )
}

/// **An empty read of a type the software ships** — a session that read,
/// worked something out and keeps it in prose has not written it down. Named on
/// the call that raises the question: the first `answers_type` read in a session
/// that selects nothing under a shipped type. A read that held something, or a
/// type the caller declared, has no such gap and spends nothing.
pub(crate) const EMPTY_SHIPPED_TYPE_DOMAIN: &str = "empty-shipped-type";

/// **The sentence for [`EMPTY_SHIPPED_TYPE_DOMAIN`]**, naming the type so a
/// caller sees the one it asked about reflected back.
pub(crate) fn empty_shipped_type_teaching(name: &str) -> String {
    format!(
        "No record answers the type {name} yet, and reading again will not find one. To keep \
         what you just worked out, capture it with the keys of {name} listed in type_keys, in \
         `sets`, with the provenance that is true of it. A record that holds only the keys you \
         know is kept. It is a partial record: answers_type finds it, and the answer names the \
         keys it lacks. `required: true` in type_keys marks a key the type counts \
         as its floor, and it never means required to write. fits_type keeps only a thing that \
         holds every key in type_keys, optional ones too."
    )
}

/// **Which field key, if any, shadows one of `properties`' own names.** Pure
/// over an already-extracted property set, so the RULE — the first field key
/// that is also a property name — is tested without a live tool registry: see
/// [`published_arguments`] for where a real schema's properties come from.
pub(crate) fn shadowed_argument<'a>(
    sent: impl IntoIterator<Item = &'a str>,
    properties: &serde_json::Map<String, serde_json::Value>,
) -> Option<&'a str> {
    sent.into_iter().find(|key| properties.contains_key(*key))
}

/// **The sent field keys the subject's kind does NOT declare.** A key the kind
/// declares is that kind's own column — `status` on a piece of work — so
/// writing it as a field is the write the caller meant, and offering the
/// claim's own `status` argument for it would send them to the wrong one.
pub(crate) fn keys_the_kind_leaves_undeclared(
    kind: Option<EntityKind>,
    sent: &[String],
) -> Vec<&str> {
    let declared: Vec<String> = kind
        .map(|kind| jojobot_domain::memory::kinds::keys_of(kind.as_token()))
        .unwrap_or_default()
        .into_iter()
        .map(|field| field.key)
        .collect();
    sent.iter()
        .map(String::as_str)
        .filter(|key| !declared.iter().any(|d| d == key))
        .collect()
}

/// **Which verb a shadowed key's argument actually belongs to — the
/// current call's own, or the OTHER verb's when a caller has moved a key
/// across from one to the other.** Checked against `current`'s own
/// properties first: a key that verb already has needs no cross-reference,
/// so a `capture` carrying `provenance` is told about `capture`. Only when
/// `current` does not have it does `other`'s set answer, so a `capture`
/// carrying `status` — real record state, but not one of capture's own
/// arguments — is told about `update_fact`, and an `update_fact` carrying
/// `check_in` is told about `capture`.
pub(crate) fn shadowed_argument_verb<'a>(
    sent: impl IntoIterator<Item = &'a str> + Clone,
    current: &str,
    current_properties: &serde_json::Map<String, serde_json::Value>,
    other: &str,
    other_properties: &serde_json::Map<String, serde_json::Value>,
) -> Option<(&'a str, String)> {
    if let Some(key) = shadowed_argument(sent.clone(), current_properties) {
        return Some((key, current.to_string()));
    }
    shadowed_argument(sent, other_properties).map(|key| (key, other.to_string()))
}

/// **The argument names `verb` actually publishes**, read from the tool
/// router rather than a list this file maintains — the schema a client sees
/// is the schema this reads, so a new argument on either verb is covered the
/// moment it exists. `None` when `verb` names no tool, or its schema carries
/// no `properties` object, neither of which is true of `capture` or
/// `update_fact`.
pub(crate) fn published_arguments(
    verb: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let tools = Jojobot::tool_router().list_all();
    let tool = tools.iter().find(|t| t.name.as_ref() == verb)?;
    let schema = serde_json::to_value(&tool.input_schema).ok()?;
    schema.get("properties")?.as_object().cloned()
}

impl Jojobot {
    /// **Teach the first empty `answers_type` read of a type the software
    /// ships, on whichever verb it arrives.** Both `recall` and `search` call
    /// this, so they share one ledger row: a session taught on one is not
    /// taught again on the other. A type the caller declared teaches nothing,
    /// and nothing is spent unless the note is shown.
    ///
    /// **`only_narrowing` says whether the type was the only thing the read
    /// asked.** The note says no record answers the type, which an answer left
    /// empty by a second narrowing cannot say, so such a read neither teaches
    /// nor spends the row — a later plain empty read still teaches.
    pub(crate) async fn note_empty_answers_type(
        &self,
        body: &mut serde_json::Value,
        declared: &DeclaredType,
        caller: Option<&Caller>,
        only_narrowing: bool,
    ) {
        if !only_narrowing || declared.origin != jojobot_domain::memory::types::Origin::Shipped {
            return;
        }
        if self.first_contact(EMPTY_SHIPPED_TYPE_DOMAIN, caller).await {
            crate::answer::note_teaching(body, &empty_shipped_type_teaching(&declared.name));
        }
    }

    /// **Name the reads a dependency makes useful, on the first write in a
    /// session that sets `depends_on`.** Gated on the write setting the key on a
    /// project's work, never on the verb: a write that sets other keys spends
    /// nothing, and the four verbs that take `sets` share one ledger row.
    pub(crate) async fn note_depends_on_read(
        &self,
        fact: &Fact,
        caller: &Caller,
        body: &mut serde_json::Value,
    ) {
        let key = jojobot_domain::memory::readiness::DEPENDS_ON;
        let sets_dependencies = fact.sets.contains(key)
            && fact
                .fields
                .get(key)
                .is_some_and(|held| !held.trim().is_empty());
        if is_project_work(fact.subject.kind())
            && sets_dependencies
            && self
                .first_contact(DEPENDS_ON_READ_DOMAIN, Some(caller))
                .await
        {
            crate::answer::note_teaching(body, DEPENDS_ON_READ_TEACHING);
        }
    }

    /// **Offer the skills that cover this act, to a session that has read none
    /// of them and been offered none of them.** One line per skill, once per
    /// session: the offer is written to the ledger before the answer carries it,
    /// so it is never made twice whatever the session does next. The act is
    /// never refused or delayed by this; a ledger that cannot answer offers
    /// nothing.
    pub(crate) async fn offer_skills_for(
        &self,
        act: crate::orientation::skills::Act,
        caller: Option<&Caller>,
        body: &mut serde_json::Value,
    ) {
        let Some(caller) = caller else {
            return;
        };
        for skill in self.skills {
            if !crate::orientation::skills::is_listed_for(skill, Some(&caller.bot)) {
                continue;
            }
            let Some((_, line)) = skill.covers.iter().find(|(covered, _)| *covered == act) else {
                continue;
            };
            let read = format!("{SKILL_READ_PREFIX}{}", skill.name);
            match self.teachings.has_contact(&caller.sid, &read).await {
                Ok(false) => {}
                Ok(true) => continue,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        skill = skill.name,
                        "whether a skill was read could not be asked — nothing was offered"
                    );
                    continue;
                }
            }
            let offered = format!("{SKILL_OFFERED_PREFIX}{}", skill.name);
            if self.first_contact(&offered, Some(caller)).await {
                crate::answer::note_teaching(body, line);
            }
        }
    }

    /// Whether this call is the first time `domain` has reached this
    /// session's handle.
    ///
    /// `false` for an anonymous caller — there is no handle to remember it
    /// against, so there is nobody to teach. A store failure is silent and
    /// answers `false` too, for the reason [`Jojobot::beat`] is silent on
    /// one: a teaching that could not be recorded must not fail the verb it
    /// rides on, and answering `true` on a write that did not land would
    /// teach nothing while believing it had.
    pub(crate) async fn first_contact(&self, domain: &str, caller: Option<&Caller>) -> bool {
        let Some(caller) = caller else {
            return false;
        };
        // **A batch holds its spends back until it delivers**: the ledger row is
        // not written here, and a domain the batch already found untaught is told
        // once. See [`BATCH_SPENDS`].
        if let Ok(held) = BATCH_SPENDS.try_with(std::sync::Arc::clone) {
            if !matches!(
                self.teachings.has_contact(&caller.sid, domain).await,
                Ok(false)
            ) {
                return false;
            }
            let mut found = held.lock().expect("a batch's spends are poisoned");
            if found.iter().any(|spent| spent == domain) {
                return false;
            }
            found.push(domain.to_string());
            return true;
        }
        match self
            .teachings
            .first_contact(&caller.sid, domain, self.clock().now())
            .await
        {
            Ok(first) => first,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    domain,
                    "a teaching could not be recorded — the verb it rides on still succeeded"
                );
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::session::testing::resumed;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::teaching::testing::InMemoryTeachings;
    use rmcp::handler::server::wrapper::Parameters;

    /// **A key that fits is the prefix and the part as they are**, so a session
    /// already taught keeps its row, and **a key that does not fit is the prefix and a
    /// short stable digest of the part, within the column**. One case pins the spelling
    /// of both, because the key is stored and nothing outside this process declares it.
    #[test]
    fn a_ledger_key_is_the_words_when_they_fit_and_a_digest_when_they_do_not() {
        assert_eq!(
            where_on_a_broad_read_domain("work"),
            "where-on-a-broad-read:work"
        );
        let long = "pet-vaccination:administered_on,next_due_on,veterinarian_name_and_clinic";
        let key = ledger_key("type-unlocks:", long);
        assert_eq!(key, "type-unlocks:~f116c650cf949aa9");
        assert!(key.chars().count() <= jojobot_domain::teaching::DOMAIN_WIDTH);
        // Stable, and two long parts that share their first characters differ.
        assert_eq!(key, ledger_key("type-unlocks:", long));
        let other = format!("{}x", &long[..long.len() - 1]);
        assert_ne!(key, ledger_key("type-unlocks:", &other));
        // A padded kind stays within the column and is told apart from a short one.
        let padded = where_on_a_broad_read_domain(&"k".repeat(80));
        assert!(padded.chars().count() <= jojobot_domain::teaching::DOMAIN_WIDTH);
        assert_ne!(padded, where_on_a_broad_read_domain("k"));
    }

    /// **An act offers a skill scoped to one bot to that bot alone.** The same
    /// write about a piece of work, by a session of each bot, carries the offer
    /// for `bot:gamma` and nothing for `bot:delta`.
    #[tokio::test]
    async fn an_act_offers_a_scoped_skill_to_its_bot_alone() {
        use crate::orientation::skills::{PROBE_OFFER, PROBES};
        let jojobot = handler().with_skills(PROBES);
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let offered = |answer: &serde_json::Value| {
            answer["teaching"]
                .as_array()
                .is_some_and(|lines| lines.contains(&serde_json::json!(PROBE_OFFER)))
        };

        let gamma = booted(&jojobot, "gamma").await;
        let delta = booted(&jojobot, "delta").await;
        let for_delta = capture_as(
            &jojobot,
            &delta,
            capture_args("work:sigma", "the piece starts"),
        )
        .await;
        assert!(
            !offered(&for_delta),
            "another bot's session is not offered the scoped skill: {for_delta}"
        );
        let for_gamma = capture_as(
            &jojobot,
            &gamma,
            capture_args("work:sigma", "the piece goes on"),
        )
        .await;
        assert!(
            offered(&for_gamma),
            "the bot the skill is scoped to is offered it: {for_gamma}"
        );
    }

    /// **Both halves in one case.** The first claim a session ever captures
    /// carries the teaching; the same session capturing a second one does
    /// not. Asserted against the constant `capture` actually sent, not a
    /// second copy of the prose — a duplicate would drift the moment the
    /// wording improved.
    #[tokio::test]
    async fn the_first_capture_of_a_session_is_taught_and_the_second_is_not() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIMS_TEACHING)),
            "the first claim this session ever wrote carries the teaching: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "the same session touching claims again is not taught twice: {second}"
        );
    }

    /// **The trigger is an edge, not a capture.** A claim naming no other
    /// entity has no side to get wrong, so the session's first plain capture
    /// must not spend this domain's one teaching. The first capture that
    /// DOES draw an edge is the one that raises the question, whichever
    /// number capture it is — and the same session drawing a second edge is
    /// not taught twice.
    #[tokio::test]
    async fn only_a_capture_that_draws_an_edge_teaches_claim_direction_and_only_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let plain = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            !plain
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "a claim with no edge has no side to get wrong: {plain}"
        );

        let with_edge = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:globex".into()),
                ..capture_args("alpha", "rides with the club")
            },
        )
        .await;
        assert!(
            with_edge["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the first edge this session drew must carry the direction teaching: {with_edge}"
        );

        let second_edge = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:mr-plow".into()),
                ..capture_args("alpha", "also sponsors mr plow")
            },
        )
        .await;
        assert!(
            !second_edge
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the same session drawing a second edge is not taught twice: {second_edge}"
        );
    }

    /// **The same convention, reached through the verb that EDITS a claim.**
    /// `update_fact` can also draw or replace an edge (`shape` with
    /// `object`), and the question is identical: which side does the claim
    /// belong on. The first such edit this session makes teaches it; the
    /// second does not.
    #[tokio::test]
    async fn update_fact_drawing_an_edge_teaches_claim_direction_and_only_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&first);

        let drawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:globex".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the first edge this session drew, through update_fact, must carry the direction \
             teaching: {drawn}"
        );

        let redrawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:mr-plow".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !redrawn
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the same session replacing an edge a second time is not taught twice: {redrawn}"
        );
    }

    /// **The trigger is what THIS write sends, not what the fact ends up
    /// carrying.** Unlike a fresh capture, an edited fact's edge can already
    /// be set before this call runs, by an edit that named neither `shape`
    /// nor `object` and so left it exactly as it was. Gating on the
    /// resulting fact's edge would spend the teaching on a call that never
    /// raised the question; gating on the write itself does not.
    #[tokio::test]
    async fn an_edit_that_leaves_the_edge_alone_does_not_teach_claim_direction() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        // A different session draws the edge, so THIS session's own contact
        // with the domain has not happened yet.
        let edged = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:globex".into()),
                ..capture_args("beta", "rides with the club")
            },
        )
        .await;
        let address = address_of(&edged);

        let content_only = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    provenance: Some("inference".to_string()),
                    sid: Some(sid.clone()),
                    content: Some("rides with the club every week".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !content_only
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the fact already carried an edge, but this edit named neither shape nor object: \
             {content_only}"
        );

        // The domain is still unspent for this session: a genuine
        // edge-drawing write still teaches.
        let drawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:mr-plow".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the content-only edit must not have spent this session's one teaching: {drawn}"
        );
    }

    /// **The one-teaching-per-session property holds ACROSS the two verbs,
    /// not once per verb.** The domain is keyed by session and name alone;
    /// whichever verb a session meets it through first spends it.
    #[tokio::test]
    async fn claim_direction_is_spent_once_across_update_fact_and_capture() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&first);

        let drawn_by_update = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:globex".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn_by_update["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "update_fact must be able to spend the domain as the session's first edge: \
             {drawn_by_update}"
        );

        let drawn_by_capture = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:mr-plow".into()),
                ..capture_args("alpha", "also sponsors mr plow")
            },
        )
        .await;
        assert!(
            !drawn_by_capture
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "capture must not re-teach a domain update_fact already spent for this session: \
             {drawn_by_capture}"
        );
    }

    /// **The gate is the aftermath, not the act.** A rhythm's schedule is
    /// fields, each key the newest write of it from whichever record made
    /// that write, so archiving one claim about a rhythm leaves a schedule
    /// opened on a DIFFERENT, still-active record exactly as it was — the
    /// paid-run failure this teaching answers. The first such archive this
    /// session makes teaches it; a second does not.
    #[tokio::test]
    async fn archiving_a_rhythm_claim_that_leaves_the_schedule_running_teaches_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "descale", "descale")
            }))
            .await
            .expect("add ok");

        // Opens the schedule, on its own record.
        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                sets: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("rhythm:descale", "we should descale this regularly")
            },
        )
        .await;

        // A SEPARATE claim on the same rhythm — the one this session archives.
        let aside = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:descale", "the filter is due too"),
        )
        .await;
        let address = address_of(&aside);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    details: Some("no longer relevant".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            archived["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING)),
            "the schedule is still standing on another record, and archiving this one did not \
             touch it: {archived}"
        );

        let aside_two = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:descale", "the seal is worn"),
        )
        .await;
        let address_two = address_of(&aside_two);
        let archived_again = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address_two)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived_again
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "the same session archiving a second claim on the same rhythm is not taught twice: \
             {archived_again}"
        );
    }

    /// **A rhythm that never opened has no schedule to leave standing.**
    /// Archiving its only claim raises no question this teaching answers.
    #[tokio::test]
    async fn archiving_a_rhythm_claim_with_no_open_schedule_does_not_teach() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "water-the-fern", "water the fern")
            }))
            .await
            .expect("add ok");

        let plain = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:water-the-fern", "the fern likes a weekly soak"),
        )
        .await;
        let address = address_of(&plain);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "this rhythm never had an open schedule, so nothing was left standing: {archived}"
        );
    }

    /// **Most archivals have nothing to do with a schedule.** Archiving an
    /// ordinary entity's claim never touches the rhythm-archive domain,
    /// whatever else is true of the session.
    #[tokio::test]
    async fn archiving_an_ordinary_entitys_claim_never_teaches_rhythm_archive() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let plain = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&plain);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "alpha is a person, not a rhythm: {archived}"
        );
    }

    /// **The kind check is load-bearing, not decorative.** A cadence is
    /// fields, and fields are not restricted to a rhythm — a thing answers
    /// whatever keys it carries whether or not anybody declared a type for
    /// them, and `carriers()` names no kind either. So a non-rhythm thing
    /// holding a due moment is not a hypothetical this product forbids; it
    /// is exactly what the model permits, and the teaching's own words —
    /// "a rhythm's cadence" — would be wrong about it.
    #[tokio::test]
    async fn archiving_a_non_rhythm_things_claim_with_a_due_moment_does_not_teach_rhythm_archive() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;

        // Opens a schedule directly on an ordinary thing — not a rhythm.
        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                sets: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("thing:kettle", "descale it regularly")
            },
        )
        .await;

        // The premise the rest of the case depends on: the due moment is
        // actually there before anything gets archived.
        let opened = json_of(
            &jojobot
                .recall(Parameters(recall_args("thing:kettle")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            opened["objects"][0]["fields"]["due_on"], "2026-08-08",
            "the fixture has to actually carry an open schedule before archiving proves \
             anything: {opened}"
        );

        let aside = capture_as(
            &jojobot,
            &sid,
            capture_args("thing:kettle", "the handle is loose"),
        )
        .await;
        let address = address_of(&aside);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "thing:kettle is not a rhythm, and the teaching's own words are about a rhythm's \
             cadence: {archived}"
        );
    }

    /// A rhythm ready to take a check-in, with no basis of its own yet —
    /// the same shape `capture`'s own version of this fixture builds,
    /// inlined here because that one is private to `capture`'s test module.
    async fn a_cadenced_rhythm_ready_for_a_check_in(jojobot: &Jojobot, handle: &str) {
        ensure(jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", handle, handle)
            }))
            .await
            .expect("add ok");
        capture_ok(
            jojobot,
            CaptureArgs {
                sets: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("rhythm:{handle}"), "every week")
            },
        )
        .await;
    }

    /// **The gate is disagreement, not presence alone.** A check-in whose
    /// `happened_at` names a day OTHER than the one dating its schedule has
    /// raised the exact question this teaches: `recorded_at`, not
    /// `happened_at`, is what a rhythm's cadence counts from, and the two
    /// having different answers means whichever one the caller trusted is
    /// wrong about what actually happened to the schedule.
    #[tokio::test]
    async fn a_check_in_whose_happened_at_disagrees_with_recorded_at_teaches_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let mistaken = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-10".into()),
                ..capture_args("rhythm:descale", "did it a few days before logging it")
            },
        )
        .await;
        assert!(
            mistaken["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CHECK_IN_DATE_TEACHING)),
            "recorded_at and happened_at name different days on a check-in: {mistaken}"
        );

        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "water-the-fern").await;
        let mistaken_again = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-20".into()),
                happened_at: Some("2026-06-18".into()),
                ..capture_args("rhythm:water-the-fern", "same mix-up, a different loop")
            },
        )
        .await;
        assert!(
            !mistaken_again
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "the same session hitting the same mix-up again is not taught twice: {mistaken_again}"
        );
    }

    /// **A check-in that never mentions `happened_at` has asked nothing.**
    /// Most check-ins log the day they happened through `recorded_at` alone
    /// and never touch the inert field, so this is the common case and it
    /// must not spend the session's one teaching.
    #[tokio::test]
    async fn a_check_in_with_no_happened_at_does_not_teach_check_in_date() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let ordinary = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args("rhythm:descale", "logged it the same day")
            },
        )
        .await;
        assert!(
            !ordinary
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "this check-in never named happened_at, so nothing disagreed with anything: \
             {ordinary}"
        );
    }

    /// **The two fields naming the SAME day already got it right.** Whether
    /// the caller believes `recorded_at` or `happened_at` is doing the
    /// work, the schedule lands on the correct date either way, so there is
    /// nothing to warn about.
    #[tokio::test]
    async fn a_check_in_whose_happened_at_matches_recorded_at_does_not_teach_check_in_date() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let agreeing = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-14".into()),
                ..capture_args(
                    "rhythm:descale",
                    "logged it the same day, and said so twice",
                )
            },
        )
        .await;
        assert!(
            !agreeing
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "recorded_at and happened_at name the same day, so the schedule is already right: \
             {agreeing}"
        );
    }

    /// **The case nobody had.** `RHYTHM_HISTORY_TEACHING` (creating a
    /// rhythm) and `CHECK_IN_DATE_TEACHING` (a mismatched check-in) used to
    /// be two domains stating the same underlying fact — that a check-in's
    /// own `recorded_at` is what a rhythm's schedule is built from — one
    /// implicitly, one by field name. A session that creates a loop and
    /// then backdates its first check-in with the wrong field, the exact
    /// workflow this whole day's work was about, met both. They now share
    /// one domain, so this is no longer a property of the wording to keep
    /// in sync by hand: whichever site reaches the session first spends the
    /// row, and the other notes nothing.
    #[tokio::test]
    async fn a_session_that_creates_a_rhythm_then_mis_dates_its_check_in_is_told_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;

        let created = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    parent: Some("thing:kettle".into()),
                    ..add_args("rhythm", "descale", "descale")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            created["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_HISTORY_TEACHING)),
            "creating the rhythm is this session's first contact with the shared domain: {created}"
        );

        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                sets: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("rhythm:descale", "every week")
            },
        )
        .await;
        let checked_in = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-10".into()),
                ..capture_args("rhythm:descale", "did it a few days before logging it")
            },
        )
        .await;
        assert!(
            !checked_in
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "rhythm-history already told this session the date basis — the mismatched check-in \
             must not tell it again: {checked_in}"
        );
    }

    /// **The trigger is a claim reaching the session, not the call.** A
    /// search that matches nothing has not touched the domain, so it must
    /// not spend the session's one teaching on an empty answer.
    #[tokio::test]
    async fn a_search_with_no_fact_hit_does_not_teach() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let empty = jojobot
            .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                query: Some("nothing here matches anything".into()),
                sid: Some(sid.clone()),
                ..search_args()
            }))
            .await
            .expect("search ok");
        let empty = json_of(&empty);
        assert_eq!(empty["count"], 0, "nothing matched: {empty}");
        assert!(
            empty.get("teaching").is_none(),
            "an empty search never touched claims: {empty}"
        );
    }

    /// **A search that surfaces a claim carries the claims teaching, once.** The
    /// teaching has to reach a session before it decides how to write, and the
    /// read that first shows it a claim is that moment. The slot is spent there,
    /// so the second search carries nothing and the capture that follows does not
    /// repeat it.
    #[tokio::test]
    async fn a_search_that_surfaces_a_fact_carries_the_claims_teaching_once() {
        use jojobot_domain::memory::search::Hit;
        use jojobot_domain::memory::{Fact, FactId, FactStatus, Provenance, Standing};

        let fact = Fact {
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            reaching: Default::default(),
            id: FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "plays go".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
            receipt: None,
        };
        let hit = Hit::Fact {
            fact: Box::new(fact),
            subject: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            home: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            source: None,
            superseded_by: None,
        };
        let spy = Arc::new(SpySearch::answering(vec![hit]));
        let jojobot = handler_with(spy);
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                    query: Some("plays go".into()),
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert!(
            first["teaching"]
                .as_array()
                .is_some_and(|taught| taught.contains(&serde_json::json!(CLAIMS_TEACHING))),
            "the search that surfaces a claim carries the claims teaching: {first}"
        );

        let second = json_of(
            &jojobot
                .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                    query: Some("plays go".into()),
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert!(
            second.get("teaching").is_none(),
            "nor does the second read: {second}"
        );

        // The write the teaching is about does not repeat it: the read spent the
        // slot, and the ledger is one per session whichever verb touched claims first.
        let written = capture_as(&jojobot, &sid, capture_args("alpha", "plays go too")).await;
        assert!(
            written
                .get("teaching")
                .and_then(|taught| taught.as_array())
                .is_none_or(|taught| !taught.contains(&serde_json::json!(CLAIMS_TEACHING))),
            "the capture after the read does not carry the claims teaching again: {written}"
        );
    }

    /// **`recall` with facts carries the claims teaching, once.** A recall that
    /// asked for no facts surfaced no claim and spends nothing; the first that
    /// returns one carries the teaching, and neither the next recall nor the
    /// capture after it does.
    #[tokio::test]
    async fn recall_with_facts_carries_the_claims_teaching_once() {
        // **A fact seeded straight through the store**, not through
        // `capture` — the MCP verb would consume the session's one teaching
        // itself, leaving nothing for this case to observe.
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                EntityId::person("person:alpha"),
                "plays go",
                jiff::civil::date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let without_facts = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                facts: None,
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let without_facts = json_of(&without_facts);
        assert!(
            without_facts.get("teaching").is_none(),
            "a recall that did not ask for facts never touched claims: {without_facts}"
        );

        let with_facts = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let with_facts = json_of(&with_facts);
        assert!(
            with_facts["teaching"]
                .as_array()
                .is_some_and(|taught| taught.contains(&serde_json::json!(CLAIMS_TEACHING))),
            "a recall that returns a claim carries the teaching: {with_facts}"
        );

        let again = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let again = json_of(&again);
        assert!(
            again.get("teaching").is_none(),
            "nor does the second recall: {again}"
        );

        // The read spent the slot: the capture after it is not taught again.
        let written = capture_as(&jojobot, &sid, capture_args("alpha", "plays go too")).await;
        assert!(
            written
                .get("teaching")
                .and_then(|taught| taught.as_array())
                .is_none_or(|taught| !taught.contains(&serde_json::json!(CLAIMS_TEACHING))),
            "the capture after the read does not carry the claims teaching again: {written}"
        );
    }

    /// **A run picked up by its name is taught again, and once.** The pick-up
    /// hands the run to a different agent, which was not taught what the first
    /// one was, so the ledger of the run starts empty. After that the run is
    /// taught once like any session.
    #[tokio::test]
    async fn a_run_picked_up_by_its_name_is_taught_again_and_once() {
        let memory = Arc::new(InMemoryMemory::booted());
        let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let sessions = Arc::new(InMemorySessions::new());
        let teachings = Arc::new(InMemoryTeachings::new());
        let registry = Arc::new(sid::SessionRegistry::new());

        let first = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            mailboxes.clone(),
            sessions.clone(),
            teachings.clone(),
            registry.clone(),
        );
        make_bot(&first, "gamma").await;
        let opened = booted(&first, "gamma").await;
        let taught = capture_as(&first, &opened, capture_args("alpha", "plays go")).await;
        assert!(
            taught["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIMS_TEACHING)),
            "taught once: {taught}"
        );

        // A second connection over the same stores and the same registry —
        // what a reconnect is. `resumed` answers the choice this boot is
        // offered rather than starting a fresh session.
        let second = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            mailboxes,
            sessions,
            teachings,
            registry,
        );
        let opened_name = run_name(&first, &opened);
        let picked_up = resumed(&second, "gamma").await;
        assert_eq!(
            run_name(&second, &picked_up),
            opened_name,
            "a reconnect resumes the same run"
        );

        let again = capture_as(
            &second,
            &picked_up,
            capture_args("alpha", "and checkers too"),
        )
        .await;
        assert!(
            again["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIMS_TEACHING)),
            "the agent that took the run over is taught again: {again}"
        );
        // The positive the single teaching depends on: the same key writing
        // again is the same agent, and is not taught a second time.
        let once = capture_as(
            &second,
            &picked_up,
            capture_args("alpha", "and chess as well"),
        )
        .await;
        assert!(
            once.get("teaching").is_none(),
            "taught once after the pick-up, like any session: {once}"
        );
    }

    /// **A second domain, with no change to the mechanism** — the same
    /// method, a different string. This is what proves it generalized;
    /// asserting that it is general would prove nothing.
    #[tokio::test]
    async fn a_second_domain_generalizes_with_no_change_to_the_mechanism() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let caller = jojobot
            .caller(Some(sid.as_str()))
            .expect("a readable handle")
            .expect("a booted caller");

        const A_SECOND_DOMAIN: &str = "a-second-domain";
        assert!(
            jojobot.first_contact(A_SECOND_DOMAIN, Some(&caller)).await,
            "a domain nobody has declared before is a first contact"
        );
        assert!(
            !jojobot.first_contact(A_SECOND_DOMAIN, Some(&caller)).await,
            "the same session touching it again is not"
        );
        assert!(
            jojobot.first_contact(CLAIMS_DOMAIN, Some(&caller)).await,
            "a DIFFERENT domain, same session, is untouched by the first"
        );
    }

    /// **An anonymous caller is never taught** — there is no handle to
    /// remember it against, so there is nobody to teach.
    #[tokio::test]
    async fn an_anonymous_caller_is_never_taught() {
        let jojobot = handler();
        assert!(!jojobot.first_contact(CLAIMS_DOMAIN, None).await);
    }

    /// **The second key names itself, and names no value.** Pinned on the
    /// identifier a caller would need to spell correctly, never on the
    /// surrounding prose — and the operator's rejected fixed vocabulary
    /// (user/feedback/project/reference) must never appear here: naming an
    /// example is the first step toward a list somebody then has to
    /// maintain.
    #[test]
    fn the_purpose_key_is_named_and_no_value_for_it_is() {
        assert!(
            CLAIM_SUBJECT_TEACHING.contains("`purpose`"),
            "the key must be named: {CLAIM_SUBJECT_TEACHING}"
        );
        for rejected in ["user", "feedback", "project", "reference"] {
            assert!(
                !CLAIM_SUBJECT_TEACHING.to_lowercase().contains(rejected),
                "the shipped text must not anchor the open vocabulary on the rejected fixed \
                 set: found {rejected:?} in {CLAIM_SUBJECT_TEACHING}"
            );
        }
    }

    /// **The teaching says recording is correct, not only that it is safe.**
    /// A paid run lost the same phase four times: a sitting met two
    /// contradicting accounts, recalled that a further claim does not erase
    /// the first, and still asked instead of writing — because "does not
    /// destroy" says nothing was destroyed, never that jojobot settles
    /// nothing and both accounts are allowed to stand. Pinned on a
    /// distinctive word, not the sentence around it.
    #[test]
    fn the_claims_teaching_says_a_contradiction_may_stand() {
        assert!(
            CLAIMS_TEACHING.to_lowercase().contains("contradict"),
            "the teaching must say a contradiction is allowed to stand, not only that a write \
             is non-destructive: {CLAIMS_TEACHING}"
        );
    }

    /// **The projects teaching says the skill holds the keys that say where a
    /// piece of work stands and whose it is.** Pinned on the key names a caller
    /// must spell, read off what the engine declares on a work thing, so a
    /// renamed key reddens it rather than leaving a teaching that names a key
    /// nobody reads.
    #[test]
    fn the_projects_teaching_names_the_status_and_ownership_keys() {
        let declared: Vec<String> = jojobot_domain::memory::kinds::keys_of("work")
            .into_iter()
            .map(|f| f.key)
            .collect();
        for key in ["status", "owner"] {
            assert!(
                declared.iter().any(|d| d == key),
                "the engine no longer declares `{key}` on a piece of work"
            );
            assert!(
                PROJECTS_SKILL_TEACHING.contains(&format!("`{key}`")),
                "the projects teaching does not name the work key `{key}`: \
                 {PROJECTS_SKILL_TEACHING}"
            );
        }
    }

    /// **Both halves in one case, for the second domain.** The first capture
    /// carries the subject-convention teaching; the same session capturing
    /// again does not.
    #[tokio::test]
    async fn the_first_capture_teaches_the_subject_convention_and_the_second_does_not() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_SUBJECT_TEACHING)),
            "the first capture carries the subject-convention teaching: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "the same session capturing again is not taught the convention twice: {second}"
        );
    }

    /// ⭐ **The case that makes this a domain rather than a longer paragraph.**
    /// A session taught about claims through `retract` has NOT been taught the
    /// subject convention — the two are independent rows on the same ledger,
    /// not one teaching that happens to render in two places.
    #[tokio::test]
    async fn claims_and_the_subject_convention_are_independently_tracked() {
        let (jojobot, sid) = a_seeded_fact_handler().await;

        // `retract` touches only the claims domain — it never mentions a
        // subject field — so this session is now taught claims and nothing
        // else.
        let retracted = json_of(
            &jojobot
                .retract(rmcp::handler::server::wrapper::Parameters(
                    crate::memory::RetractArgs {
                        request_id: None,
                        address: "person:alpha#f1".into(),
                        reason: Some("never happened".into()),
                        recorded_at: None,
                        sid: Some(sid.clone()),
                    },
                ))
                .await
                .expect("retract ok"),
        );
        assert_eq!(
            retracted["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "retract taught claims, and only claims: {retracted}"
        );

        // The first capture this session ever makes still owes it the
        // subject-convention teaching, because that domain is untouched —
        // and it must NOT re-teach claims, which the retraction already covered.
        let captured = capture_as(&jojobot, &sid, capture_args("alpha", "plays go too")).await;
        assert_eq!(
            captured["teaching"],
            serde_json::json!([CLAIM_SUBJECT_TEACHING]),
            "capture owes only the domain the retraction never touched: {captured}"
        );
    }

    /// ⭐ **The shape `first_contact`'s check-and-set could get wrong if
    /// delivery clobbered.** A session whose very first call is a capture —
    /// no prior search or recall — touches both domains at once, and the
    /// answer must carry both. A second capture must then carry neither: both
    /// rows are spent by the first answer, not just the one that rendered
    /// last.
    #[tokio::test]
    async fn a_first_ever_capture_teaches_both_domains_and_spends_both() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert_eq!(
            first["teaching"],
            serde_json::json!([CLAIMS_TEACHING, CLAIM_SUBJECT_TEACHING]),
            "the first call ever touches both domains at once and both survive: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "both domains were spent by the first answer, not just the one that rendered last: \
             {second}"
        );
    }

    /// A fact seeded straight through the store, bypassing `capture` — which
    /// would consume the session's one teaching itself, leaving nothing for
    /// the case under test to observe.
    async fn a_seeded_fact_handler() -> (Jojobot, String) {
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                EntityId::person("person:alpha"),
                "plays go",
                jiff::civil::date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        (jojobot, sid)
    }

    /// **`update_fact` teaches on the same trigger `capture` does: the write
    /// landing.**
    #[tokio::test]
    async fn update_fact_teaches_on_the_first_edit() {
        let (jojobot, sid) = a_seeded_fact_handler().await;

        let edited = jojobot
            .update_fact(rmcp::handler::server::wrapper::Parameters(
                crate::memory::UpdateFactArgs {
                    provenance: Some("inference".to_string()),
                    content: Some("plays go on weekends".into()),
                    sid: Some(sid),
                    ..update_args("person:alpha#f1")
                },
            ))
            .await
            .expect("update ok");
        let edited = json_of(&edited);
        assert_eq!(
            edited["teaching"],
            serde_json::json!([CLAIMS_TEACHING, CLAIM_SUBJECT_TEACHING]),
            "the first edit this session made touches both domains at once, \
             the same as a first capture does: {edited}"
        );
    }

    /// **`retract` teaches on the write landing.**
    #[tokio::test]
    async fn retract_teaches_on_the_first_retraction() {
        let (jojobot, sid) = a_seeded_fact_handler().await;

        let retracted = jojobot
            .retract(rmcp::handler::server::wrapper::Parameters(
                crate::memory::RetractArgs {
                    request_id: None,
                    address: "person:alpha#f1".into(),
                    reason: Some("never happened".into()),
                    recorded_at: None,
                    sid: Some(sid),
                },
            ))
            .await
            .expect("retract ok");
        let retracted = json_of(&retracted);
        assert_eq!(
            retracted["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first retraction this session made carries the teaching: {retracted}"
        );
    }

    /// **`merge_entities` teaches on the write landing.**
    #[tokio::test]
    async fn merge_entities_teaches_on_the_first_merge() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        ensure(&jojobot, "person:milhouse").await;

        let merged = jojobot
            .merge_entities(rmcp::handler::server::wrapper::Parameters(
                crate::memory::merge_entities::MergeArgs {
                    request_id: None,
                    duplicate: "person:milhouse".into(),
                    survivor: "person:alpha".into(),
                    reason: Some("same person".into()),
                    recorded_at: None,
                    sid: Some(sid),
                },
            ))
            .await
            .expect("merge ok");
        let merged = json_of(&merged);
        assert_eq!(
            merged["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first merge this session made carries the teaching: {merged}"
        );
    }

    /// **`add_entity` teaches the rhythm-history domain, and only for a
    /// rhythm.** The absence has to be proved next to a build that DOES teach
    /// it — a person carrying no teaching passes identically whether the
    /// mechanism is wired or missing entirely, so the positive on a rhythm
    /// rides in the same test. The third call, a second rhythm in the same
    /// session, proves the ledger rather than a constant that always renders.
    #[tokio::test]
    async fn add_entity_teaches_rhythm_history_only_for_a_rhythm_and_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let person = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    ..add_args("person", "alpha", "Alpha")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            person.get("teaching").is_none(),
            "an ordinary entity never touches the rhythm-history domain: {person}"
        );

        let parent = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    ..add_args("thing", "kettle", "Kettle")
                }))
                .await
                .expect("add ok"),
        );
        assert_ne!(parent["status"], "blocked", "{parent}");

        let first = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    parent: Some("thing:kettle".into()),
                    sid: Some(sid.clone()),
                    ..add_args("rhythm", "descale", "Descale")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_HISTORY_TEACHING)),
            "the first rhythm this session creates carries the teaching: {first}"
        );

        let second = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    parent: Some("thing:kettle".into()),
                    sid: Some(sid),
                    ..add_args("rhythm", "water-the-fern", "Water The Fern")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            second.get("teaching").is_none(),
            "the same session creating a second rhythm is not taught again: {second}"
        );
    }

    /// **The rule is general, never a hand-written list** — a synthetic
    /// schema carrying an argument name no real verb has ever published is
    /// caught exactly as `status` or `recorded_at` would be, because
    /// `shadowed_argument` knows nothing about which names are real. This is
    /// what a new argument added to either verb is covered by, without
    /// anybody editing a list here.
    #[test]
    fn shadowed_argument_is_caught_by_shape_never_by_a_maintained_list() {
        let properties: serde_json::Map<String, serde_json::Value> = [(
            "an-argument-nobody-has-published-yet".to_string(),
            serde_json::json!({}),
        )]
        .into_iter()
        .collect();

        assert_eq!(
            shadowed_argument(
                ["content", "an-argument-nobody-has-published-yet"],
                &properties
            ),
            Some("an-argument-nobody-has-published-yet"),
            "a name found in the schema is caught whether or not it is a name anybody wrote down"
        );
        assert_eq!(
            shadowed_argument(["content", "details"], &properties),
            None,
            "a name absent from the schema is not"
        );
    }

    /// **`capture`'s own published schema really does name `status` and
    /// `provenance`** — the pin `published_arguments("capture")` rests on,
    /// so the integration cases below are read against a real property set
    /// rather than trusted to exist.
    #[test]
    fn captures_published_arguments_name_its_own_real_ones() {
        let properties = published_arguments("capture").expect("capture is a published tool");
        for name in ["provenance", "recorded_at", "standing"] {
            assert!(
                properties.contains_key(name),
                "capture's own schema does not name its own argument {name}: {properties:?}"
            );
        }
    }

    /// **The real case: a capture carrying `status`, which is not one of
    /// capture's own arguments — only update_fact's.** Checking capture's
    /// own schema alone misses this; the teaching has to name update_fact,
    /// never capture. Both halves in one case, the same shape
    /// [`the_first_capture_of_a_session_is_taught_and_the_second_is_not`]
    /// already uses: the negative is read against the positive that fires,
    /// never trusted alone.
    #[tokio::test]
    async fn a_capture_carrying_status_is_taught_to_use_update_fact() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let mut args = capture_args("alpha", "plays go");
        args.fields = Some(
            [("status".to_string(), "retired".to_string())]
                .into_iter()
                .collect(),
        );
        let shadowing = capture_as(&jojobot, &sid, args).await;
        let teaching = shadowing["teaching"]
            .as_array()
            .unwrap_or_else(|| panic!("a teaching list: {shadowing}"));
        assert!(
            teaching.iter().any(|t| {
                t.as_str().is_some_and(|t| {
                    t.contains("\"status\"")
                        && t.contains("is stored as ordinary data")
                        && t.contains("update_fact")
                })
            }),
            "a capture carrying status must be taught to use update_fact, not capture: \
             {shadowing}"
        );

        let mut second_args = capture_args("alpha", "also plays chess");
        second_args.fields = Some(
            [("status".to_string(), "retired".to_string())]
                .into_iter()
                .collect(),
        );
        let second = capture_as(&jojobot, &sid, second_args).await;
        assert!(
            !second["teaching"]
                .as_array()
                .map(|t| t.iter().any(|t| t.as_str().is_some_and(
                    |t| t.contains("\"status\"") && t.contains("is stored as ordinary data")
                )))
                .unwrap_or(false),
            "the same session shadowing the same argument again is not taught again: {second}"
        );

        let mut plain_args = capture_args("alpha", "and also poker");
        plain_args.fields = Some(
            [("favourite_suit".to_string(), "spades".to_string())]
                .into_iter()
                .collect(),
        );
        let plain = capture_as(&jojobot, &sid, plain_args).await;
        assert!(
            !plain["teaching"]
                .as_array()
                .map(|t| t.iter().any(|t| t
                    .as_str()
                    .is_some_and(|s| s.contains("is stored as ordinary data"))))
                .unwrap_or(false),
            "a field naming nothing either verb takes as an argument teaches nothing: {plain}"
        );
    }

    /// **The two ids a session meets are one run, and the first beat says so.**
    /// A boot hands back a `sid` and a journal receipt names a `session`, and
    /// nothing said they are the same run or which one to quote. The first
    /// receipt of a session names BOTH real values and says which to quote;
    /// the second beat of that session, and an amendment after it, carry
    /// nothing; a fresh session is told again. Sent through the verbs a caller
    /// uses, and read against the values the boot and the receipt actually
    /// returned.
    #[tokio::test]
    async fn the_first_beat_says_the_sid_and_the_session_are_one_run() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first =
            crate::session::testing::journal_entry(&jojobot, &sid, "set out to build").await;
        let session = first["session"]
            .as_str()
            .unwrap_or_else(|| panic!("a receipt naming its session: {first}"))
            .to_string();
        let name = run_name(&jojobot, &sid);
        assert_ne!(
            session, sid,
            "the case rests on the key and the session being different strings"
        );
        let told = |body: &serde_json::Value| -> bool {
            body["teaching"]
                .as_array()
                .map(|teaching| {
                    teaching.iter().any(|t| {
                        t.as_str()
                            .is_some_and(|t| t.contains(&name) && t.contains(&session))
                    })
                })
                .unwrap_or(false)
        };
        assert!(
            told(&first),
            "the first receipt names the sid and the session and says they are one run: {first}"
        );

        for line in first["teaching"].as_array().into_iter().flatten() {
            let line = line.as_str().expect("a teaching is a string");
            assert!(
                !line.contains("  ") && !line.contains('\n'),
                "a teaching is one line with no run of spaces: {line:?}"
            );
        }

        let second = crate::session::testing::journal_entry(&jojobot, &sid, "found a thing").await;
        assert_eq!(
            second["session"], first["session"],
            "the second beat lands in the same run, so the silence below is not a different run"
        );
        assert!(
            !told(&second),
            "the same session is not told twice: {second}"
        );

        let amended = json_of(
            &jojobot
                .amend_journal(Parameters(crate::session::AmendJournalArgs {
                    entry: "found two things".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("amend ok"),
        );
        assert!(
            !told(&amended),
            "an amendment after the first beat is not told it either: {amended}"
        );

        make_bot(&jojobot, "delta").await;
        let other_sid = booted(&jojobot, "delta").await;
        let other =
            crate::session::testing::journal_entry(&jojobot, &other_sid, "set out to test").await;
        assert!(
            other["teaching"]
                .as_array()
                .map(|teaching| teaching.iter().any(|t| t
                    .as_str()
                    .is_some_and(|t| t.contains(&run_name(&jojobot, &other_sid))
                        && other["session"].as_str().is_some_and(|s| t.contains(s)))))
                .unwrap_or(false),
            "a fresh session is told again, with its own two values: {other}"
        );
    }

    /// **The second shape: a field that shadows the CURRENT verb's own
    /// argument.** No cross-reference is needed — `provenance` is one of
    /// capture's own arguments, so the teaching names capture, exactly as
    /// before the union fix.
    #[tokio::test]
    async fn a_capture_carrying_its_own_argument_is_taught_to_use_capture() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let mut args = capture_args("alpha", "plays go");
        args.fields = Some(
            [("provenance".to_string(), "testimony".to_string())]
                .into_iter()
                .collect(),
        );
        let shadowing = capture_as(&jojobot, &sid, args).await;
        let teaching = shadowing["teaching"]
            .as_array()
            .unwrap_or_else(|| panic!("a teaching list: {shadowing}"));
        assert!(
            teaching.iter().any(|t| {
                t.as_str().is_some_and(|t| {
                    t.contains("\"provenance\"")
                        && t.contains("is stored as ordinary data")
                        && t.contains("capture")
                })
            }),
            "a capture carrying its own argument names capture, not update_fact: {shadowing}"
        );
    }

    /// **A key the subject's kind DECLARES is the caller's column, not a
    /// shadow.** A piece of work declares `status`, so a capture writing it as
    /// a field means the column and must never be told to call update_fact
    /// naming `status` as an argument — that argument is the claim's own
    /// state. Paired with the key the kind does not declare, on the same
    /// session: the declared key must not spend the domain's one teaching, and
    /// the undeclared one must still fire it.
    #[tokio::test]
    async fn a_capture_writing_a_declared_key_is_not_taught_it_shadows_an_argument() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let declared: Vec<String> = jojobot_domain::memory::kinds::keys_of("work")
            .into_iter()
            .map(|f| f.key)
            .collect();
        assert!(
            declared.iter().any(|d| d == "status"),
            "the case rests on a work thing declaring `status`"
        );

        let mut column = capture_args("work:sigma", "the piece is under way");
        column.fields = Some(
            [("status".to_string(), "now".to_string())]
                .into_iter()
                .collect(),
        );
        let column = capture_as(&jojobot, &sid, column).await;
        assert!(
            !column["teaching"]
                .as_array()
                .map(|t| t.iter().any(|t| t
                    .as_str()
                    .is_some_and(|s| s.contains("is stored as ordinary data"))))
                .unwrap_or(false),
            "a work's own status column is not a field shadowing an argument: {column}"
        );

        let mut shadow = capture_args("work:sigma", "the piece has a note");
        shadow.fields = Some(
            [("provenance".to_string(), "testimony".to_string())]
                .into_iter()
                .collect(),
        );
        let shadow = capture_as(&jojobot, &sid, shadow).await;
        assert!(
            shadow["teaching"]
                .as_array()
                .map(|t| t
                    .iter()
                    .any(|t| t.as_str().is_some_and(|s| s.contains("\"provenance\"")
                        && s.contains("is stored as ordinary data"))))
                .unwrap_or(false),
            "a key the kind does not declare still shadows, and the declared one spent nothing: \
             {shadow}"
        );
    }
}

#[cfg(test)]
mod where_example_tests {
    use super::*;
    use serde_json::json;

    fn object(fields: serde_json::Value) -> serde_json::Value {
        json!({"id": "work:tau", "fields": fields})
    }

    /// **The example is a key most objects hold, with two values and one of them shared.**
    /// A key unique to each object groups nothing, a key held by few is not the read's own,
    /// and a value with a space cannot be written bare, so each is passed over for the key
    /// that fits. The commonest value of that key is the example.
    #[test]
    fn the_example_is_the_commonest_value_of_the_key_most_objects_hold() {
        let objects: Vec<_> = [
            ("working", "1", "a b"),
            ("working", "2", "c d"),
            ("working", "3", "e f"),
            ("queued", "4", "g h"),
            ("queued", "5", "i j"),
            ("done", "6", "k l"),
        ]
        .into_iter()
        .map(|(status, position, label)| {
            object(json!({"status": status, "position": position, "label": label}))
        })
        .collect();
        assert_eq!(
            where_example(&objects),
            Some(("status".to_string(), "working".to_string()))
        );
    }

    /// **Nothing fits, so nothing is offered**: every key is unique per object, or held by
    /// fewer than half, or holds one value. Paired with the case above, which has a key.
    #[test]
    fn no_key_that_fits_means_no_example() {
        let unique: Vec<_> = (1..=6)
            .map(|n| object(json!({"position": n.to_string()})))
            .collect();
        assert_eq!(
            where_example(&unique),
            None,
            "a key unique to each groups nothing"
        );
        let few: Vec<_> = (1..=6)
            .map(|n| {
                if n <= 2 {
                    object(json!({"status": if n == 1 { "a" } else { "b" }}))
                } else {
                    object(json!({}))
                }
            })
            .collect();
        assert_eq!(
            where_example(&few),
            None,
            "a key most do not hold is not the read's own"
        );
        let one: Vec<_> = (1..=6)
            .map(|_| object(json!({"status": "working"})))
            .collect();
        assert_eq!(
            where_example(&one),
            None,
            "a key with one value asks nothing"
        );
    }
}
