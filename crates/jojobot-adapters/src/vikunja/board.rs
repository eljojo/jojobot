//! Finding and provisioning one of jojobot's own kanban boards.
//!
//! Two bounded contexts keep their records on Vikunja — Mailboxes and Sessions
//! — in **two different projects**, and neither may touch the other's or the
//! operator's. Everything about getting from a project *title* to a scope you
//! are allowed to write in is the same for both, so it is written once here:
//! adopt-or-create by title, nest a new board under jojobot's home, converge a
//! concurrent double-create on the oldest, find the kanban view, provision the
//! columns, point the done flag at the right one.
//!
//! What differs between the two is only **which columns**, and that is the
//! argument. The invariant this file exists to hold is that a [`Scope`] can be
//! minted no other way: a call path cannot name a project without having
//! discovered it as jojobot's own.

use jojobot_domain::mailbox::MailboxError;

use super::api::{ProjectRec, TaskRec, VikunjaApi};

/// How many records a list call asks for per page.
pub(super) const PAGE: u64 = 50;

/// The marker jojobot stamps into the description of everything it creates —
/// every project and every mailbox label — and checks on match, so it only ever
/// adopts something it created itself.
pub(super) const OWNER_TAG: &str = "[jojobot:owned]";

/// jojobot's home: the project a NEW board is created under, by name convention
/// (the operator's call, 2026-07-26 — a board belongs inside the `jojobot`
/// project, not beside it). An existing project with this title is adopted as
/// the home; **when none exists jojobot creates it for itself**. jojobot never
/// writes INTO the home — no cards, no labels, no edits to its record.
pub(super) const PARENT_PROJECT: &str = "jojobot";

/// The one project a store may touch, and the kanban view its columns live in.
///
/// Minted only by [`Provisioner::resolve`], which means no call path can name a
/// project without having discovered it as jojobot's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Scope {
    project: u64,
    pub view: u64,
}

impl Scope {
    /// The project id, for a call that is scoped to it by construction.
    pub fn project(&self) -> u64 {
        self.project
    }

    /// Whether a card belongs to this scope's project.
    ///
    /// **This is the sharp edge of the invariant.** A card id is global in
    /// Vikunja: `POST /tasks/{id}` reaches any card the token can see, the
    /// operator's boards included, and the update carries a `project_id` — so
    /// writing to a card jojobot does not own does not merely edit it, it
    /// *moves* it onto jojobot's board. A card that turns up in jojobot's
    /// columns while declaring another project is an integrity violation, not
    /// routine noise, and every verb refuses rather than degrading around it.
    ///
    /// The refusal itself is each context's to phrase — they name different
    /// projects and answer with different error types — so this returns the
    /// judgement, not the error.
    pub fn owns(&self, task: &TaskRec) -> bool {
        task.project_id == self.project
    }
}

/// Adopt-or-create one of jojobot's boards, by title.
pub(super) struct Provisioner<'a> {
    /// The transport.
    pub api: &'a dyn VikunjaApi,
    /// The project title this store manages.
    pub project: &'a str,
    /// The columns the board must carry, in funnel order.
    pub columns: &'a [&'a str],
    /// The column the view's done flag points at, if any.
    ///
    /// For Mailboxes that is `processed`, and for Sessions `wrapped`: in both
    /// contexts it is the column that means "finished the way it was meant to",
    /// so the operator's UI and jojobot's archive agree, and a card the operator
    /// ticks done lands somewhere that reads as finished.
    pub done: Option<&'a str>,
}

