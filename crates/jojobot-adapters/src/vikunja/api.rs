//! The Vikunja HTTP surface, behind a port.
//!
//! Isolating the raw REST calls behind [`VikunjaApi`] lets the store's whole
//! logic — discover, self-provision, screen, place, move, verify — run under a
//! fast in-memory double with no network. The real HTTP mapping is the only
//! thing left to the gated integration test.
//!
//! Records carry `created` because the store reconciles a concurrent
//! double-create by picking the **oldest** as canonical, and `project_id` on a
//! task because the write-scope invariant is checked against it before any card
//! is rewritten.
//!
//! The endpoints are Vikunja's v1 API. Buckets live under a project's **views**
//! (`/projects/{p}/views/{v}/buckets`), and the board endpoint
//! (`/projects/{p}/views/{v}/tasks`) returns buckets **with their tasks** — one
//! call for placement and content together, which is why the store reads state
//! through it rather than through per-bucket task lists.

use async_trait::async_trait;
use serde_json::{Value, json};

use jojobot_domain::mailbox::MailboxError;

use super::Secret;

/// A project as the store needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectRec {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub created: String,
}

/// One of a project's views. Only the kanban view carries buckets, and buckets
/// are where state lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewRec {
    pub id: u64,
    /// Vikunja's `view_kind`: `list` · `gantt` · `table` · `kanban`.
    pub kind: String,
}

/// A column on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BucketRec {
    pub id: u64,
    pub title: String,
}

/// A card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskRec {
    pub id: u64,
    /// Which project it belongs to — checked against the mailbox project before
    /// any write, so a card outside jojobot's project can never be rewritten.
    pub project_id: u64,
    pub title: String,
    pub description: String,
    /// The label titles on the card. A mailbox IS a label, so this is where a
    /// message's box is read from.
    pub labels: Vec<String>,
}

/// A column together with the cards in it — the board endpoint's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoardBucket {
    pub id: u64,
    pub title: String,
    pub tasks: Vec<TaskRec>,
}

/// A label. Global in Vikunja — labels are not scoped to a project — which is
/// why jojobot's mailbox labels carry both a title prefix and an owner tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LabelRec {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub created: String,
}

