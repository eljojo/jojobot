//! The pages themselves — HTML written by hand, because a directory listing is
//! a heading and a table and nothing that needs a template engine.

use std::collections::{BTreeMap, HashMap};

use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};

use jojobot_domain::attention;
use jojobot_domain::memory::{Entity, EntityId, EntityKind, Fact, graph, kinds, mention};
use jojobot_domain::text;

use crate::AppState;
use crate::ui::tree;

/// `/` — the index of the roots: every entity that sits under nothing.
///
/// The roots are where the tree starts, so this is the top of the listing.
/// Descending is a link away and a level at a time, which is what the tree is
/// for: a page of everything jojobot knows is the silting the tree exists to
/// stop.
pub async fn index(State(state): State<AppState>) -> Response {
    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => {
            return unreadable(err);
        }
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();

    let mut roots: Vec<&Entity> = Vec::new();
    let mut unreachable: Vec<&Entity> = Vec::new();
    for entity in &entities {
        match &entity.parent {
            None => roots.push(entity),
            // It is filed under a handle no entity has, so it is nobody's
            // child and no descent from a root arrives at it. Listed here or
            // listed nowhere.
            Some(parent) if !by_id.contains_key(parent) => unreachable.push(entity),
            Some(_) => {}
        }
    }
    roots.sort_by(|a, b| a.id.cmp(&b.id));
    unreachable.sort_by(|a, b| a.id.cmp(&b.id));

    let mut body = String::from(
        "<h1>Index of /</h1>\n<table id=\"roots\">\n<tr><th>Name</th><th>Called</th></tr>\n",
    );
    for entity in &roots {
        body.push_str(&row(entity, &by_id));
    }
    if roots.is_empty() {
        body.push_str("<tr><td colspan=\"2\">Nothing is filed here yet.</td></tr>\n");
    }
    body.push_str("</table>\n");
    body.push_str(&damaged(&unreachable, &by_id));

    html(&page("Index of /", &body))
}

/// The entities that name a parent nobody has.
///
/// **Shown as damaged, not folded in with the roots.** Promoting one would
/// render a broken record as a normal one, and this is the only page that can
/// say the record is broken at all. The section is absent when there are none:
/// a standing "damaged" heading over an empty table on a healthy store teaches
/// a reader to skip it, which is what it costs when it matters.
fn damaged(unreachable: &[&Entity], by_id: &HashMap<&EntityId, &Entity>) -> String {
    if unreachable.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "<h2>Filed under something that is not there</h2>\n\
         <p>Each of these names a parent no entity has, so nothing below a root reaches it. \
         No jojobot write can produce this, so these records were changed elsewhere.</p>\n\
         <table id=\"unreachable\">\n\
         <tr><th>Name</th><th>Called</th><th>Filed under</th></tr>\n",
    );
    for entity in unreachable {
        let handle = escape(entity.id.as_str());
        out.push_str(&format!(
            "<tr><td><a href=\"{}\">{handle}/</a></td><td>{}</td><td>{} (not found)</td></tr>\n",
            escape(&tree::canonical_path(entity, by_id)),
            escape(&entity.name),
            escape(entity.parent.as_ref().map_or("", EntityId::as_str)),
        ));
    }
    out.push_str("</table>\n");
    out
}

/// One entity as a row: its handle, linking to where it lives, and the name it
/// goes by.
///
/// **The link is the ancestry walk, the same one the node page redirects to.**
/// A row that built the path from where it happened to be listed would be a
/// second answer to where a thing lives, and the two would disagree the first
/// time a record was damaged.
fn row(entity: &Entity, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let handle = escape(entity.id.as_str());
    format!(
        "<tr><td><a href=\"{}\">{handle}/</a></td><td>{}</td></tr>\n",
        escape(&tree::canonical_path(entity, by_id)),
        escape(&entity.name)
    )
}

