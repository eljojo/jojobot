//! The pages themselves — HTML written by hand, because a directory listing is
//! a heading and a table and nothing that needs a template engine.

use std::collections::{BTreeMap, HashMap};

use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};

use jojobot_domain::attention;
use jojobot_domain::memory::types::DeclaredType;
use jojobot_domain::memory::view_inputs;
use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, Fact, FieldAges, MemoryError, graph, kinds, mention, readiness,
};
use jojobot_domain::text;
use jojobot_mcp::memory::{left_out, view_run};

use crate::AppState;
use crate::ui::{org, shell, tree};

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
    for entity in entities.iter().filter(|e| e.browsable()) {
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

    html(&shell::page("Index of /", "/", &body))
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
    row_with(entity, by_id, &[])
}

/// A row with a cell after the two every listing carries for each of `cells`,
/// already escaped. A view's `shows_keys` are those cells.
fn row_with(entity: &Entity, by_id: &HashMap<&EntityId, &Entity>, cells: &[String]) -> String {
    let handle = escape(entity.id.as_str());
    let extra: String = cells
        .iter()
        .map(|cell| format!("<td>{cell}</td>"))
        .collect();
    format!(
        "<tr><td><a href=\"{}\">{handle}/</a></td><td>{}</td>{extra}</tr>\n",
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
    /// **A field added here is a key a view's input can no longer have**, so its
    /// name goes in [`view_inputs::RESERVED_NAMES`] too.
    #[serde(default)]
    pub(crate) history: Option<String>,
    /// **The values of a view's inputs**, each under the input's own name: the
    /// form a view's page renders is a GET of these. `history` is this page's
    /// own, so an input named that cannot be filled from the URL.
    #[serde(flatten)]
    pub(crate) inputs: BTreeMap<String, String>,
}

/// **How much of what grows a page holds before it says what it left out.** A
/// page that listed everything would grow with the history of the thing it
/// shows; the newest of each is what a reader came to look at, and the rest is
/// one read away, which each section says.
const FIELDS_SHOWN: usize = 40;
/// The runs whose chronologies a bot's page writes out. The table above them
/// keeps a line for every run.
const RUNS_SHOWN: usize = 20;
/// The messages of one box a bot's page writes out.
const MAIL_SHOWN: usize = 20;

/// **The line a section ends with when it showed fewer than it holds.** The count
/// rides in an attribute so a reader of the markup, and a test, takes the number
/// from the structure and not from the sentence around it.
fn left_out_line(id: &str, left: usize, what: &str, where_read: &str) -> String {
    format!(
        "<p id=\"{id}\" data-left-out=\"{left}\">{left} {what} left out of this page. \
         {where_read}</p>\n"
    )
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
        .filter(|candidate| candidate.parent.as_ref() == Some(&entity.id) && candidate.browsable())
        .collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));

    // **The review mark is bookkeeping, so the page leaves it out the way `recall`
    // does**, through the same predicate.
    let facts = match state.memory.recall(&entity.id).await {
        Ok(mut facts) => {
            facts.retain(|fact| !jojobot_domain::memory::review::is_review_mark(&fact.fields));
            facts
        }
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
    // **What a bot is working on is shown only while the session that wrote it
    // is live**, the same answer the other readers of a bot get.
    let mut held = held;
    let mut working_on_unknown = false;
    if entity.kind == EntityKind::BOT
        && held.contains_key(jojobot_domain::memory::kinds::WORKING_ON)
    {
        use jojobot_mcp::memory::working_on::{WorkingOn, working_on_of};
        if !matches!(
            working_on_of(&*state.memory, &*state.sessions, &entity.id).await,
            Ok(WorkingOn::Live(_))
        ) {
            held.remove(jojobot_domain::memory::kinds::WORKING_ON);
            working_on_unknown = true;
        }
    }
    // **When each key was written is what orders the section**, so it is read
    // the way the keys are and fails the page the same way.
    let ages = match state.memory.field_ages(&entity.id).await {
        Ok(ages) => ages,
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
    // **A document's page shows the document**, above everything that is the
    // record-level view of it. The text is the one `read_document` serves.
    if entity.kind == EntityKind::DOCUMENT {
        body.push_str(&crate::ui::document::block(
            &entity.name,
            held.get("one_liner").map(String::as_str),
            &jojobot_mcp::memory::documents::live_text(&facts),
            &by_id,
        ));
    }
    body.push_str(&fields_section(&held, &ages, &entity.id, &canonical));
    if working_on_unknown {
        body.push_str(
            "<p id=\"working-on-unknown\">What this bot is working on is unknown: the \
             session that wrote it is no longer live.</p>\n",
        );
    }
    // **Who works on a piece of work, and who has**, rolled up from everything
    // under it. Only work and projects carry it.
    if matches!(entity.kind, EntityKind::WORK | EntityKind::PROJECT) {
        body.push_str(&working_on_section(&state, entity, &entities, &by_id).await);
    }
    // **A bot's place in the org chart**: the chain it reports through and the
    // bots that report to it, read from the colleagues view.
    if entity.kind == EntityKind::BOT {
        body.push_str(&org::bot_section(&state, &entity.id, &by_id).await);
    }
    // **A project's ready work is one link away**, the ready view with the
    // project as its input.
    if entity.kind == EntityKind::PROJECT {
        body.push_str(&format!(
            "<p id=\"ready-work\"><a href=\"{}\">Ready work</a></p>\n",
            escape(&shell::view_url(
                "view:ready",
                &[("project", entity.id.as_str())]
            ))
        ));
    }
    // **A view is a question, so its page answers it.** It sits under the
    // definition rather than over it: a reader has to see which question they
    // asked before they read what it came back with.
    if entity.kind == EntityKind::VIEW {
        body.push_str(
            &view_section(&state, &entity.id, &held, &facts, &by_id, &asked.inputs).await,
        );
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

    body.push_str(&facts_table(&facts, &by_id, working_on_unknown));

    if !prose.trim().is_empty() {
        body.push_str(&format!(
            "<h2>Prose</h2>\n<pre>{}</pre>\n",
            linkify(&prose, &by_id)
        ));
    }

    // The records that are not entities hang off the one entity that owns
    // them, so they are reached by walking from a handle like everything else.
    if entity.kind == EntityKind::BOT {
        body.push_str(&mailbox_section(&state, &entity.id).await);
        body.push_str(&sessions_section(&state, &entity.id).await);
    } // **The operator's own box is on the operator's own page and nowhere else.**
    // Opening the page moves no mail: the listing is a GET, and a prefetch of it
    // must not mark anything seen.
    if entity.kind == EntityKind::PERSON
        && crate::ui::mail::operator_of(&state).await.as_ref() == Some(&entity.id)
    {
        body.push_str(&operators_mailbox_section(&state, &entity.id).await);
    }

    html(&shell::page(
        &format!("Index of {canonical}"),
        &canonical,
        &body,
    ))
}

/// **Who is working on this piece of work now, and who has been**, for a piece
/// of work or a project. A bot is on it when its live `working_on` points at it or
/// at anything under it; the runs that declared it and have ended are listed
/// below, dated, each linking to its row on the bot's page. Whether a
/// declaration is live is the one gate's answer ([`working_on_of`]); a bot that
/// only wrote to the work is not here, because only a declaration counts.
async fn working_on_section(
    state: &AppState,
    entity: &Entity,
    entities: &[Entity],
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    use jojobot_mcp::memory::working_on::declarations_on;

    // The piece of work itself and everything under it, at any depth.
    let mut under: std::collections::BTreeSet<EntityId> =
        std::collections::BTreeSet::from([entity.id.clone()]);
    loop {
        let before = under.len();
        for candidate in entities.iter().filter(|e| e.merged_into.is_none()) {
            if candidate
                .parent
                .as_ref()
                .is_some_and(|parent| under.contains(parent))
            {
                under.insert(candidate.id.clone());
            }
        }
        if under.len() == before {
            break;
        }
    }
    let bots: Vec<EntityId> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::BOT && e.merged_into.is_none())
        .map(|e| e.id.clone())
        .collect();
    let (declarations, incomplete) =
        declarations_on(&*state.memory, &*state.sessions, &bots, &under).await;

    let link = |id: &EntityId| match by_id.get(id) {
        Some(found) => format!(
            "<a href=\"{}\">{}/</a>",
            escape(&tree::canonical_path(found, by_id)),
            escape(id.as_str())
        ),
        None => escape(id.as_str()),
    };
    let session = |bot: &EntityId, run: &Option<String>| match (by_id.get(bot), run) {
        (Some(found), Some(run)) => format!(
            "<a href=\"{}#session-{}\">{}</a>",
            escape(&tree::canonical_path(found, by_id)),
            escape(run),
            escape(run)
        ),
        _ => String::new(),
    };

    let mut out = String::from("<h2>Working on this</h2>\n");
    let now: Vec<_> = declarations.iter().filter(|d| d.live).collect();
    if now.is_empty() {
        out.push_str("<p id=\"working-on-none\">Nobody is working on this now.</p>\n");
    } else {
        out.push_str(
            "<table id=\"working-on-now\">\n<tr><th>Bot</th><th>On</th><th>Session</th></tr>\n",
        );
        for d in now {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                link(&d.bot),
                link(&d.on),
                session(&d.bot, &d.run),
            ));
        }
        out.push_str("</table>\n");
    }
    let mut past: Vec<_> = declarations.iter().filter(|d| !d.live).collect();
    past.sort_by_key(|declaration| std::cmp::Reverse(declaration.at));
    if !past.is_empty() {
        out.push_str(
            "<h3>Past runs that declared it</h3>\n<table id=\"working-on-past\">\n\
             <tr><th>Day</th><th>Bot</th><th>On</th><th>Session</th></tr>\n",
        );
        for d in past {
            let day = d.at.map_or(String::new(), |at| {
                at.to_zoned(jiff::tz::TimeZone::UTC).date().to_string()
            });
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape(&day),
                link(&d.bot),
                link(&d.on),
                session(&d.bot, &d.run),
            ));
        }
        out.push_str("</table>\n");
    }
    if incomplete {
        out.push_str(
            "<p id=\"working-on-incomplete\">The history of some bot's declarations could \
             not be read whole, so this list may be missing runs.</p>\n",
        );
    }
    out
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
        // **The newest first**, and the message's id breaks a tie so two
        // readings of one page agree.
        mail.sort_by(|a, b| {
            b.sent_at
                .cmp(&a.sent_at)
                .then_with(|| b.id.as_str().cmp(a.id.as_str()))
        });

        if mail.is_empty() {
            out.push_str("<p>Nothing has been left here.</p>\n");
            continue;
        }
        let left = mail.len().saturating_sub(MAIL_SHOWN);
        mail.truncate(MAIL_SHOWN);
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
        if left > 0 {
            out.push_str(&left_out_line(
                "mailbox-left-out",
                left,
                "older messages are",
                "`search` with include_mail reads them, whatever their state.",
            ));
        }
    }
    out
}

