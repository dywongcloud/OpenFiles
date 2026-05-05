use crate::error::Result;
use crate::types::FileKind;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Range;

mod local;
pub use local::LocalFsBackend;

#[cfg(feature = "opendal-backend")]
mod opendal_backend;
#[cfg(feature = "opendal-backend")]
pub use opendal_backend::OpendalBackend;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub etag: Option<String>,
    pub version: Option<String>,
    pub updated_ns: u128,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub kind: FileKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectVersion {
    pub etag: Option<String>,
    pub version: Option<String>,
}

#[async_trait]
pub trait ObjectBackend: Send + Sync + 'static {
    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>>;
    async fn read(&self, key: &str) -> Result<Bytes>;
    async fn read_range(&self, key: &str, range: Range<u64>) -> Result<Bytes>;
    async fn write(
        &self,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectVersion>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn copy(&self, from: &str, to: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>>;
}