/// **What a reader asked this page for beyond the node itself.**
///
/// A query string rather than a second URL: the node is the page, and opening
/// one of its keys is a way of looking at that page rather than a different
/// thing to look at. It is also the whole of the state — there is none in the
/// browser, and a reader who bookmarks the link gets the same page back.
#[derive(Debug, Default, serde::Deserialize)]
pub struct NodeQuery {
    /// A key to open: the page renders every write behind it, oldest first.
    #[serde(default)]
    pub(crate) history: Option<String>,
}

/// `/{path}` — one node: what sits below it, and the facts held there.
///
/// **The path is the entity's ancestry, so there is one URL per entity.** A
/// handle reached by any other path is the same entity somewhere it does not
/// live, and the browser is sent to where it does — a second URL serving the
/// same node is how two readers come to disagree about where something is.
pub async fn node(
    State(state): State<AppState>,
    Path(path): Path<String>,
    axum::extract::Query(asked): axum::extract::Query<NodeQuery>,
) -> Response {
    let Some(handles) = tree::segments(&path) else {
        return not_found();
    };

    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => return unreadable(err),
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();

    let wanted = handles
        .last()
        .expect("segments never returns an empty path");
    let Some(entity) = by_id.get(wanted).copied() else {
        return not_found();
    };

    let canonical = tree::canonical_path(entity, &by_id);
    if canonical.trim_matches('/') != path.trim_matches('/') {
        return Redirect::to(&canonical).into_response();
    }

    let mut children: Vec<&Entity> = entities
        .iter()
        .filter(|candidate| candidate.parent.as_ref() == Some(&entity.id))
        .collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));

    let facts = match state.memory.recall(&entity.id).await {
        Ok(facts) => facts,
        Err(err) => return unreadable(err),
    };

    // **What the thing IS is a read of its own**, and it fails the page the way
    // a failed read of its records does: the row is the headline of this page,
    // and a page that quietly showed an empty one would be reporting that
    // nothing is recorded here.
    let held = match state.memory.fields(&entity.id).await {
        Ok(held) => held,
        Err(err) => return unreadable(err),
    };

    // Prose is the human half of the record — a charter, a portrait. Its
    // absence is ordinary, and a store that cannot be asked for it costs the
    // prose rather than the page.
    let prose = match state.memory.scan_entity(&entity.id).await {
        Ok(Some(scan)) => scan.prose,
        Ok(None) => String::new(),
        Err(err) => {
            tracing::debug!(error = %err, entity = %entity.id, "the listing could not read prose");
            String::new()
        }
    };

    let mut body = format!(
        "<h1>Index of {}</h1>\n{}",
        escape(&canonical),
        trail(&handles)
    );
    body.push_str(&about(entity));
    body.push_str(&fields_section(&held, &canonical));
    // **A view is a question, so its page answers it.** It sits under the
    // definition rather than over it: a reader has to see which question they
    // asked before they read what it came back with.
    if entity.kind == EntityKind::VIEW {
        body.push_str(&view_section(&state, &held, &by_id).await);
    }
    body.push_str(&history_section(&state, &entity.id, asked.history.as_deref()).await);
    body.push_str(&conforms_section(&state, &held).await);

    body.push_str("<h2>Below here</h2>\n");
    if children.is_empty() {
        body.push_str("<p>Nothing sits under this.</p>\n");
    } else {
        body.push_str("<table id=\"children\">\n<tr><th>Name</th><th>Called</th></tr>\n");
        for child in children {
            body.push_str(&row(child, &by_id));
        }
        body.push_str("</table>\n");
    }

    body.push_str(&facts_table(&facts, &by_id));

    if !prose.trim().is_empty() {
        body.push_str(&format!("<h2>Prose</h2>\n<pre>{}</pre>\n", escape(&prose)));
    }

    // The records that are not entities hang off the one entity that owns
    // them, so they are reached by walking from a handle like everything else.
    if entity.kind == EntityKind::BOT {
        body.push_str(&mailbox_section(&state, &entity.id).await);
        body.push_str(&sessions_section(&state, &entity.id).await);
    }

    html(&page(&format!("Index of {canonical}"), &body))
}