/// The Vikunja operations the store depends on. One real adapter
/// ([`HttpVikunja`]); a test double lives in the store's test module.
///
/// **Every project-scoped method takes its project id explicitly**, and every
/// caller inside the store obtains that id from one place — the resolved
/// mailbox project. That is the seam the write-scope invariant is checked at.
#[async_trait]
pub(super) trait VikunjaApi: Send + Sync {
    async fn list_projects(&self, page: u64, per_page: u64) -> Result<Vec<ProjectRec>, MailboxError>;
    async fn create_project(
        &self,
        title: &str,
        description: &str,
    ) -> Result<ProjectRec, MailboxError>;
    async fn list_views(&self, project_id: u64) -> Result<Vec<ViewRec>, MailboxError>;
    async fn list_buckets(
        &self,
        project_id: u64,
        view_id: u64,
    ) -> Result<Vec<BucketRec>, MailboxError>;
    async fn create_bucket(
        &self,
        project_id: u64,
        view_id: u64,
        title: &str,
    ) -> Result<BucketRec, MailboxError>;
    /// The board: every column with the cards in it.
    async fn board(
        &self,
        project_id: u64,
        view_id: u64,
        page: u64,
        per_page: u64,
    ) -> Result<Vec<BoardBucket>, MailboxError>;
    async fn create_task(
        &self,
        project_id: u64,
        title: &str,
        description: &str,
    ) -> Result<TaskRec, MailboxError>;
    /// Rewrite a card's title and description. Vikunja's task update takes the
    /// task model, so the project id rides along — these are jojobot's own
    /// cards, which carry no due date, assignee, or reminder to preserve.
    async fn update_task(
        &self,
        project_id: u64,
        task_id: u64,
        title: &str,
        description: &str,
    ) -> Result<(), MailboxError>;
    async fn delete_task(&self, task_id: u64) -> Result<(), MailboxError>;
    async fn move_task(
        &self,
        project_id: u64,
        view_id: u64,
        bucket_id: u64,
        task_id: u64,
    ) -> Result<(), MailboxError>;
    async fn list_labels(&self, page: u64, per_page: u64) -> Result<Vec<LabelRec>, MailboxError>;
    async fn create_label(&self, title: &str, description: &str)
    -> Result<LabelRec, MailboxError>;
    /// Replace the whole label set on a card.
    async fn set_task_labels(&self, task_id: u64, labels: &[u64]) -> Result<(), MailboxError>;
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn project_rec(p: &Value) -> Option<ProjectRec> {
    Some(ProjectRec {
        id: as_u64(&p["id"])?,
        title: text(&p["title"]),
        description: text(&p["description"]),
        created: text(&p["created"]),
    })
}

fn view_rec(v: &Value) -> Option<ViewRec> {
    Some(ViewRec {
        id: as_u64(&v["id"])?,
        kind: text(&v["view_kind"]),
    })
}

fn bucket_rec(b: &Value) -> Option<BucketRec> {
    Some(BucketRec {
        id: as_u64(&b["id"])?,
        title: text(&b["title"]),
    })
}

fn task_rec(t: &Value) -> Option<TaskRec> {
    Some(TaskRec {
        id: as_u64(&t["id"])?,
        project_id: as_u64(&t["project_id"]).unwrap_or_default(),
        title: text(&t["title"]),
        description: text(&t["description"]),
        // A card with no labels comes back with a null, not an empty array.
        labels: t["labels"]
            .as_array()
            .map(|ls| ls.iter().map(|l| text(&l["title"])).collect())
            .unwrap_or_default(),
    })
}

fn board_bucket(b: &Value) -> Option<BoardBucket> {
    Some(BoardBucket {
        id: as_u64(&b["id"])?,
        title: text(&b["title"]),
        tasks: b["tasks"]
            .as_array()
            .map(|ts| ts.iter().filter_map(task_rec).collect())
            .unwrap_or_default(),
    })
}

fn label_rec(l: &Value) -> Option<LabelRec> {
    Some(LabelRec {
        id: as_u64(&l["id"])?,
        title: text(&l["title"]),
        description: text(&l["description"]),
        created: text(&l["created"]),
    })
}

/// The real adapter: a thin REST client over Vikunja's v1 API.
pub(super) struct HttpVikunja {
    http: reqwest::Client,
    /// Vikunja's root, e.g. `https://tasks.example.org` — **not** including
    /// `/api/v1`, which this client appends.
    base_url: String,
    token: Secret,
}

impl HttpVikunja {
    pub(super) fn new(http: reqwest::Client, base_url: String, token: Secret) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    /// Issue a request and return the parsed JSON body. The central place for
    /// auth and error mapping — no `unwrap` on any path.
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, MailboxError> {
        // The query string is assembled here rather than through reqwest's
        // `query`, which the workspace's default-features-off build does not
        // carry. Every value is a plain integer, so there is nothing to escape.
        let mut url = self.url(path);
        if !query.is_empty() {
            let pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
            url.push('?');
            url.push_str(&pairs.join("&"));
        }
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(self.token.expose());
        if let Some(body) = body {
            request = request.json(&body);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| MailboxError::Store(format!("{path} request: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(MailboxError::Store(format!("{path} returned {status}")));
        }
        // A DELETE answers with a message object, and some endpoints answer with
        // an empty body; neither is an error, and neither is read.
        let raw = resp
            .text()
            .await
            .map_err(|e| MailboxError::Store(format!("{path} body: {e}")))?;
        if raw.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&raw)
            .map_err(|e| MailboxError::Store(format!("{path} body: {e}")))
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, MailboxError> {
        self.send(reqwest::Method::GET, path, query, None).await
    }

    async fn put(&self, path: &str, body: Value) -> Result<Value, MailboxError> {
        self.send(reqwest::Method::PUT, path, &[], Some(body)).await
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, MailboxError> {
        self.send(reqwest::Method::POST, path, &[], Some(body)).await
    }

    fn array<'a>(v: &'a Value, path: &str) -> Result<&'a Vec<Value>, MailboxError> {
        v.as_array()
            .ok_or_else(|| MailboxError::Store(format!("{path}: expected a list")))
    }

    fn paging(page: u64, per_page: u64) -> Vec<(&'static str, String)> {
        vec![
            ("page", page.to_string()),
            ("per_page", per_page.to_string()),
        ]
    }
}

#[async_trait]
impl VikunjaApi for HttpVikunja {
    async fn list_projects(
        &self,
        page: u64,
        per_page: u64,
    ) -> Result<Vec<ProjectRec>, MailboxError> {
        let v = self.get("/projects", &Self::paging(page, per_page)).await?;
        Ok(Self::array(&v, "/projects")?
            .iter()
            .filter_map(project_rec)
            .collect())
    }

