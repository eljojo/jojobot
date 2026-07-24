//! The Outline HTTP surface, behind a port. Isolating the raw REST calls behind
//! [`OutlineApi`] lets the store's provisioning logic (discover, create,
//! reconcile, paginate, marker-lookup) run under a fast in-memory double with no
//! network — the real HTTP mapping is the only thing left to the gated
//! integration test.
//!
//! Records carry `created_at` because the store reconciles a concurrent
//! double-create by picking the **oldest** as canonical; and `text`, because a
//! doc's entity is resolved from its embedded marker, read straight off the list
//! response (Outline's `documents.list` returns text, and a list is immediately
//! consistent — unlike the lagging search index).

use async_trait::async_trait;
use serde_json::{Value, json};

use jojobot_domain::memory::MemoryError;

use super::Secret;

/// A collection as the store needs it.
#[derive(Debug, Clone)]
pub(super) struct CollectionRec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

/// A document as the store needs it — including `text`, so a marker can be read
/// without a second round-trip.
#[derive(Debug, Clone)]
pub(super) struct DocRec {
    pub id: String,
    /// The doc's title. Deliberately NOT used to resolve an entity — that's the
    /// marker's job — because users rename titles. Kept to mirror the API and
    /// because the rename-safety tests manipulate it; hence the allow.
    #[allow(dead_code)]
    pub title: String,
    pub text: String,
    pub created_at: String,
}

/// The Outline operations the store depends on. One real adapter
/// ([`HttpOutline`]); a test double lives in the store's test module.
#[async_trait]
pub(super) trait OutlineApi: Send + Sync {
    async fn list_collections(&self, offset: u64, limit: u64)
    -> Result<Vec<CollectionRec>, MemoryError>;
    async fn create_collection(
        &self,
        name: &str,
        description: &str,
    ) -> Result<CollectionRec, MemoryError>;
    async fn list_documents(
        &self,
        collection_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DocRec>, MemoryError>;
    async fn create_document(
        &self,
        collection_id: &str,
        title: &str,
        text: &str,
    ) -> Result<DocRec, MemoryError>;
    async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError>;
}

fn collection_rec(c: &Value) -> Option<CollectionRec> {
    Some(CollectionRec {
        id: c["id"].as_str()?.to_string(),
        name: c["name"].as_str().unwrap_or_default().to_string(),
        description: c["description"].as_str().unwrap_or_default().to_string(),
        created_at: c["createdAt"].as_str().unwrap_or_default().to_string(),
    })
}

fn doc_rec(d: &Value) -> Option<DocRec> {
    Some(DocRec {
        id: d["id"].as_str()?.to_string(),
        title: d["title"].as_str().unwrap_or_default().to_string(),
        text: d["text"].as_str().unwrap_or_default().to_string(),
        created_at: d["createdAt"].as_str().unwrap_or_default().to_string(),
    })
}

/// The real adapter: thin REST client over Outline's HTTP API.
pub(super) struct HttpOutline {
    http: reqwest::Client,
    base_url: String,
    token: Secret,
}

impl HttpOutline {
    pub(super) fn new(http: reqwest::Client, base_url: String, token: Secret) -> Self {
        Self {
            http,
            base_url,
            token,
        }
    }

    /// POST a JSON body to an endpoint and return the parsed JSON. Central place
    /// for auth + error mapping — no `unwrap` on any path.
    async fn post(&self, endpoint: &str, body: Value) -> Result<Value, MemoryError> {
        let resp = self
            .http
            .post(format!("{}/api/{endpoint}", self.base_url))
            .bearer_auth(self.token.expose())
            .json(&body)
            .send()
            .await
            .map_err(|e| MemoryError::Store(format!("{endpoint} request: {e}")))?;
        if !resp.status().is_success() {
            return Err(MemoryError::Store(format!(
                "{endpoint} returned {}",
                resp.status()
            )));
        }
        resp.json()
            .await
            .map_err(|e| MemoryError::Store(format!("{endpoint} body: {e}")))
    }

    fn data_array<'a>(v: &'a Value, endpoint: &str) -> Result<&'a Vec<Value>, MemoryError> {
        v["data"]
            .as_array()
            .ok_or_else(|| MemoryError::Store(format!("{endpoint}: no data array")))
    }
}

#[async_trait]
impl OutlineApi for HttpOutline {
    async fn list_collections(
        &self,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<CollectionRec>, MemoryError> {
        let v = self
            .post("collections.list", json!({ "offset": offset, "limit": limit }))
            .await?;
        Ok(Self::data_array(&v, "collections.list")?
            .iter()
            .filter_map(collection_rec)
            .collect())
    }

    async fn create_collection(
        &self,
        name: &str,
        description: &str,
    ) -> Result<CollectionRec, MemoryError> {
        let v = self
            .post(
                "collections.create",
                json!({ "name": name, "description": description }),
            )
            .await?;
        collection_rec(&v["data"])
            .ok_or_else(|| MemoryError::Store("collections.create: malformed collection".into()))
    }

    async fn list_documents(
        &self,
        collection_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DocRec>, MemoryError> {
        let v = self
            .post(
                "documents.list",
                json!({ "collectionId": collection_id, "offset": offset, "limit": limit }),
            )
            .await?;
        Ok(Self::data_array(&v, "documents.list")?
            .iter()
            .filter_map(doc_rec)
            .collect())
    }

    async fn create_document(
        &self,
        collection_id: &str,
        title: &str,
        text: &str,
    ) -> Result<DocRec, MemoryError> {
        let v = self
            .post(
                "documents.create",
                json!({
                    "collectionId": collection_id,
                    "title": title,
                    "text": text,
                    "publish": true,
                }),
            )
            .await?;
        doc_rec(&v["data"])
            .ok_or_else(|| MemoryError::Store("documents.create: malformed document".into()))
    }

    async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
        self.post("documents.update", json!({ "id": id, "text": text }))
            .await
            .map(|_| ())
    }
}

/// An adapter with no credentials — every call refuses. Lets the server boot and
/// serve `ping` before Outline is wired, without shipping a toy store.
pub(super) struct Unconfigured;

impl Unconfigured {
    fn refuse<T>() -> Result<T, MemoryError> {
        Err(MemoryError::NotConfigured(
            "set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN".into(),
        ))
    }
}

#[async_trait]
impl OutlineApi for Unconfigured {
    async fn list_collections(&self, _: u64, _: u64) -> Result<Vec<CollectionRec>, MemoryError> {
        Self::refuse()
    }
    async fn create_collection(&self, _: &str, _: &str) -> Result<CollectionRec, MemoryError> {
        Self::refuse()
    }
    async fn list_documents(&self, _: &str, _: u64, _: u64) -> Result<Vec<DocRec>, MemoryError> {
        Self::refuse()
    }
    async fn create_document(&self, _: &str, _: &str, _: &str) -> Result<DocRec, MemoryError> {
        Self::refuse()
    }
    async fn update_document(&self, _: &str, _: &str) -> Result<(), MemoryError> {
        Self::refuse()
    }
}