/// The mailbox this bot owns, and the mail sitting in it.
///
/// **This read takes no delivery.** `list_mailboxes` and `scan_messages` move
/// nothing and mark nothing, which is what lets a window watch the mail rail
/// without changing it. The delivery verbs are not reachable from here and must
/// not become so: a page that marked a message read would take it out of the
/// waiting state its real consumer has never seen it in.
async fn mailbox_section(state: &AppState, bot: &EntityId) -> String {
    let boxes = match state.mailboxes.list_mailboxes().await {
        Ok(boxes) => boxes,
        Err(err) => return blind("Mailbox", "the mail rail", &err.to_string()),
    };
    let owned: Vec<_> = boxes
        .into_iter()
        .filter(|box_| &box_.owner == bot)
        .collect();
    if owned.is_empty() {
        return "<h2>Mailbox</h2>\n<p>This bot owns no mailbox.</p>\n".to_string();
    }

    let messages = match state.mailboxes.scan_messages().await {
        Ok(messages) => messages,
        Err(err) => return blind("Mailbox", "the mail rail", &err.to_string()),
    };

    let mut out = String::new();
    for box_ in owned {
        out.push_str(&format!(
            "<h2>Mailbox {}</h2>\n<p>{} new, {} read, {} processed",
            escape(box_.name.as_str()),
            box_.counts.new,
            box_.counts.read,
            box_.counts.processed,
        ));
        if !box_.quarantined.is_empty() {
            // Invisible to every other verb, so this is the only place its
            // existence is stated at all.
            out.push_str(&format!(
                ", and {} jojobot cannot read",
                box_.quarantined.len()
            ));
        }
        out.push_str(".</p>\n");

        let mut mail: Vec<_> = messages
            .iter()
            .filter(|message| message.mailbox == box_.name)
            .collect();
        mail.sort_by_key(|message| message.sent_at);

        if mail.is_empty() {
            out.push_str("<p>Nothing has been left here.</p>\n");
            continue;
        }
        out.push_str(
            "<table id=\"mailbox\">\n<tr><th>Id</th><th>State</th><th>From</th><th>About</th>\
             <th>Sent</th><th>Outcome</th></tr>\n",
        );
        for message in mail {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n\
                 <tr><td colspan=\"6\">{}</td></tr>\n",
                escape(message.id.as_str()),
                escape(message.state.as_token()),
                escape(&message.sender),
                escape(message.subject.as_deref().unwrap_or("")),
                escape(&message.sent_at.to_string()),
                escape(message.notes.as_deref().unwrap_or("")),
                block(&message.body),
            ));
        }
        out.push_str("</table>\n");
    }
    out
}

/// Every run of this bot, newest first, each with the chronology it wrote.
///
/// `sessions_of` is a read: it begins nothing, closes nothing and sweeps
/// nothing. Booting is what starts a run, and this page is not a boot.
async fn sessions_section(state: &AppState, bot: &EntityId) -> String {
    let mut runs = match state.sessions.sessions_of(bot).await {
        Ok(runs) => runs,
        Err(err) => return blind("Runs", "the session records", &err.to_string()),
    };
    if runs.is_empty() {
        return "<h2>Runs</h2>\n<p>This bot has never run.</p>\n".to_string();
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.started_at));

    let mut out = String::from(
        "<h2>Runs</h2>\n<table id=\"sessions\">\n\
         <tr><th>Session</th><th>State</th><th>Working on</th><th>Started</th>\
         <th>Beats</th></tr>\n",
    );
    for run in &runs {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape(run.sid.as_ref().map_or("", |sid| sid.as_str())),
            escape(run.state.as_token()),
            escape(&run.focus),
            escape(&run.started_at.to_string()),
            run.entries.len(),
        ));
    }
    out.push_str("</table>\n");

    // The chronology is the record of the run, so it is on the page rather
    // than a click away. A beat jojobot wrote is marked apart from one the
    // session wrote, because they are different kinds of evidence.
    for run in &runs {
        if run.entries.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "<h3>Chronology of {}</h3>\n<table>\n<tr><th>When</th><th>Beat</th><th>Entry</th></tr>\n",
            escape(run.sid.as_ref().map_or("", |sid| sid.as_str())),
        ));
        for entry in &run.entries {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape(&entry.at.to_string()),
                escape(entry.beat.as_deref().unwrap_or("")),
                block(&entry.text),
            ));
        }
        out.push_str("</table>\n");
    }
    out
}