/// **The operator's mailbox, and the mail in it, with the two actions.**
///
/// **This read moves nothing.** It is a GET, so a prefetch of the page must not
/// mark mail seen. A new message shows its row, an Open button and a
/// mark-processed form, and no body, because a body shown here would be a
/// message read while it still says new. Opening is a POST that takes delivery
/// of that one message; marking it processed is a POST too, and it works from
/// `new` or `read`. A read or processed message shows its body.
///
/// **Only while the allowlist admits exactly one person.** A session carries no
/// subject, so with any other number of people admitted, anyone who logs in
/// would read the operator's mail; the section says so instead.
async fn operators_mailbox_section(state: &AppState, operator: &EntityId) -> String {
    if !state
        .ui
        .as_ref()
        .is_some_and(|ui| ui.admits_exactly_one_person())
    {
        return format!(
            "<h2>Mailbox</h2>\n<p id=\"mailbox-withheld\">{}</p>\n",
            escape(crate::ui::mail::WITHHELD)
        );
    }
    let Some(held) = crate::ui::mail::operators_box(state, operator).await else {
        return "<h2>Mailbox</h2>\n<p>Nobody has written to this person yet.</p>\n".to_string();
    };
    let messages = match state.mailboxes.scan_messages().await {
        Ok(messages) => messages,
        Err(err) => return blind("Mailbox", "the mail rail", &err.to_string()),
    };
    let mut mail: Vec<_> = messages
        .iter()
        .filter(|message| message.mailbox == held.name)
        .collect();
    mail.sort_by_key(|message| message.sent_at);

    let mut out = format!(
        "<h2>Mailbox {}</h2>\n<p>{} new, {} read, {} processed",
        escape(held.name.as_str()),
        held.counts.new,
        held.counts.read,
        held.counts.processed,
    );
    if !held.quarantined.is_empty() {
        out.push_str(&format!(
            ", and {} jojobot cannot read",
            held.quarantined.len()
        ));
    }
    out.push_str(".</p>\n");
    if mail.is_empty() {
        out.push_str("<p>Nothing has been left here.</p>\n");
        return out;
    }
    out.push_str(
        "<table id=\"mailbox\">\n<tr><th>Id</th><th>State</th><th>From</th><th>About</th>\
         <th>Sent</th><th>Outcome</th><th></th></tr>\n",
    );
    for message in mail {
        let state_token = message.state.as_token();
        let open_form = format!(
            "<form method=\"post\" action=\"/ui/mail/open\">\
             <input type=\"hidden\" name=\"id\" value=\"{}\">\
             <button type=\"submit\">Open</button></form>",
            escape(message.id.as_str()),
        );
        let mark_form = format!(
            "<form method=\"post\" action=\"/ui/mail/processed\">\
             <input type=\"hidden\" name=\"id\" value=\"{}\">\
             <input type=\"text\" name=\"note\" placeholder=\"note (optional)\">\
             <button type=\"submit\">Mark processed</button></form>",
            escape(message.id.as_str()),
        );
        // **Only Open sets `read`**, so a message can be finished from `new`
        // without its body ever having been shown.
        let action = match state_token {
            "new" => format!("{open_form}{mark_form}"),
            "read" => mark_form,
            _ => String::new(),
        };
        // **A message its sender replaced says so and names the replacement**, so
        // the operator reads the correction and knows the mistake for what it is.
        // It keeps its state and its buttons: marking it processed stays theirs.
        let state_cell = match &message.superseded_by {
            Some(by) => format!(
                "<td data-replaced-by=\"{}\">{}, replaced by {}</td>",
                escape(by.as_str()),
                escape(state_token),
                escape(by.as_str()),
            ),
            None => format!("<td>{}</td>", escape(state_token)),
        };
        out.push_str(&format!(
            "<tr><td>{}</td>{}<td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape(message.id.as_str()),
            state_cell,
            escape(&message.sender),
            escape(message.subject.as_deref().unwrap_or("")),
            escape(&message.sent_at.to_string()),
            escape(message.notes.as_deref().unwrap_or("")),
            action,
        ));
        if state_token != "new" {
            out.push_str(&format!(
                "<tr><td colspan=\"7\">{}</td></tr>\n",
                block(&message.body)
            ));
        }
    }
    out.push_str("</table>\n");
    out
}