impl Provisioner<'_> {
    /// The scope every other call runs inside. Idempotent — it provisions
    /// whatever is missing and adopts whatever is already there.
    pub async fn resolve(&self) -> Result<Scope, MailboxError> {
        let project = self.project().await?;
        let view = self
            .api
            .list_views(project)
            .await?
            .into_iter()
            .filter(|v| v.kind == "kanban")
            .min_by_key(|v| v.id)
            .ok_or_else(|| {
                MailboxError::Store(format!(
                    "project {project} has no kanban view — columns are where state lives"
                ))
            })?;
        let scope = Scope { project, view: view.id };
        self.ensure_columns(&scope).await?;
        // A fresh board ships the done flag pointing at its default column, so
        // this cannot be skipped on the assumption it starts unset.
        if let Some(done) = self.done {
            let bucket = self.column(&scope, done).await?;
            if view.done_bucket_id != bucket {
                self.api.set_view_done_bucket(project, &view, bucket).await?;
            }
        }
        Ok(scope)
    }

    /// The bucket id for a column title, on this board.
    pub async fn column(&self, scope: &Scope, title: &str) -> Result<u64, MailboxError> {
        self.api
            .list_buckets(scope.project(), scope.view)
            .await?
            .into_iter()
            .find(|b| b.title == title)
            .map(|b| b.id)
            .ok_or_else(|| MailboxError::Store(format!("the board has no '{title}' column")))
    }

    /// Every project, paged in full.
    ///
    /// **Stops on an empty page, never on a short one.** Vikunja clamps
    /// `per_page` server-side to its own `maxitemsperpage`, which can sit below
    /// the request, and a loop that stops there reads page one and concludes the
    /// project is absent.
    async fn all_projects(&self) -> Result<Vec<ProjectRec>, MailboxError> {
        let mut all = Vec::new();
        let mut page = 1;
        loop {
            let batch = self.api.list_projects(page, PAGE).await?;
            if batch.is_empty() {
                break;
            }
            all.extend(batch);
            page += 1;
        }
        Ok(all)
    }

    /// The projects that are both named ours AND carry the ownership tag.
    fn owned_of(&self, all: &[ProjectRec]) -> Vec<ProjectRec> {
        all.iter()
            .filter(|p| p.title == self.project && p.description.contains(OWNER_TAG))
            .cloned()
            .collect()
    }

    /// The project a NEW board is created under — jojobot's home, by name
    /// convention.
    ///
    /// A store whose own board is named like the home gets no parent at all:
    /// that board IS the home, and a home cannot nest under itself.
    async fn parent(&self, all: &[ProjectRec]) -> Result<Option<u64>, MailboxError> {
        if self.project == PARENT_PROJECT {
            return Ok(None);
        }
        if let Some(p) = oldest(
            all.iter()
                .filter(|p| p.title == PARENT_PROJECT)
                .cloned()
                .collect::<Vec<_>>(),
            |p| (p.created.as_str(), p.id),
        ) {
            return Ok(Some(p.id));
        }
        let created = self
            .api
            .create_project(PARENT_PROJECT, &format!("jojobot's home. {OWNER_TAG}"), None)
            .await?;
        Ok(Some(created.id))
    }

    /// This board's project id, creating it if absent. After a create it
    /// re-lists and picks the canonical (oldest) owned project, so a concurrent
    /// double-create converges on one rather than forking.
    ///
    /// A NEW board is created under jojobot's home. An existing board is adopted
    /// exactly where it stands — there is no re-homing (the operator's call,
    /// 2026-07-26): where a board sits is the operator's to arrange, and jojobot
    /// only decides where one is BORN.
    async fn project(&self) -> Result<u64, MailboxError> {
        let all = self.all_projects().await?;
        if let Some(p) = oldest(self.owned_of(&all), |p| (p.created.as_str(), p.id)) {
            return Ok(p.id);
        }
        let parent = self.parent(&all).await?;
        self.api
            .create_project(self.project, &owner_description(), parent)
            .await?;
        let relisted = self.all_projects().await?;
        oldest(self.owned_of(&relisted), |p| (p.created.as_str(), p.id))
            .map(|p| p.id)
            .ok_or_else(|| {
                MailboxError::Store(format!("project '{}' missing after create", self.project))
            })
    }

    /// Make sure the board carries one column per state. Missing ones are
    /// created in funnel order; anything else on the board is left alone.
    async fn ensure_columns(&self, scope: &Scope) -> Result<(), MailboxError> {
        let existing = self.api.list_buckets(scope.project(), scope.view).await?;
        for title in self.columns {
            if !existing.iter().any(|b| &b.title == title) {
                self.api
                    .create_bucket(scope.project(), scope.view, title)
                    .await?;
            }
        }
        Ok(())
    }
}

/// What jojobot writes into the description of a project it owns.
pub(super) fn owner_description() -> String {
    format!("Managed by jojobot — do not edit by hand. {OWNER_TAG}")
}

/// The deterministic canonical winner: oldest by the record's own creation
/// stamp, ties broken by id. Both are stable across list calls, so every session
/// agrees on which one is canonical.
pub(super) fn oldest<T>(mut items: Vec<T>, key: impl Fn(&T) -> (&str, u64)) -> Option<T> {
    items.sort_by(|a, b| key(a).cmp(&key(b)));
    items.into_iter().next()
}