/// A block somebody wrote — whole, and folded to its opening when it is long.
///
/// **Nothing is left out.** The full text is inside the element, on this page:
/// a window for reading your own instance that dropped the end of a record
/// would be the wrong tool, and the fold is about scanning a page rather than
/// about what it carries. So there is no marker saying where to get the rest —
/// the rest is here.
///
/// **The opening and the size are jojobot's existing answers, not new ones.**
/// `BODY_DIGEST` is the strategy every other surface uses for the opening of a
/// body it is not shipping up front, and the byte count beside it is what
/// `body_bytes` carries. A second convention invented in a rendering pass would
/// be a second answer to how much of a body is enough to recognize it by.
fn block(text: &str) -> String {
    let digest = text::BODY_DIGEST.render(text);
    // The ellipsis is what the strategy appends when it cuts — and it is also a
    // character people type, so on its own it folds a one-line message that
    // trails off, with nothing behind the fold. The strategy hands back a
    // string and no cut flag, so the other half of the question is asked of the
    // budget it cut against: below that, it cannot have cut, and the ellipsis
    // is the writer's own.
    if !digest.ends_with('…') || text.chars().count() <= text::BODY_DIGEST.budget {
        return format!("<pre>{}</pre>", escape(text));
    }
    format!(
        "<details><summary>{} ({} bytes)</summary><pre>{}</pre></details>",
        escape(&digest),
        text.len(),
        escape(text),
    )
}

/// A section that could not be read.
///
/// **It says so rather than rendering as empty.** "Nothing is here" and
/// "jojobot could not look" are different claims, and the reader of this page
/// is reading it to debug the instance it serves: the first sends them to the
/// record, the second to the layer that would not answer. The store's own
/// words go to the log, not to the page: this text names jojobot's vocabulary,
/// never the storage product's furniture.
fn blind(heading: &str, what: &str, err: &str) -> String {
    tracing::warn!(error = %err, "the listing could not read {what}");
    format!(
        "<h2>{}</h2>\n<p>jojobot could not read {}, so this section is missing rather \
         than empty.</p>\n",
        escape(heading),
        escape(what),
    )
}

/// The path as links, one per ancestor, so any level is a click away.
fn trail(handles: &[EntityId]) -> String {
    let mut out = String::from("<p><a href=\"/\">/</a>");
    let mut so_far = String::from("/");
    for handle in handles {
        so_far.push_str(handle.as_str());
        so_far.push('/');
        out.push_str(&format!(
            "<a href=\"{}\">{}/</a>",
            escape(&so_far),
            escape(handle.as_str())
        ));
    }
    out.push_str("</p>\n");
    out
}

/// What the record itself says about this entity, beyond the facts on it.
fn about(entity: &Entity) -> String {
    let mut rows = format!(
        "<tr><td>name</td><td>{}</td></tr>\n<tr><td>kind</td><td>{}</td></tr>\n\
         <tr><td>source</td><td>{}</td></tr>\n",
        escape(&entity.name),
        escape(entity.kind.as_token()),
        escape(&entity.source),
    );
    if !entity.aliases.is_empty() {
        rows.push_str(&format!(
            "<tr><td>also called</td><td>{}</td></tr>\n",
            escape(&entity.aliases.join(", "))
        ));
    }
    if let Some(crm) = &entity.crm {
        rows.push_str(&format!("<tr><td>crm</td><td>{}</td></tr>\n", escape(crm)));
    }
    format!("<table>\n{rows}</table>\n")
}