    async fn create_project(
        &self,
        title: &str,
        description: &str,
    ) -> Result<ProjectRec, MailboxError> {
        let v = self
            .put("/projects", json!({ "title": title, "description": description }))
            .await?;
        project_rec(&v).ok_or_else(|| MailboxError::Store("projects.create: malformed".into()))
    }

    async fn list_views(&self, project_id: u64) -> Result<Vec<ViewRec>, MailboxError> {
        let path = format!("/projects/{project_id}/views");
        let v = self.get(&path, &[]).await?;
        Ok(Self::array(&v, &path)?.iter().filter_map(view_rec).collect())
    }

    async fn list_buckets(
        &self,
        project_id: u64,
        view_id: u64,
    ) -> Result<Vec<BucketRec>, MailboxError> {
        let path = format!("/projects/{project_id}/views/{view_id}/buckets");
        let v = self.get(&path, &[]).await?;
        Ok(Self::array(&v, &path)?.iter().filter_map(bucket_rec).collect())
    }

    async fn create_bucket(
        &self,
        project_id: u64,
        view_id: u64,
        title: &str,
    ) -> Result<BucketRec, MailboxError> {
        let path = format!("/projects/{project_id}/views/{view_id}/buckets");
        let v = self.put(&path, json!({ "title": title })).await?;
        bucket_rec(&v).ok_or_else(|| MailboxError::Store("buckets.create: malformed".into()))
    }

    async fn board(
        &self,
        project_id: u64,
        view_id: u64,
        page: u64,
        per_page: u64,
    ) -> Result<Vec<BoardBucket>, MailboxError> {
        let path = format!("/projects/{project_id}/views/{view_id}/tasks");
        let v = self.get(&path, &Self::paging(page, per_page)).await?;
        Ok(Self::array(&v, &path)?
            .iter()
            .filter_map(board_bucket)
            .collect())
    }

    async fn create_task(
        &self,
        project_id: u64,
        title: &str,
        description: &str,
    ) -> Result<TaskRec, MailboxError> {
        let path = format!("/projects/{project_id}/tasks");
        let v = self
            .put(&path, json!({ "title": title, "description": description }))
            .await?;
        task_rec(&v).ok_or_else(|| MailboxError::Store("tasks.create: malformed".into()))
    }

    async fn update_task(
        &self,
        project_id: u64,
        task_id: u64,
        title: &str,
        description: &str,
    ) -> Result<(), MailboxError> {
        self.post(
            &format!("/tasks/{task_id}"),
            json!({
                "id": task_id,
                "project_id": project_id,
                "title": title,
                "description": description,
            }),
        )
        .await
        .map(|_| ())
    }

