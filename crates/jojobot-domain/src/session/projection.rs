use super::*;

/// **A session, as an object the graph query can select.**
///
/// ⭐ **The axis already existed; the handle is what a session lacked.** Give a
/// run a handle and every question the query already answers reaches it —
/// select by kind, follow the edge to the bot that owns it, read its
/// chronology.
///
/// **The owner rides on the document rather than in its fields**, so a caller
/// cannot change who may read a run by writing a key. A bot reads its own runs
/// and nobody else's, and the count of what was withheld is what stops that
/// reading as an empty store.
///
/// ⛔️ **The store holds no row for this.** It is projected at the read, the way
/// the build's own records are, so nothing is migrated and no session is
/// duplicated into the entity table. The write gate refuses a memory write onto
/// the handle, which is what keeps the projection read-only.
pub fn projected(session: &Session) -> crate::memory::search::DocScan {
    crate::memory::search::DocScan {
        // **The chronology is the run's prose**, oldest first, which is the
        // order it was written and the order it reads in.
        prose: session
            .entries
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        ..projected_shell(
            &session.id,
            &session.bot,
            &session.focus,
            session.state,
            session.started_at,
            session.entries.len(),
            session.sid.as_ref(),
        )
    }
}

/// **A run, selectable and unreadable — deliberately, and only ever a
/// starting point.** Same entity, same owner, same fields [`projected`]
/// builds; the one thing this never carries is the chronology, because a
/// [`SessionSummary`] never read it. **Never hand this to a caller as the
/// answer**: it exists so a run can join `resolve`'s selection cheaply, and
/// whatever it selects is read whole afterwards, through
/// [`Sessions::read_session`] and [`projected`] — see
/// [`crate::memory::graph::walk`]'s own doc on why this file draws that
/// line rather than fetching the chronology here.
pub fn projected_summary(summary: &SessionSummary) -> crate::memory::search::DocScan {
    let mut doc = crate::memory::search::DocScan {
        prose: String::new(),
        ..projected_shell(
            &summary.id,
            &summary.bot,
            &summary.focus,
            summary.state,
            summary.started_at,
            summary.entry_count,
            summary.sid.as_ref(),
        )
    };
    // **What the run cost, from its own record.** The days it was served on ride
    // only on a read of the one run: see `served_on_keys`.
    doc.fields.extend([
        (
            SERVED_SINCE_COMPACTION.to_string(),
            summary.served_chars.to_string(),
        ),
        (
            SERVED_LIFETIME.to_string(),
            summary.served_lifetime().to_string(),
        ),
        (COMPACTIONS.to_string(), summary.compactions.to_string()),
    ]);
    // **A run that could not be read says so and shows no moment**: the start and
    // the beat count it would otherwise carry are not to be trusted (rule 130).
    if let Some(why) = &summary.unreadable {
        doc.fields.remove("started_at");
        doc.fields.remove("beats");
        doc.fields.insert("unreadable".to_string(), why.clone());
    }
    doc
}

/// The key that holds the characters a run has been served since its last
/// compaction.
pub const SERVED_SINCE_COMPACTION: &str = "served_since_compaction";
/// The key that holds the characters a run has been served over its whole life.
pub const SERVED_LIFETIME: &str = "served_lifetime";
/// The key that holds how many compactions a run reported.
pub const COMPACTIONS: &str = "compactions";

/// **The days one run was served on, as the keys a read of that run carries**:
/// `served_<YYYY-MM-DD>` and the characters. A day on which nothing was served
/// has no key.
pub fn served_on_keys(days: impl IntoIterator<Item = (Date, u64)>) -> Vec<(String, String)> {
    days.into_iter()
        .map(|(day, chars)| {
            (
                format!("{}{day}", crate::usage::SERVED_ON),
                chars.to_string(),
            )
        })
        .collect()
}

/// **Everything [`projected`] and [`projected_summary`] share** — the
/// entity, the owner, the cheap fields — with `prose` left for each caller
/// to fill in its own way, since that is the one thing they answer
/// differently.
fn projected_shell(
    id: &SessionId,
    bot: &EntityId,
    focus: &str,
    state: SessionState,
    started_at: Timestamp,
    entry_count: usize,
    sid: Option<&Sid>,
) -> crate::memory::search::DocScan {
    let entity_id = EntityId::new(crate::memory::EntityKind::SESSION, id.as_str());
    crate::memory::search::DocScan {
        doc_id: id.as_str().to_string(),
        title: focus.to_string(),
        prose: String::new(),
        entity: Some(crate::memory::Entity {
            folded_names: Vec::new(),
            id: entity_id,
            kind: crate::memory::EntityKind::SESSION,
            // A run is known by what it was working on, which is what tells two
            // of them apart in an offer.
            name: focus.to_string(),
            aliases: Vec::new(),
            source: "jojobot".to_string(),
            crm: None,
            // **The bot owns its runs**, so the tree already says whose it is
            // and no second copy has to be kept in step.
            parent: Some(bot.clone()),
            boot: Default::default(),
            merged_into: None,
            // A session is projected as an entity so search can rank it; it is
            // no row in the entity table, so it wears no badge.
            badge: None,
            archived: None,
        }),
        facts: Vec::new(),
        fields: std::collections::BTreeMap::from([
            ("state".to_string(), state.as_token().to_string()),
            ("started_at".to_string(), started_at.to_string()),
            ("beats".to_string(), entry_count.to_string()),
        ])
        .into_iter()
        // **The run's sid is a field of the run**, so its owner can filter on
        // it and read it back, and the same value is what `start_here` takes
        // as `resume`. Absent on a run written before it was kept. Only the
        // owner ever reads this document, so it reaches nobody else.
        .chain(sid.map(|sid| ("sid".to_string(), sid.as_str().to_string())))
        .collect(),
        owner: Some(bot.clone()),
        field_links: Default::default(),
        unmapped: Vec::new(),
    }
}