/// **What the thing IS: one value per key, the newest write.**
///
/// A thing gets described a piece at a time, so this is the map every type
/// question is asked of — not a per-record scattering the reader has to merge
/// in their head.
///
/// **Asked of the store rather than folded from the records this page already
/// read**, because those two answers differ: a record has projected away which
/// of its writes was the newest on the thing and which key was taken off it, so
/// a page folding them for itself would show a value the store does not hold
/// and a key it has dropped.
///
/// **The heading stands even when there is nothing under it.** A section that
/// vanished would leave a reader unable to tell a thing with no fields from a
/// page that does not show them, which is the very confusion this section
/// exists to end.
fn fields_section(folded: &BTreeMap<String, String>, canonical: &str) -> String {
    if folded.is_empty() {
        return "<h2>Fields</h2>\n<p>Nothing is recorded on this.</p>\n".to_string();
    }
    let mut out = String::from("<h2>Fields</h2>\n<table id=\"fields\">\n");
    for (key, value) in folded {
        // **The key is a link to its own writes.** What a key holds now is one
        // answer and how it got there is the other, and a page that showed
        // only the first is a page where the second cannot be asked for.
        out.push_str(&format!(
            "<tr><td><a href=\"{}?history={}\">{}</a></td><td>{}</td></tr>\n",
            escape(canonical),
            escape(key),
            escape(key),
            escape(value)
        ));
    }
    out.push_str("</table>\n");
    out
}