    async fn delete_task(&self, task_id: u64) -> Result<(), MailboxError> {
        self.send(
            reqwest::Method::DELETE,
            &format!("/tasks/{task_id}"),
            &[],
            None,
        )
        .await
        .map(|_| ())
    }

    async fn move_task(
        &self,
        project_id: u64,
        view_id: u64,
        bucket_id: u64,
        task_id: u64,
    ) -> Result<(), MailboxError> {
        self.post(
            &format!("/projects/{project_id}/views/{view_id}/buckets/{bucket_id}/tasks"),
            json!({
                "task_id": task_id,
                "bucket_id": bucket_id,
                "project_view_id": view_id,
            }),
        )
        .await
        .map(|_| ())
    }

    async fn list_labels(&self, page: u64, per_page: u64) -> Result<Vec<LabelRec>, MailboxError> {
        let v = self.get("/labels", &Self::paging(page, per_page)).await?;
        Ok(Self::array(&v, "/labels")?
            .iter()
            .filter_map(label_rec)
            .collect())
    }

    async fn create_label(
        &self,
        title: &str,
        description: &str,
    ) -> Result<LabelRec, MailboxError> {
        let v = self
            .put("/labels", json!({ "title": title, "description": description }))
            .await?;
        label_rec(&v).ok_or_else(|| MailboxError::Store("labels.create: malformed".into()))
    }

    async fn set_task_labels(&self, task_id: u64, labels: &[u64]) -> Result<(), MailboxError> {
        self.post(
            &format!("/tasks/{task_id}/labels/bulk"),
            json!({ "labels": labels.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>() }),
        )
        .await
        .map(|_| ())
    }
}

/// An adapter with no credentials — every call refuses. Lets the server boot and
/// serve the other contexts before Vikunja is wired, without shipping a toy
/// store.
pub(super) struct Unconfigured;

impl Unconfigured {
    fn refuse<T>() -> Result<T, MailboxError> {
        Err(MailboxError::NotConfigured(
            "set JOJOBOT_VIKUNJA_URL and JOJOBOT_VIKUNJA_TOKEN".into(),
        ))
    }
}

#[async_trait]
impl VikunjaApi for Unconfigured {
    async fn list_projects(&self, _: u64, _: u64) -> Result<Vec<ProjectRec>, MailboxError> {
        Self::refuse()
    }
    async fn create_project(&self, _: &str, _: &str) -> Result<ProjectRec, MailboxError> {
        Self::refuse()
    }
    async fn list_views(&self, _: u64) -> Result<Vec<ViewRec>, MailboxError> {
        Self::refuse()
    }
    async fn list_buckets(&self, _: u64, _: u64) -> Result<Vec<BucketRec>, MailboxError> {
        Self::refuse()
    }
    async fn create_bucket(&self, _: u64, _: u64, _: &str) -> Result<BucketRec, MailboxError> {
        Self::refuse()
    }
    async fn board(&self, _: u64, _: u64, _: u64, _: u64) -> Result<Vec<BoardBucket>, MailboxError> {
        Self::refuse()
    }
    async fn create_task(&self, _: u64, _: &str, _: &str) -> Result<TaskRec, MailboxError> {
        Self::refuse()
    }
    async fn update_task(&self, _: u64, _: u64, _: &str, _: &str) -> Result<(), MailboxError> {
        Self::refuse()
    }
    async fn delete_task(&self, _: u64) -> Result<(), MailboxError> {
        Self::refuse()
    }
    async fn move_task(&self, _: u64, _: u64, _: u64, _: u64) -> Result<(), MailboxError> {
        Self::refuse()
    }
    async fn list_labels(&self, _: u64, _: u64) -> Result<Vec<LabelRec>, MailboxError> {
        Self::refuse()
    }
    async fn create_label(&self, _: &str, _: &str) -> Result<LabelRec, MailboxError> {
        Self::refuse()
    }
    async fn set_task_labels(&self, _: u64, _: &[u64]) -> Result<(), MailboxError> {
        Self::refuse()
    }
}
