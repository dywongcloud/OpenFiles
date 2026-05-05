use crate::error::{OpenFilesError, Result};
use crate::types::{now_ns, FileKind, FileStat, PosixMetadata};
use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{ops::Range, path::PathBuf, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub path: String,
    pub key: String,
    pub kind: FileKind,
    pub size: u64,
    pub etag: Option<String>,
    pub version: Option<String>,
    pub base_etag: Option<String>,
    pub base_version: Option<String>,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub mtime_ns: u128,
    pub ctime_ns: u128,
    pub last_access_ns: u128,
    pub cached_data: bool,
    pub dirty: bool,
    pub deleted: bool,
    pub lost_found: bool,
}

impl CacheEntry {
    pub fn from_posix(path: String, key: String, posix: PosixMetadata, size: u64) -> Self {
        Self {
            path,
            key,
            kind: posix.kind,
            size,
            etag: None,
            version: None,
            base_etag: None,
            base_version: None,
            uid: posix.uid,
            gid: posix.gid,
            mode: posix.mode,
            mtime_ns: posix.mtime_ns,
            ctime_ns: posix.ctime_ns,
            last_access_ns: now_ns(),
            cached_data: false,
            dirty: false,
            deleted: false,
            lost_found: false,
        }
    }

    pub fn to_stat(&self) -> FileStat {
        FileStat {
            path: self.path.clone(),
            key: self.key.clone(),
            kind: self.kind,
            size: self.size,
            mode: self.mode,
            uid: self.uid,
            gid: self.gid,
            mtime_ns: self.mtime_ns,
            ctime_ns: self.ctime_ns,
            cached: self.cached_data,
            dirty: self.dirty,
            etag: self.etag.clone(),
            version: self.version.clone(),
        }
    }

    pub fn posix(&self) -> PosixMetadata {
        PosixMetadata {
            path: self.path.clone(),
            kind: self.kind,
            uid: self.uid,
            gid: self.gid,
            mode: self.mode,
            mtime_ns: self.mtime_ns,
            ctime_ns: self.ctime_ns,
            symlink_target: None,
            xattrs: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Cache {
    root: PathBuf,
    entries: Arc<DashMap<String, CacheEntry>>,
}

impl Cache {
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let cache = Self {
            root: root.into(),
            entries: Arc::new(DashMap::new()),
        };
        tokio::fs::create_dir_all(cache.objects_dir()).await?;
        tokio::fs::create_dir_all(cache.meta_dir()).await?;
        cache.load_index().await?;
        Ok(cache)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn meta_dir(&self) -> PathBuf {
        self.root.join("meta")
    }

    fn hash(key: &str) -> String {
        let mut h = Sha256::new();
        h.update(key.as_bytes());
        hex::encode(h.finalize())
    }

    fn data_path(&self, key: &str) -> PathBuf {
        self.objects_dir().join(format!("{}.bin", Self::hash(key)))
    }

    fn meta_path(&self, key: &str) -> PathBuf {
        self.meta_dir().join(format!("{}.json", Self::hash(key)))
    }

    async fn load_index(&self) -> Result<()> {
        let mut dir = tokio::fs::read_dir(self.meta_dir()).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read(&path).await {
                Ok(data) => match serde_json::from_slice::<CacheEntry>(&data) {
                    Ok(meta) => {
                        self.entries.insert(meta.path.clone(), meta);
                    }
                    Err(err) => tracing::warn!(?path, ?err, "skipping corrupt cache metadata"),
                },
                Err(err) => tracing::warn!(?path, ?err, "failed to read cache metadata"),
            }
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<CacheEntry> {
        self.entries.get(path).map(|v| v.value().clone())
    }

    pub fn iter_entries(&self) -> Vec<CacheEntry> {
        self.entries.iter().map(|v| v.value().clone()).collect()
    }

    pub fn dirty_entries(&self) -> Vec<CacheEntry> {
        self.entries
            .iter()
            .filter(|v| v.value().dirty)
            .map(|v| v.value().clone())
            .collect()
    }

    pub async fn put_entry(&self, entry: CacheEntry) -> Result<()> {
        let path = self.meta_path(&entry.key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(&entry)?;
        tokio::fs::write(path, bytes).await?;
        self.entries.insert(entry.path.clone(), entry);
        Ok(())
    }

    pub async fn remove_entry(&self, path: &str) -> Result<()> {
        if let Some((_, entry)) = self.entries.remove(path) {
            let _ = tokio::fs::remove_file(self.meta_path(&entry.key)).await;
            let _ = tokio::fs::remove_file(self.data_path(&entry.key)).await;
        }
        Ok(())
    }

    pub async fn write_data(&self, key: &str, data: Bytes) -> Result<()> {
        let path = self.data_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut f = tokio::fs::File::create(path).await?;
        f.write_all(&data).await?;
        f.flush().await?;
        Ok(())
    }

    pub async fn read_all(&self, key: &str) -> Result<Bytes> {
        let path = self.data_path(key);
        match tokio::fs::read(path).await {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(OpenFilesError::NotFound(key.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn read_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        let path = self.data_path(key);
        let mut f = tokio::fs::File::open(path).await?;
        f.seek(std::io::SeekFrom::Start(range.start)).await?;
        let len = range.end.saturating_sub(range.start) as usize;
        let mut buf = vec![0; len];
        let n = f.read(&mut buf).await?;
        buf.truncate(n);
        Ok(Bytes::from(buf))
    }

    pub async fn remove_data(&self, key: &str) -> Result<()> {
        match tokio::fs::remove_file(self.data_path(key)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn mark_clean(
        &self,
        path: &str,
        etag: Option<String>,
        version: Option<String>,
    ) -> Result<()> {
        let mut entry = self
            .get(path)
            .ok_or_else(|| OpenFilesError::NotFound(path.to_string()))?;
        entry.dirty = false;
        entry.deleted = false;
        entry.etag = etag.clone();
        entry.version = version.clone();
        entry.base_etag = etag;
        entry.base_version = version;
        self.put_entry(entry).await
    }

    pub async fn touch(&self, path: &str) -> Result<()> {
        if let Some(mut entry) = self.get(path) {
            entry.last_access_ns = now_ns();
            self.put_entry(entry).await?;
        }
        Ok(())
    }

    pub async fn expire_data_older_than_ns(&self, cutoff_ns: u128) -> Result<u64> {
        let mut removed = 0;
        for mut entry in self.iter_entries() {
            if entry.cached_data && !entry.dirty && entry.last_access_ns < cutoff_ns {
                self.remove_data(&entry.key).await?;
                entry.cached_data = false;
                self.put_entry(entry).await?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}