/// **What this view answers** — the view, run.
///
/// A view is a record that holds a query, so a page that rendered only what the
/// record holds showed the question and never the answer. **Running it is
/// showing what is there**, for a thing whose contents are defined by a query.
///
/// **Nothing here computes anything the query does not.** It reads the view's
/// keys through the one reader of that vocabulary, hands the graph the same
/// query the served verb would hand it, and renders the objects in the shape
/// every other listing on this page uses.
///
/// ⛔️ **It writes nothing and takes no delivery.** Looking at a view cannot
/// move a message, mark anything read, or open a session.
///
/// **A caller with no identity**, so the answer reaches everything unowned and
/// nothing owned. There is no session behind a browser, and the access rule is
/// the absence of an argument rather than a check on one.
async fn view_section(
    state: &AppState,
    held: &BTreeMap<String, String>,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let asked = graph::asked_by_view(held);
    // **A view short of what it selects is a question nobody can ask**, and it
    // says which key it lacks. Rendering it as an empty answer would send a
    // reader looking for the records instead of for the key.
    let Some(selects) = asked
        .selects
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return cannot_run(
            "it holds no <code>selects</code>, so it does not say which kind it looks at.",
        );
    };
    let kind = match kinds::resolve(selects) {
        Ok(kind) => kind,
        // **The resolver's own answer**, which already names the value and the
        // kinds this instance holds. A sentence of this page's own beside it
        // would be a second account of one refusal.
        Err(why) => return cannot_run(&escape(&why.to_string())),
    };
    let query = graph::GraphQuery {
        select: graph::Selection {
            kind: Some(kind),
            ..graph::Selection::default()
        },
        // **The objects, and nothing of each.** A view's `shows` says what of
        // each one comes back on a READ; this is a directory listing, and it
        // lists. Asking for prose here would cost a page per result and put
        // none of it on screen.
        include: graph::Include {
            facts: false,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    let mut found = match graph::walk(&*state.memory, &query).await {
        Ok(selected) => selected.objects,
        Err(err) => {
            return blind(
                "What this view answers",
                "the records it selects",
                &err.to_string(),
            );
        }
    };
    // **The one narrowing a selection cannot express.** A page that ran the
    // selection alone would answer with more than the view asked for, which is
    // the page telling a reader something the view does not say.
    if asked.overdue {
        let carriers = attention::shipped();
        let carriers: Vec<&dyn attention::Carrier> =
            carriers.iter().map(std::convert::AsRef::as_ref).collect();
        // **The server's own day, in UTC.** A browser states no timezone, and
        // UTC is the same stated fallback every other unzoned read here uses.
        let today = state.clock.today_in(&jiff::tz::TimeZone::UTC);
        found.retain(|object| {
            attention::owed(&carriers, object.entity.id.kind_token(), &object.fields).owed_on(today)
        });
    }
    if found.is_empty() {
        return "<h2>What this view answers</h2>\n\
                <p>This view matches nothing today. The question is a good one; \
                nothing here answers it.</p>\n"
            .to_string();
    }
    let mut out = String::from(
        "<h2>What this view answers</h2>\n<table id=\"answer\">\n\
         <tr><th>Name</th><th>Called</th></tr>\n",
    );
    for object in &found {
        out.push_str(&row(&object.entity, by_id));
    }
    out.push_str("</table>\n");
    out
}

/// **A view that cannot be run says why, under the heading its answer would
/// have had.**
///
/// A section that vanished, or one that rendered as an empty answer, tells a
/// reader that nothing matches — which sends them looking at the records when
/// the repair is on the view itself.
fn cannot_run(why: &str) -> String {
    format!("<h2>What this view answers</h2>\n<p>This view cannot be run: {why}</p>\n")
}

/// **The writes behind one key, oldest first** — what the fold above projects
/// away.
///
/// **Absent unless a key was asked for.** Every page carrying an empty history
/// block would be a section a reader learns to skip, and the fold is the answer
/// nearly every time: this is the one-in-a-hundred read, and it is one click
/// from the value it explains.
///
/// **Nothing here writes.** It is a read of the same substrate the fold comes
/// from, so looking at the operator's own page cannot move a message, mark
/// anything read, or start a session.
async fn history_section(state: &AppState, entity: &EntityId, key: Option<&str>) -> String {
    let key = match key.map(str::trim) {
        None | Some("") => return String::new(),
        Some(key) => key,
    };
    let writes = match state.memory.history(entity, key).await {
        Ok(writes) => writes,
        Err(err) => {
            return blind(
                &format!("Writes of {}", escape(key)),
                "the writes behind a key",
                &err.to_string(),
            );
        }
    };
    if writes.is_empty() {
        return format!(
            "<h2>Writes of {}</h2>\n<p>Nothing has been written under this key.</p>\n",
            escape(key)
        );
    }
    // **The last column is headed as the facts table heads the same axis.**
    // It carries the record's status, and "Standing" is spent one section down
    // on settled/open: one word over two axes on one page reads as either a
    // write that can be settled or a standing that can be retracted.
    let mut out = format!(
        "<h2>Writes of {}</h2>\n<table id=\"history\">\n\
         <tr><th>Value</th><th>When</th><th>Record</th><th>State</th></tr>\n",
        escape(key)
    );
    for write in &writes {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            // **A clear is a write, and it says so rather than rendering
            // blank.** An empty cell reads as a value somebody wrote.
            match &write.value {
                Some(value) => escape(value),
                None => "<em>taken off</em>".to_string(),
            },
            escape(&write.recorded_at.to_string()),
            escape(&write.fact.to_string()),
            escape(write.status.as_token()),
        ));
    }
    out.push_str("</table>\n");
    out
}

