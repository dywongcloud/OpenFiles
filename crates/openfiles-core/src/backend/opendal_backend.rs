use super::{ObjectBackend, ObjectMeta, ObjectVersion};
use crate::error::Result;
use crate::types::{now_ns, FileKind};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use opendal::Operator;
use std::{collections::HashMap, ops::Range};

#[derive(Clone)]
pub struct OpendalBackend {
    op: Operator,
}

impl OpendalBackend {
    pub fn new(op: Operator) -> Self {
        Self { op }
    }

    fn meta_from_entry(path: String, meta: opendal::Metadata) -> ObjectMeta {
        let kind = if meta.is_dir() {
            FileKind::Directory
        } else {
            FileKind::File
        };
        ObjectMeta {
            key: path,
            size: meta.content_length(),
            etag: meta.etag().map(ToString::to_string),
            version: meta.version().map(ToString::to_string),
            updated_ns: now_ns(),
            metadata: HashMap::new(),
            kind,
        }
    }
}

#[async_trait]
impl ObjectBackend for OpendalBackend {
    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
        match self.op.stat(key).await {
            Ok(meta) => Ok(Some(Self::meta_from_entry(key.to_string(), meta))),
            Err(e) if e.kind() == opendal::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn read(&self, key: &str) -> Result<Bytes> {
        let data = self.op.read(key).await?;
        Ok(Bytes::from(data.to_vec()))
    }

    async fn read_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        let data = self.op.read_with(key).range(range).await?;
        Ok(Bytes::from(data.to_vec()))
    }

    async fn write(
        &self,
        key: &str,
        data: Bytes,
        _metadata: HashMap<String, String>,
    ) -> Result<ObjectVersion> {
        self.op.write(key, data.to_vec()).await?;
        let head = self.head(key).await?;
        Ok(ObjectVersion {
            etag: head.as_ref().and_then(|m| m.etag.clone()),
            version: head.as_ref().and_then(|m| m.version.clone()),
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.op.delete(key).await?;
        Ok(())
    }

    async fn copy(&self, from: &str, to: &str) -> Result<()> {
        match self.op.copy(from, to).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == opendal::ErrorKind::Unsupported => {
                let data = self.read(from).await?;
                self.write(to, data, HashMap::new()).await?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
        let mut lister = self.op.lister(prefix).await?;
        let mut out = Vec::new();
        while let Some(entry) = lister.next().await {
            let entry = entry?;
            let path = entry.path().to_string();
            let meta = entry.metadata();
            out.push(Self::meta_from_entry(path, meta.clone()));
        }
        Ok(out)
    }
}
