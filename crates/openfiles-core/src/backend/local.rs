use super::{ObjectBackend, ObjectMeta, ObjectVersion};
use crate::error::{OpenFilesError, Result};
use crate::types::{normalize_path, now_ns, FileKind};
use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[derive(Clone, Debug)]
pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn full_path(&self, key: &str) -> Result<PathBuf> {
        let normalized = normalize_path(key)?;
        Ok(self.root.join(normalized))
    }

    fn etag_for(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        hex::encode(h.finalize())
    }

    fn walk(
        root: &Path,
        base: &Path,
        prefix: &str,
        out: &mut Vec<ObjectMeta>,
    ) -> std::io::Result<()> {
        if !base.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(base)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() {
                Self::walk(root, &path, prefix, out)?;
            } else if meta.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let key = rel.to_string_lossy().replace('\\', "/");
                if key.starts_with(prefix) {
                    out.push(ObjectMeta {
                        key,
                        size: meta.len(),
                        etag: None,
                        version: None,
                        updated_ns: now_ns(),
                        metadata: HashMap::new(),
                        kind: FileKind::File,
                    });
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ObjectBackend for LocalFsBackend {
    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
        let path = self.full_path(key)?;
        match tokio::fs::metadata(&path).await {
            Ok(meta) if meta.is_file() => Ok(Some(ObjectMeta {
                key: normalize_path(key)?,
                size: meta.len(),
                etag: None,
                version: None,
                updated_ns: now_ns(),
                metadata: HashMap::new(),
                kind: FileKind::File,
            })),
            Ok(meta) if meta.is_dir() => Ok(Some(ObjectMeta {
                key: normalize_path(key)?,
                size: 0,
                etag: None,
                version: None,
                updated_ns: now_ns(),
                metadata: HashMap::new(),
                kind: FileKind::Directory,
            })),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn read(&self, key: &str) -> Result<Bytes> {
        let path = self.full_path(key)?;
        let data = tokio::fs::read(path).await?;
        Ok(Bytes::from(data))
    }

    async fn read_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        let path = self.full_path(key)?;
        let mut f = tokio::fs::File::open(path).await?;
        f.seek(std::io::SeekFrom::Start(range.start)).await?;
        let len = range.end.saturating_sub(range.start) as usize;
        let mut buf = vec![0; len];
        let n = f.read(&mut buf).await?;
        buf.truncate(n);
        Ok(Bytes::from(buf))
    }

    async fn write(
        &self,
        key: &str,
        data: Bytes,
        _metadata: HashMap<String, String>,
    ) -> Result<ObjectVersion> {
        let path = self.full_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut f = tokio::fs::File::create(&path).await?;
        f.write_all(&data).await?;
        f.flush().await?;
        Ok(ObjectVersion {
            etag: Some(Self::etag_for(&data)),
            version: None,
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.full_path(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn copy(&self, from: &str, to: &str) -> Result<()> {
        let src = self.full_path(from)?;
        let dst = self.full_path(to)?;
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        match tokio::fs::copy(src, dst).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(OpenFilesError::NotFound(from.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
        let root = self.root.clone();
        let prefix =
            normalize_path(prefix).unwrap_or_else(|_| prefix.trim_start_matches('/').to_string());
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            Self::walk(&root, &root, &prefix, &mut out)?;
            Ok::<_, std::io::Error>(out)
        })
        .await
        .map_err(|e| OpenFilesError::Internal(e.to_string()))?
        .map_err(OpenFilesError::from)
    }
}