/// **What the thing's fields add up to**: every declared type it answers, the
/// keys it holds and the keys it lacks.
///
/// This is the payoff of the fold and the one thing on the page a reader cannot
/// work out unaided — it would mean holding every declaration in mind.
///
/// **One extra read, and no walk.** The declarations are a table of their own
/// and the thing's fields are already in hand, so this costs one query on a
/// small table rather than a pass over the corpus per thing.
///
/// **Absent when the thing answers nothing.** A standing "conforms to nothing"
/// on every page is a judgement nobody asked for, and a store with no
/// declarations at all would carry it everywhere.
async fn conforms_section(state: &AppState, folded: &BTreeMap<String, String>) -> String {
    if folded.is_empty() {
        return String::new();
    }
    let declared = match state.memory.declared_types().await {
        Ok(declared) => declared,
        Err(err) => {
            tracing::debug!(error = %err, "the listing could not read the declared types");
            return String::new();
        }
    };
    let mut rows = String::new();
    for declaration in &declared {
        let Some(found) = declaration.matched_by(folded) else {
            continue;
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape(&declaration.name),
            if found.complete() { "whole" } else { "partly" },
            escape(&found.held.join(", ")),
            escape(&found.lacking.join(", ")),
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    format!(
        "<h2>Conforms to</h2>\n<table id=\"conforms\">\n\
         <tr><th>Type</th><th>How far</th><th>Holds</th><th>Lacks</th></tr>\n{rows}</table>\n"
    )
}

/// The facts held at this node.
///
/// **Every claim arrives with what qualifies it.** Who backs it and how settled
/// it is are what tell a claim from a hypothesis, so they are columns rather
/// than something a reader has to go and ask for.
fn facts_table(facts: &[Fact], by_id: &HashMap<&EntityId, &Entity>) -> String {
    if facts.is_empty() {
        return "<h2>Facts</h2>\n<p>Nothing is recorded here.</p>\n".to_string();
    }

    let mut out = String::from(
        "<h2>Facts</h2>\n<table>\n<tr><th>Claim</th><th>Backed by</th><th>Standing</th>\
         <th>State</th><th>Date</th><th>Relation</th><th>Address</th></tr>\n",
    );
    for fact in facts {
        let claim = match &fact.details {
            Some(details) if !details.trim().is_empty() => format!(
                "{}<br><small>{}</small>",
                linkify(&fact.content, by_id),
                linkify(details, by_id)
            ),
            _ => linkify(&fact.content, by_id),
        };
        out.push_str(&format!(
            "<tr><td>{claim}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td>{}</td></tr>\n",
            escape(fact.provenance.as_token()),
            escape(fact.standing.as_token()),
            escape(fact.status.as_token()),
            escape(&fact.recorded_at.to_string()),
            relation(fact, by_id),
            escape(&fact.address().to_string()),
        ));
        out.push_str(&record_fields(fact));
    }
    out.push_str("</table>\n");
    out
}

/// **The fields this one record carries**, on a line of its own under it.
///
/// Not an eighth column. The seven columns are each one value and each present
/// on every row; a record's fields are a map of any size and are on few rows,
/// so a column would be empty almost everywhere and would push the address out
/// of a readable width when it was not. A line under the row keeps the columns
/// meaning what they say and puts the fields where the record is.
///
/// Empty for a record carrying none, which is most of them.
fn record_fields(fact: &Fact) -> String {
    if fact.fields.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = fact
        .fields
        .iter()
        .map(|(key, value)| format!("{} = {}", escape(key), escape(value)))
        .collect();
    format!(
        "<tr class=\"fields\"><td colspan=\"7\"><small>{}</small></td></tr>\n",
        pairs.join(" · ")
    )
}

/// **A mention in served text becomes a link to that thing's own page.**
///
/// The text a reader sees is unchanged — today's handle, exactly as
/// `mention::rendered` already resolved it — and a followable span becomes
/// an anchor. **The two shapes that cannot be followed stay exactly as
/// served, never as a dead link**: `mention::followable` is the one place
/// that tells them apart, so this never has to.
fn linkify(text: &str, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cut = 0;
    for (span, handle) in mention::followable(text) {
        out.push_str(&escape(&text[cut..span.start]));
        let written = escape(&text[span.clone()]);
        match by_id.get(&handle).copied() {
            Some(entity) => out.push_str(&format!(
                "<a href=\"{}\">{written}</a>",
                escape(&tree::canonical_path(entity, by_id)),
            )),
            // **Followable and not on this page's listing.** `by_id` is every
            // entity `list_entities` returned, so this is a read that raced a
            // deletion rather than a scope this page never carries — rare
            // enough that the safe fallback is the plain text, not a link
            // that might not resolve.
            None => out.push_str(&written),
        }
        cut = span.end;
    }
    out.push_str(&escape(&text[cut..]));
    out
}

/// The relation a fact draws, as a link to where that entity lives.
///
/// The shape is named beside the link, because the shape IS what the link
/// means — and `connection` in particular says a link is there and that how it
/// relates was not recorded, which is a different claim from any of the others.
fn relation(fact: &Fact, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let Some(edge) = &fact.edge else {
        return String::new();
    };
    let shape = escape(edge.shape.as_token());
    match by_id.get(&edge.object).copied() {
        Some(object) => format!(
            "{shape} → <a href=\"{}\">{}</a>",
            escape(&tree::canonical_path(object, by_id)),
            escape(edge.object.as_str())
        ),
        // The object of an edge must exist to be written, so a miss here is a
        // record edited outside jojobot. Naming it beats a dead link.
        None => format!("{shape} → {} (not found)", escape(edge.object.as_str())),
    }
}

/// Nothing is filed at that path. Public, because the gate answers with it too:
/// a path that is not a handle path is missing whether or not anybody is
/// logged in.
pub fn not_found() -> Response {
    crate::ui::refuse(
        StatusCode::NOT_FOUND,
        "No entity has that handle. Nothing is filed at this path.",
    )
}

fn unreadable(err: jojobot_domain::memory::MemoryError) -> Response {
    tracing::warn!(error = %err, "the listing could not read the store");
    crate::ui::refuse(
        StatusCode::BAD_GATEWAY,
        "jojobot could not read its own store, so this page would be empty rather than accurate.",
    )
}

/// A page with a sentence on it — a refusal, or anything else with nothing to
/// list.
pub fn plain_page(title: &str, message: &str) -> String {
    page(
        title,
        &format!("<h1>{}</h1>\n<p>{}</p>\n", escape(title), escape(message)),
    )
}

/// The document around a page's body.
fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(title)
    )
}