/// Every run of this bot, newest first, each with the chronology it wrote.
///
/// `sessions_of` is a read: it begins nothing, closes nothing and sweeps
/// nothing. Booting is what starts a run, and this page is not a boot.
async fn sessions_section(state: &AppState, bot: &EntityId) -> String {
    // **The summaries are the table, and a run is read whole only when its
    // chronology is written out**, so a bot with a long history costs a line a
    // run and the chronologies of the newest few.
    let mut runs = match state.sessions.summaries_of(bot).await {
        Ok(runs) => runs,
        Err(err) => return blind("Runs", "the session records", &err.to_string()),
    };
    if runs.is_empty() {
        return "<h2>Runs</h2>\n<p>This bot has never run.</p>\n".to_string();
    }
    runs.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.id.as_str().cmp(a.id.as_str()))
    });

    let mut out = String::from(
        "<h2>Runs</h2>\n<table id=\"sessions\">\n\
         <tr><th>Session</th><th>State</th><th>Working on</th><th>Started</th>\
         <th>Beats</th></tr>\n",
    );
    for run in &runs {
        // **The run's row is an anchor**, so a page that names the run can link
        // to it (the past runs on a piece of work's page do).
        let name = run.sid.as_ref().map_or("", |sid| sid.as_str());
        out.push_str(&format!(
            "<tr id=\"session-{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape(name),
            escape(run.sid.as_ref().map_or("", |sid| sid.as_str())),
            escape(run.state.as_token()),
            escape(&run.focus),
            escape(&run.started_at.to_string()),
            run.entry_count,
        ));
    }
    out.push_str("</table>\n");

    // The chronology is the record of the run, so it is on the page rather
    // than a click away. A beat jojobot wrote is marked apart from one the
    // session wrote, because they are different kinds of evidence.
    for summary in runs
        .iter()
        .filter(|run| run.entry_count > 0)
        .take(RUNS_SHOWN)
    {
        let run = match state.sessions.read_session(&summary.id).await {
            Ok(run) => run,
            Err(err) => return blind("Runs", "the session records", &err.to_string()),
        };
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
    let with_beats = runs.iter().filter(|run| run.entry_count > 0).count();
    if with_beats > RUNS_SHOWN {
        out.push_str(&left_out_line(
            "sessions-left-out",
            with_beats - RUNS_SHOWN,
            "older chronologies are",
            "`recall` with kind session and prose reads them.",
        ));
    }
    out
}

/// A block somebody wrote — whole, and folded to its opening when it is long.
///
/// **Nothing of a block it writes is left out.** The full text is inside the
/// element, on this page: a window for reading your own instance that dropped
/// the end of a record would be the wrong tool, and the fold is about scanning
/// a page rather than about what it carries. So there is no marker saying where
/// to get the rest — the rest is here. Which records a section writes out at
/// all is a different question, and a section that leaves some out says so.
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
    shell::fold(
        &format!("{} ({} bytes)", escape(&digest), text.len()),
        &format!("<pre>{}</pre>", escape(text)),
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
pub(crate) fn blind(heading: &str, what: &str, err: &str) -> String {
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
fn fields_section(
    folded: &BTreeMap<String, String>,
    ages: &FieldAges,
    entity: &EntityId,
    canonical: &str,
) -> String {
    if folded.is_empty() {
        return "<h2>Fields</h2>\n<p>Nothing is recorded on this.</p>\n".to_string();
    }
    // **The newest write first, and a key with no moment after every key that
    // has one**: absence is the store's answer where no moment was observed, and
    // it is not turned into a day here. Ties fall to the key's name, so two
    // readings of one page agree.
    let mut keys: Vec<(&String, &String)> = folded.iter().collect();
    keys.sort_by(|(a, _), (b, _)| {
        ages.keys
            .get(*b)
            .cmp(&ages.keys.get(*a))
            .then_with(|| a.cmp(b))
    });
    let left = keys.len().saturating_sub(FIELDS_SHOWN);
    let mut out = String::from("<h2>Fields</h2>\n<table id=\"fields\">\n");
    for (key, value) in keys.into_iter().take(FIELDS_SHOWN) {
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
    if left > 0 {
        out.push_str(&left_out_line(
            "fields-left-out",
            left,
            "older keys are",
            &format!(
                "`recall` on {} reads every key this holds.",
                escape(entity.as_str())
            ),
        ));
    }
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
pub(crate) async fn answer_view(
    state: &AppState,
    view: &EntityId,
    held: &BTreeMap<String, String>,
    facts: &[Fact],
    sent: &BTreeMap<String, String>,
) -> Result<Answered, String> {
    let asked = graph::asked_by_view(held, facts);
    // **The kind a view selects, when it names one.** A view may narrow in other
    // ways (a parent, a type, an overdue or readiness ask, a filter), so a view
    // without `selects` is not refused here: whether it narrows anything is asked
    // once the view is filled, of the same selection the verb asks it of.
    let selects = asked
        .selects
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let kind = match selects {
        Some(selects) => match kinds::resolve(selects) {
            Ok(kind) => Some(kind),
            // **The resolver's own answer**, which already names the value and the
            // kinds this instance holds. A sentence of this page's own beside it
            // would be a second account of one refusal.
            Err(why) => return Err(cannot_run(&escape(&why.to_string()))),
        },
        None => None,
    };
    // **Readiness is a question about work, and only work answers it** — the
    // refusal the verb gives a view that asks it of another kind.
    if asked.ready.is_some() && kind.is_some_and(|kind| kind != EntityKind::WORK) {
        return Err(cannot_run(
            "it asks what is ready, which only work answers.",
        ));
    }
    // **The server's own day, in the instance's zone.** A browser states no
    // timezone, so the page takes the frame every unzoned read takes: the zone
    // the instance's record holds, and UTC only when it holds none.
    let frame = jojobot_mcp::orientation::instance_zone::unzoned_frame(&*state.memory).await;
    let today = state.clock.today_in(&frame);
    // **The inputs the view declares are a form**, one field to an input, typed
    // as the input is, and prefilled from the URL. **The view is filled where the
    // verb fills it** ([`graph::Asked::filled`]), so a value means the same thing
    // here and there, and a refusal is the verb's own words with the form still
    // on the page. A field submitted empty is an input left out.
    let declared = match view_inputs::parse(asked.inputs.as_deref().unwrap_or("")) {
        Ok(declared) => declared,
        Err(refused) => return Err(cannot_run(&escape(&refused.words))),
    };
    let known = match state.memory.list_entities(None).await {
        Ok(known) => known,
        Err(err) => {
            return Err(blind(
                "What this view answers",
                "the things it names",
                &err.to_string(),
            ));
        }
    };
    let form = inputs_form(&declared, sent, &known);
    // **Only the keys the view declares are read from the URL.** A link carries
    // other keys (a tracking parameter), and a page that refused them would turn
    // every such link into an error.
    let given: BTreeMap<String, String> = sent
        .iter()
        .filter(|(name, _)| declared.iter().any(|input| &input.name == *name))
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let former = state.memory.former_handles().await.unwrap_or_default();
    // **A first visit to a view that needs a value asks for it and runs
    // nothing.** What needs a value is the verb's own answer, asked of the same
    // function: an input that is not optional and carries a default needs none,
    // because the verb fills it in.
    if given.is_empty()
        && view_inputs::bind(view.as_str(), &declared, &given, &known, &former, today).is_err()
    {
        return Err(format!(
            "<h2>What this view answers</h2>\n{form}<p>Fill in the inputs above to ask this \
             view.</p>\n"
        ));
    }
    let (asked, parent) = match asked.filled(view.as_str(), &given, &known, &former, today) {
        Ok(filled) => filled,
        Err(refused) => {
            return Err(format!(
                "<h2>What this view answers</h2>\n{form}<p>This view cannot be run: {}</p>\n",
                escape(&refused.words)
            ));
        }
    };
    // **The type the view names is looked up as the verb looks it up, after the
    // view is filled because an input may name it**, and a name nobody declared
    // is a view that cannot be run, with the form still on the page. **An empty
    // value is a stored value and not a removed key**: the verb resolves it as a
    // name and finds none, so the page does too, and does not list the whole
    // kind.
    let answers_type = match asked.answers_type.as_deref() {
        None => None,
        Some(wanted) => match state.memory.declared_types().await {
            Ok(known) => match DeclaredType::named(&known, wanted) {
                Some(declared) => Some(declared.clone()),
                None => {
                    let why = match wanted.trim() {
                        "" => "its <code>answers_type</code> is empty, which names no type."
                            .to_string(),
                        name => format!(
                            "it asks for things that answer the type <code>{}</code>, and no \
                             type is called that.",
                            escape(name)
                        ),
                    };
                    return Err(format!(
                        "<h2>What this view answers</h2>\n{form}<p>This view cannot be run: \
                         {why}</p>\n"
                    ));
                }
            },
            Err(err) => {
                return Err(blind(
                    "What this view answers",
                    "the declared types",
                    &err.to_string(),
                ));
            }
        },
    };
    // **The question is built and answered where the verb builds and answers
    // it** (rules 444 and 473), so a key the view holds means the same thing on
    // both. The objects are listed and nothing of each is asked for: a view's
    // `shows` says what of each one comes back on a READ, and a directory
    // listing lists.
    let question =
        match view_run::Question::of_view(view, &asked, kind, answers_type, parent, today) {
            Ok(question) => question,
            // **The verb's own refusal, in the verb's own words**, with the form still
            // on the page: a key written `$name` narrows only when its input is sent.
            Err(MemoryError::ViewNarrowsNothing { view }) => {
                return Err(format!(
                    "<h2>What this view answers</h2>\n{form}<p>This view cannot be run: {}</p>\n",
                    escape(&view_run::narrows_nothing_words(&view))
                ));
            }
            Err(why) => return Err(cannot_run(&escape(&why.to_string()))),
        };
    // **The words the operator mapped**, so a status that means `done` here
    // finishes a thing exactly as it does in the verbs. Read only for a view
    // that asks what is owed.
    let carriers = attention::shipped();
    let carriers: Vec<&dyn attention::Carrier> =
        carriers.iter().map(std::convert::AsRef::as_ref).collect();
    let instance = match question.asks.overdue {
        Some(_) => {
            jojobot_mcp::orientation::instance_zone::instance_fields_of(&*state.memory).await
        }
        None => BTreeMap::new(),
    };
    let context = view_run::Context {
        carriers: &carriers,
        instance: &instance,
    };
    // **A bot's `working_on` is left out of the walk for a bot whose session is
    // gone**, as a `recall` leaves it out, so a view that selects on it answers
    // the same here.
    let mut question = question;
    question.query.masked = jojobot_mcp::memory::working_on::masked_working_on(
        &jojobot_mcp::memory::working_on::stale_working_on_bots(&*state.memory, &*state.sessions)
            .await,
    );
    let (found, omitted) = match view_run::answer(&*state.memory, &question, &context).await {
        Ok(answered) => (
            answered.selected.objects,
            left_out_lines(
                &answered.narrowed,
                question.asks.ready.as_ref().map(|asked| asked.ask),
            ),
        ),
        Err(view_run::Failed::Selection(err)) => {
            return Err(blind(
                "What this view answers",
                "the records it selects",
                &err.to_string(),
            ));
        }
        Err(view_run::Failed::Readiness(err)) => {
            return Err(blind(
                "What this view answers",
                "the work it depends on",
                &err.to_string(),
            ));
        }
    };
    Ok(Answered {
        form,
        found,
        omitted,
        shown: asked.keys.unwrap_or_default(),
    })
}

/// **A view, run**: what it found, and what the page around the answer needs.
/// The page of a view renders it as a table; the org chart folds it into a tree.
pub(crate) struct Answered {
    form: String,
    /// The objects the view selected, each with the keys the view shows.
    pub(crate) found: Vec<graph::Object>,
    omitted: String,
    shown: Vec<String>,
}

/// **The page of a view**: its answer, or the words saying why it has none.
async fn view_section(
    state: &AppState,
    view: &EntityId,
    held: &BTreeMap<String, String>,
    facts: &[Fact],
    by_id: &HashMap<&EntityId, &Entity>,
    sent: &BTreeMap<String, String>,
) -> String {
    match answer_view(state, view, held, facts, sent).await {
        Ok(answer) => answer_table(&answer, by_id),
        Err(page) => page,
    }
}

fn answer_table(answer: &Answered, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let Answered {
        form,
        found,
        omitted,
        shown,
    } = answer;
    if found.is_empty() {
        return format!(
            "<h2>What this view answers</h2>\n{form}\
             <p>This view matches nothing today. The question is a good one; \
             nothing here answers it.</p>\n{omitted}"
        );
    }
    // **The keys the view says it shows**, a column each, as `recall` narrows an
    // object's fields to them. A key an object does not hold is an empty cell.
    let mut out = format!(
        "<h2>What this view answers</h2>\n{form}<table id=\"answer\">\n\
         <tr><th>Name</th><th>Called</th>"
    );
    for key in shown {
        out.push_str(&format!("<th>{}</th>", escape(key)));
    }
    out.push_str("</tr>\n");
    for object in found {
        let cells: Vec<String> = shown
            .iter()
            .map(|key| escape(object.fields.get(key).map_or("", String::as_str)))
            .collect();
        out.push_str(&row_with(&object.entity, by_id, &cells));
    }
    out.push_str("</table>\n");
    out.push_str(omitted);
    out
}

/// **The lines a view's page ends with when its questions left things out**, one
/// for what is owed and one for what is ready. The count and the call that lists
/// what was left out are the ones `recall` serves, built by the same functions.
fn left_out_lines(narrowed: &view_run::Narrowed, ask: Option<readiness::Ask>) -> String {
    let mut out = String::new();
    let because = narrowed
        .owed
        .as_ref()
        .and_then(|pass| left_out::overdue_excluded_because(&pass.excluded_because));
    if let Some(note) = because {
        let reasons = note["reasons"].as_object().expect("reasons are an object");
        let total: u64 = reasons
            .values()
            .map(|reason| reason["count"].as_u64().unwrap_or(0))
            .sum();
        let by_reason: Vec<String> = reasons
            .iter()
            .map(|(why, reason)| format!("{why} {}", reason["count"]))
            .collect();
        out.push_str(&left_out_line(
            "left-out-owed",
            usize::try_from(total).unwrap_or(usize::MAX),
            "things",
            &format!(
                "By reason: {}. {}",
                escape(&by_reason.join(", ")),
                escape(note["how_to_read"].as_str().unwrap_or_default())
            ),
        ));
    }
    let left = narrowed.ready.as_ref().map(|pass| pass.left_out);
    if let Some(note) = left.and_then(|left| left_out::ready_left_out(left, ask)) {
        out.push_str(&left_out_line(
            "left-out-ready",
            usize::try_from(note["count"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX),
            "pieces of work",
            &escape(note["how_to_read"].as_str().unwrap_or_default()),
        ));
    }
    out
}

/// **The form a view's inputs make**: one labelled field to an input, named as the
/// input is named, and typed as it is: a handle is text offering the things of that
/// kind, a day is a date (kept as text when the URL holds a relative day, which a
/// date field cannot show), a number is a number, and words are text. A field is
/// required when its input is, and holds what the URL sent. Nothing for a view that
/// takes no inputs.
fn inputs_form(
    declared: &[view_inputs::InputDecl],
    sent: &BTreeMap<String, String>,
    known: &[Entity],
) -> String {
    if declared.is_empty() {
        return String::new();
    }
    let mut out = String::from("<form id=\"inputs\" method=\"get\" action=\"\">\n");
    for input in declared {
        let value = sent.get(&input.name).map_or("", String::as_str);
        let (kind, spelled, list) = match &input.ty {
            view_inputs::InputType::Kind(kind) => {
                let id = format!("inputs-{}", input.name);
                let mut options: Vec<&str> = known
                    .iter()
                    .filter(|entity| entity.id.kind().is_some_and(|k| k.as_token() == kind))
                    .map(|entity| entity.id.as_str())
                    .collect();
                options.sort_unstable();
                let options: String = options
                    .iter()
                    .map(|handle| format!("<option value=\"{}\">", escape(handle)))
                    .collect();
                (
                    "text",
                    format!("a {} (kind:slug)", escape(kind)),
                    Some((id, options)),
                )
            }
            view_inputs::InputType::Date => (
                if value.is_empty() || value.parse::<jiff::civil::Date>().is_ok() {
                    "date"
                } else {
                    "text"
                },
                "a day".to_string(),
                None,
            ),
            view_inputs::InputType::Number => ("number", "a number".to_string(), None),
            view_inputs::InputType::Text => ("text", "words".to_string(), None),
        };
        out.push_str(&format!(
            "<p><label>{name} <small>({spelled}{optional})</small> <input type=\"{kind}\" \
             name=\"{name}\"{required}{step}{list_attr} value=\"{value}\"></label>{datalist}</p>\n",
            name = escape(&input.name),
            optional = if input.optional { ", optional" } else { "" },
            required = if input.optional || input.default.is_some() {
                ""
            } else {
                " required"
            },
            step = if kind == "number" {
                " step=\"any\""
            } else {
                ""
            },
            list_attr = list
                .as_ref()
                .map_or(String::new(), |(id, _)| format!(" list=\"{id}\"")),
            value = escape(value),
            datalist = list.as_ref().map_or(String::new(), |(id, options)| format!(
                "<datalist id=\"{id}\">{options}</datalist>"
            )),
        ));
    }
    out.push_str("<p><button type=\"submit\">Ask</button></p>\n</form>\n");
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
/// keys it holds, the keys it lacks and the keys it holds badly.
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
        // **A key the thing holds badly is named with the value it holds.**
        // `Holds` lists every key that is there, the wrong ones included, so
        // the verdict alone would say "partly" and leave the reader to find
        // which key.
        let wrong: Vec<String> = found
            .mistyped
            .iter()
            .map(|bad| format!("{}: {}", bad.key, bad.value))
            .collect();
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape(&declaration.name),
            if found.complete() { "whole" } else { "partly" },
            escape(&found.held.join(", ")),
            escape(&found.lacking.join(", ")),
            escape(&wrong.join(", ")),
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    format!(
        "<h2>Conforms to</h2>\n<table id=\"conforms\">\n\
         <tr><th>Type</th><th>How far</th><th>Holds</th><th>Lacks</th><th>Wrong</th></tr>\n{rows}</table>\n"
    )
}

/// The facts held at this node.
///
/// **Every claim arrives with what qualifies it.** Who backs it and how settled
/// it is are what tell a claim from a hypothesis, so they are columns rather
/// than something a reader has to go and ask for.
fn facts_table(
    facts: &[Fact],
    by_id: &HashMap<&EntityId, &Entity>,
    working_on_stale: bool,
) -> String {
    if facts.is_empty() {
        return "<h2>Facts</h2>\n<p>Nothing is recorded here.</p>\n".to_string();
    }

    let mut out = String::from(
        "<h2>Facts</h2>\n<table>\n<tr><th>Claim</th><th>Backed by</th><th>Standing</th>\
         <th>State</th><th>Date</th><th>Relation</th><th>Address</th></tr>\n",
    );
    for fact in facts {
        // **Content and details are read as one text for what is code**, so a
        // handle inside a fence that opens on the content line is not a link.
        let (in_content, in_details) =
            mention::followable_words(&fact.content, fact.details.as_deref());
        let claim = match &fact.details {
            // **The claim leads and its details sit behind a ▸**, collapsed until the
            // reader opens them, so a page of records reads as a list of claims.
            Some(details) if !details.trim().is_empty() => format!(
                "{}{}",
                linkify_spans(&fact.content, in_content, by_id),
                shell::fold(
                    "",
                    &format!(
                        "<small>{}</small>",
                        line_breaks(&linkify_spans(details, in_details, by_id))
                    )
                )
            ),
            _ => linkify_spans(&fact.content, in_content, by_id),
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
        out.push_str(&record_fields(fact, working_on_stale));
    }
    out.push_str("</table>\n");
    out
}

/// **The keys this one record carries**, on lines of their own under it: its own
/// fields, then what it set on its thing.
///
/// Not an eighth column. The seven columns are each one value and each present
/// on every row; a record's keys are a map of any size and are on few rows, so a
/// column would be empty almost everywhere and would push the address out of a
/// readable width when it was not. A line under the row keeps the columns
/// meaning what they say and puts the keys where the record is.
///
/// **Two lines, as every served read splits them.** A key the record set on its
/// thing is a property of the thing, and a key it kept as its own is a label on
/// this claim; one line for both let a reader take a label for a property.
///
/// Nothing for a record carrying no key, which is most of them, and no line for
/// a bag that holds none.
fn record_fields(fact: &Fact, working_on_stale: bool) -> String {
    let mut own: Vec<String> = Vec::new();
    let mut set: Vec<String> = Vec::new();
    for (key, value) in &fact.fields {
        let mut pair = format!("{} = {}", escape(key), escape(value));
        // **A `working_on` whose session is gone is marked as `recall` marks it**,
        // so the value a record still carries as history is not read as what the
        // bot is doing now.
        if working_on_stale
            && key == jojobot_domain::memory::kinds::WORKING_ON
            && fact.sets.contains(key)
        {
            pair.push_str(" (working_on_stale)");
        }
        if fact.sets.contains(key) {
            set.push(pair);
        } else {
            own.push(pair);
        }
    }
    let line = |class: &str, label: &str, pairs: &[String]| {
        if pairs.is_empty() {
            return String::new();
        }
        format!(
            "<tr class=\"{class}\"><td colspan=\"7\"><small>{label}: {}</small></td></tr>\n",
            pairs.join(" · ")
        )
    };
    format!(
        "{}{}",
        line("fields", "fields", &own),
        line("sets", "sets", &set)
    )
}

/// Details may hold paragraph breaks, and HTML collapses a newline to a space.
/// Each break becomes a `<br>`, so a blank line between paragraphs reads as one;
/// a `\r` is dropped so a Windows break is one break and not two.
fn line_breaks(escaped: &str) -> String {
    escaped.replace('\r', "").replace('\n', "<br>\n")
}

/// **A mention in served text becomes a link to that thing's own page.**
///
/// The text a reader sees is unchanged — today's handle, exactly as
/// `mention::rendered` already resolved it — and a followable span becomes
/// an anchor. **The two shapes that cannot be followed stay exactly as
/// served, never as a dead link**: `mention::followable` is the one place
/// that tells them apart, so this never has to.
pub(crate) fn linkify(text: &str, by_id: &HashMap<&EntityId, &Entity>) -> String {
    linkify_spans(text, mention::followable(text), by_id)
}

/// [`linkify`] over spans the caller already found, for text whose code is
/// decided beside other text.
fn linkify_spans(
    text: &str,
    spans: mention::Followed,
    by_id: &HashMap<&EntityId, &Entity>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cut = 0;
    for (span, handle) in spans {
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
    shell::page(
        title,
        "",
        &format!("<h1>{}</h1>\n<p>{}</p>\n", escape(title), escape(message)),
    )
}

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

pub(crate) fn html(body: &str) -> Response {
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

    /// **Every name a view cannot give an input is a key the page reads for
    /// itself**, so a link carrying it reaches the page and never an input.
    /// **Paired**: a name that is not reserved reaches the inputs, so the list is
    /// the page's own keys and not every key.
    #[test]
    fn the_names_an_input_cannot_have_are_the_pages_own_keys() {
        use jojobot_domain::memory::view_inputs::RESERVED_NAMES;
        let read = |key: &str| {
            axum::extract::Query::<super::NodeQuery>::try_from_uri(
                &format!("/view:ready/?{key}=x").parse().expect("a uri"),
            )
            .expect("a query")
            .0
        };
        for reserved in RESERVED_NAMES {
            assert!(
                read(reserved).inputs.is_empty(),
                "{reserved} is reserved but the page does not take it for itself"
            );
        }
        assert_eq!(read("project").inputs.len(), 1);
        assert_eq!(read("history").history.as_deref(), Some("x"));
    }
}