/// Enough style to be readable and no more. A directory listing that grew a
/// design would be the application this is deliberately not.
const STYLE: &str = "body{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;\
                     max-width:60rem}table{border-collapse:collapse}\
                     td,th{text-align:left;padding:.15rem 1.5rem .15rem 0}\
                     th{border-bottom:1px solid currentColor}a{text-decoration:none}\
                     a:hover{text-decoration:underline}";

/// Escape text for HTML. Handles are validated to a narrow alphabet, but a
/// name, a fact and a piece of prose are free text a person wrote.
pub fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for character in raw.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn html(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{block, escape};

    /// **A fold is a promise that there is more behind it.** The long half is
    /// the positive the short half depends on: without it, a `block` that never
    /// folded anything would pass the second assertion alone.
    #[test]
    fn a_block_folds_only_when_the_digest_left_something_out() {
        let long = "counted the crates. ".repeat(20);
        let folded = block(&long);
        assert!(
            folded.starts_with("<details>"),
            "a block past the digest's budget folds: {folded}"
        );
        assert!(
            folded.contains(&long),
            "and the whole text is behind the fold: {folded}"
        );

        // An ellipsis somebody typed is not the strategy saying it cut.
        let trails_off = "well, that settles it…";
        assert_eq!(
            block(trails_off),
            format!("<pre>{trails_off}</pre>"),
            "a short block that trails off has nothing to hide"
        );
    }

    #[test]
    fn a_name_a_person_wrote_cannot_close_a_tag() {
        assert_eq!(
            escape("<script>alert(\"x\")</script>"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
        );
        assert_eq!(escape("Ampersand & co"), "Ampersand &amp; co");
    }
}
