use crate::backend::{ObjectBackend, ObjectMeta};
use crate::cache::{Cache, CacheEntry};
use crate::config::OpenFilesConfig;
use crate::error::{OpenFilesError, Result};
use crate::metadata::{
    decode_user_metadata, encode_user_metadata, is_internal_key, sidecar_key, SidecarMetadata,
};
use crate::types::{
    dir_prefix, display_path, file_name, normalize_path, now_ns, DirEntry, FileKind, FileStat,
    ImportDataRule, ImportTrigger, PosixMetadata,
};
use bytes::Bytes;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone)]
pub struct OpenFilesEngine {
    backend: Arc<dyn ObjectBackend>,
    cache: Cache,
    config: OpenFilesConfig,
}

impl OpenFilesEngine {
    pub async fn new(config: OpenFilesConfig, backend: Arc<dyn ObjectBackend>) -> Result<Self> {
        let mut config = config;
        config.ensure_root_import_rule();
        let cache = Cache::open(config.cache.dir.clone()).await?;
        Ok(Self {
            backend,
            cache,
            config,
        })
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    pub fn config(&self) -> &OpenFilesConfig {
        &self.config
    }

    pub fn key_for_path(&self, path: &str) -> Result<String> {
        let normalized = normalize_path(path)?;
        Ok(format!("{}{}", self.config.normalized_prefix(), normalized))
    }

    fn path_from_key(&self, key: &str) -> String {
        let prefix = self.config.normalized_prefix();
        key.strip_prefix(&prefix)
            .unwrap_or(key)
            .trim_matches('/')
            .to_string()
    }

    fn matching_import_rule(&self, path: &str) -> ImportDataRule {
        let mut best: Option<ImportDataRule> = None;
        for rule in &self.config.sync.import_rules {
            let p = rule.prefix.trim_start_matches('/');
            let matches = p.is_empty() || path.starts_with(p);
            if matches {
                match &best {
                    Some(existing) if existing.prefix.len() >= rule.prefix.len() => {}
                    _ => best = Some(rule.clone()),
                }
            }
        }
        best.unwrap_or_default()
    }

    async fn read_sidecar(&self, key: &str, path: &str) -> Result<Option<PosixMetadata>> {
        let side = sidecar_key(key);
        match self.backend.read(&side).await {
            Ok(bytes) => {
                let sidecar = serde_json::from_slice::<SidecarMetadata>(&bytes)?;
                Ok(Some(sidecar.posix))
            }
            Err(OpenFilesError::NotFound(_)) => Ok(None),
            Err(e) => {
                tracing::warn!(?key, ?e, "failed to read sidecar metadata");
                let head = self.backend.head(key).await?;
                Ok(head.and_then(|m| decode_user_metadata(path, &m.metadata)))
            }
        }
    }

    async fn write_sidecar(&self, key: &str, posix: PosixMetadata) -> Result<()> {
        let side = sidecar_key(key);
        let bytes = serde_json::to_vec_pretty(&SidecarMetadata::new(posix))?;
        self.backend
            .write(&side, Bytes::from(bytes), HashMap::new())
            .await?;
        Ok(())
    }

    async fn import_meta_from_object(&self, path: &str, obj: &ObjectMeta) -> Result<CacheEntry> {
        let posix = match self.read_sidecar(&obj.key, path).await? {
            Some(m) => m,
            None => decode_user_metadata(path, &obj.metadata)
                .unwrap_or_else(|| PosixMetadata::new_file(path)),
        };
        let mut entry = CacheEntry::from_posix(path.to_string(), obj.key.clone(), posix, obj.size);
        entry.etag = obj.etag.clone();
        entry.version = obj.version.clone();
        entry.base_etag = obj.etag.clone();
        entry.base_version = obj.version.clone();
        self.cache.put_entry(entry.clone()).await?;
        Ok(entry)
    }

    pub async fn stat(&self, path: &str) -> Result<FileStat> {
        let normalized = normalize_path(path)?;
        if normalized.is_empty() {
            let meta = PosixMetadata::new_dir("/");
            return Ok(FileStat {
                path: "/".to_string(),
                key: self.config.normalized_prefix(),
                kind: FileKind::Directory,
                size: 0,
                mode: meta.mode,
                uid: meta.uid,
                gid: meta.gid,
                mtime_ns: meta.mtime_ns,
                ctime_ns: meta.ctime_ns,
                cached: true,
                dirty: false,
                etag: None,
                version: None,
            });
        }
        if let Some(entry) = self.cache.get(&normalized) {
            if !entry.deleted {
                return Ok(entry.to_stat());
            }
        }
        let key = self.key_for_path(&normalized)?;
        if let Some(obj) = self.backend.head(&key).await? {
            let entry = self.import_meta_from_object(&normalized, &obj).await?;
            return Ok(entry.to_stat());
        }
        let dir_prefix_key = format!("{}/", key.trim_end_matches('/'));
        let entries = self.backend.list(&dir_prefix_key).await?;
        if entries
            .iter()
            .any(|m| !m.key.ends_with('/') && !is_internal_key(&m.key))
        {
            let posix = PosixMetadata::new_dir(display_path(&normalized));
            let entry = CacheEntry::from_posix(normalized.clone(), dir_prefix_key, posix, 0);
            self.cache.put_entry(entry.clone()).await?;
            return Ok(entry.to_stat());
        }
        Err(OpenFilesError::NotFound(display_path(&normalized)))
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let normalized = normalize_path(path)?;
        let dir_prefix_path = dir_prefix(&normalized);
        let key_prefix = format!("{}{}", self.config.normalized_prefix(), dir_prefix_path);
        let objects = self.backend.list(&key_prefix).await?;
        let rule = self.matching_import_rule(&dir_prefix_path);
        let mut files = BTreeMap::<String, DirEntry>::new();
        let mut dirs = HashSet::<String>::new();

        for obj in objects {
            if obj.key.ends_with('/') || is_internal_key(&obj.key) {
                continue;
            }
            let rel = self.path_from_key(&obj.key);

            if rel.is_empty()
                || rel == ".openfiles"
                || rel.starts_with(".openfiles/")
                || !rel.starts_with(&dir_prefix_path)
            {
                continue;
            }
            let child_rel = &rel[dir_prefix_path.len()..];
            if child_rel.is_empty() {
                continue;
            }
            if let Some((dir, _rest)) = child_rel.split_once('/') {
                let child_path = if normalized.is_empty() {
                    dir.to_string()
                } else {
                    format!("{normalized}/{dir}")
                };
                dirs.insert(child_path);
            } else {
                let entry = self.import_meta_from_object(&rel, &obj).await?;
                if rule.trigger == ImportTrigger::OnDirectoryFirstAccess
                    && obj.size < rule.size_less_than
                    && !entry.cached_data
                {
                    let data = self.backend.read(&obj.key).await?;
                    let mut cached = entry.clone();
                    self.cache.write_data(&obj.key, data).await?;
                    cached.cached_data = true;
                    cached.last_access_ns = now_ns();
                    self.cache.put_entry(cached).await?;
                }
                files.insert(
                    rel.clone(),
                    DirEntry {
                        name: file_name(&rel),
                        path: display_path(&rel),
                        kind: FileKind::File,
                        size: obj.size,
                    },
                );
            }
        }

        for entry in self.cache.iter_entries() {
            if entry.deleted
                || entry.path == ".openfiles"
                || entry.path.starts_with(".openfiles/")
                || entry.path.starts_with(".openfiles")
            {
                continue;
            }
            if !entry.path.starts_with(&dir_prefix_path) {
                continue;
            }
            let child_rel = &entry.path[dir_prefix_path.len()..];
            if child_rel.is_empty() {
                continue;
            }
            if let Some((dir, _)) = child_rel.split_once('/') {
                let child_path = if normalized.is_empty() {
                    dir.to_string()
                } else {
                    format!("{normalized}/{dir}")
                };
                dirs.insert(child_path);
            } else {
                files.insert(
                    entry.path.clone(),
                    DirEntry {
                        name: file_name(&entry.path),
                        path: display_path(&entry.path),
                        kind: entry.kind,
                        size: entry.size,
                    },
                );
            }
        }

        let mut out = Vec::new();
        for dir in dirs {
            out.push(DirEntry {
                name: file_name(&dir),
                path: display_path(&dir),
                kind: FileKind::Directory,
                size: 0,
            });
        }
        out.extend(files.into_values());
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Bytes> {
        let normalized = normalize_path(path)?;
        let stat = self.stat(&normalized).await?;
        if stat.kind != FileKind::File {
            return Err(OpenFilesError::InvalidPath(format!("not a file: {path}")));
        }
        let key = stat.key.clone();
        let end = offset.saturating_add(len).min(stat.size);
        if end <= offset {
            return Ok(Bytes::new());
        }
        if let Some(entry) = self.cache.get(&normalized) {
            if entry.cached_data || entry.dirty {
                self.cache.touch(&normalized).await?;
                return self.cache.read_range(&key, offset..end).await;
            }
        }

        let rule = self.matching_import_rule(&normalized);
        if len >= self.config.cache.direct_read_threshold_bytes
            || stat.size >= self.config.cache.direct_read_threshold_bytes
        {
            if rule.trigger == ImportTrigger::OnFileAccess && stat.size < rule.size_less_than {
                let all = self.backend.read(&key).await?;
                let mut entry = self
                    .cache
                    .get(&normalized)
                    .ok_or_else(|| OpenFilesError::NotFound(path.to_string()))?;
                self.cache.write_data(&key, all.clone()).await?;
                entry.cached_data = true;
                entry.last_access_ns = now_ns();
                self.cache.put_entry(entry).await?;
                return Ok(all.slice(offset as usize..end as usize));
            }
            return self.backend.read_range(&key, offset..end).await;
        }

        let all = self.backend.read(&key).await?;
        let mut entry = self
            .cache
            .get(&normalized)
            .ok_or_else(|| OpenFilesError::NotFound(path.to_string()))?;
        self.cache.write_data(&key, all.clone()).await?;
        entry.cached_data = true;
        entry.last_access_ns = now_ns();
        self.cache.put_entry(entry).await?;
        Ok(all.slice(offset as usize..end as usize))
    }

    pub async fn read_all(&self, path: &str) -> Result<Bytes> {
        let stat = self.stat(path).await?;
        self.read_range(path, 0, stat.size).await
    }

    pub async fn write_file(&self, path: &str, data: Bytes) -> Result<()> {
        let normalized = normalize_path(path)?;
        if normalized.is_empty() {
            return Err(OpenFilesError::InvalidPath("cannot write root".to_string()));
        }
        let key = self.key_for_path(&normalized)?;
        let head = self.backend.head(&key).await?;
        let posix = PosixMetadata::new_file(display_path(&normalized));
        let mut entry =
            CacheEntry::from_posix(normalized.clone(), key.clone(), posix, data.len() as u64);
        entry.cached_data = true;
        entry.dirty = true;
        entry.base_etag = head.as_ref().and_then(|h| h.etag.clone());
        entry.base_version = head.as_ref().and_then(|h| h.version.clone());
        entry.etag = entry.base_etag.clone();
        entry.version = entry.base_version.clone();
        self.cache.write_data(&key, data).await?;
        self.cache.put_entry(entry).await?;
        if self.config.sync.export_batch_window_secs == 0 {
            self.flush_path(&normalized).await?;
        }
        Ok(())
    }

    pub async fn delete_path(&self, path: &str) -> Result<()> {
        let normalized = normalize_path(path)?;
        let key = self.key_for_path(&normalized)?;
        let mut entry = self.cache.get(&normalized).unwrap_or_else(|| {
            let mut e = CacheEntry::from_posix(
                normalized.clone(),
                key.clone(),
                PosixMetadata::new_file(display_path(&normalized)),
                0,
            );
            e.base_etag = None;
            e.base_version = None;
            e
        });
        entry.deleted = true;
        entry.dirty = true;
        self.cache.put_entry(entry).await?;
        if self.config.sync.export_batch_window_secs == 0 {
            self.flush_path(&normalized).await?;
        }
        Ok(())
    }

    pub async fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        let from_norm = normalize_path(from)?;
        let to_norm = normalize_path(to)?;
        let from_stat = self.stat(&from_norm).await?;

        if from_stat.kind == FileKind::Directory {
            let mut pending_dirs = vec![from_norm.clone()];
            let mut files = Vec::new();

            while let Some(dir) = pending_dirs.pop() {
                for child in self.list_dir(&dir).await? {
                    match child.kind {
                        FileKind::Directory => pending_dirs.push(normalize_path(&child.path)?),
                        FileKind::File | FileKind::Symlink => {
                            files.push(normalize_path(&child.path)?);
                        }
                    }
                }
            }

            files.sort();
            for file in files {
                let rel = file
                    .strip_prefix(&from_norm)
                    .unwrap_or(file.as_str())
                    .trim_start_matches('/');
                let target = if rel.is_empty() {
                    to_norm.clone()
                } else {
                    format!("{to_norm}/{rel}")
                };
                self.rename_file(&file, &target).await?;
            }

            self.cache.remove_entry(&from_norm).await?;
            return Ok(());
        }

        self.rename_file(&from_norm, &to_norm).await
    }

    async fn rename_file(&self, from_norm: &str, to_norm: &str) -> Result<()> {
        let data = self.read_all(from_norm).await?;
        self.write_file(to_norm, data).await?;
        self.delete_path(from_norm).await?;
        Ok(())
    }

    async fn flush_path(&self, normalized: &str) -> Result<()> {
        let entry = match self.cache.get(normalized) {
            Some(e) if e.dirty => e,
            _ => return Ok(()),
        };
        if entry.deleted {
            self.backend.delete(&entry.key).await?;
            let _ = self.backend.delete(&sidecar_key(&entry.key)).await;
            self.cache.remove_entry(normalized).await?;
            return Ok(());
        }
        let head = self.backend.head(&entry.key).await?;
        let remote_changed = match (&head, &entry.base_etag, &entry.base_version) {
            (Some(h), Some(base), _) if h.etag.as_ref() != Some(base) => true,
            (Some(h), _, Some(base)) if h.version.as_ref() != Some(base) => true,
            (Some(_), None, None) if entry.base_etag.is_none() && entry.base_version.is_none() => {
                false
            }
            _ => false,
        };
        if remote_changed {
            self.move_to_lost_found(&entry).await?;
            if let Some(remote) = head {
                self.import_meta_from_object(normalized, &remote).await?;
            }
            return Err(OpenFilesError::Conflict(format!(
                "{} changed remotely; local copy moved to lost+found",
                display_path(normalized)
            )));
        }
        let data = self.cache.read_all(&entry.key).await?;
        let posix = entry.posix();
        let version = self
            .backend
            .write(&entry.key, data, encode_user_metadata(&posix))
            .await?;
        self.write_sidecar(&entry.key, posix).await?;
        self.cache
            .mark_clean(normalized, version.etag, version.version)
            .await?;
        Ok(())
    }

    async fn move_to_lost_found(&self, entry: &CacheEntry) -> Result<()> {
        let name = file_name(&entry.path);
        let lost_path = format!(
            ".openfiles-lost+found-{}/{}-{}",
            self.config.fs_id,
            now_ns(),
            name
        );
        let lost_key = self.key_for_path(&lost_path)?;
        let data = self.cache.read_all(&entry.key).await?;
        let mut lost = entry.clone();
        lost.path = lost_path;
        lost.key = lost_key.clone();
        lost.dirty = false;
        lost.lost_found = true;
        lost.cached_data = true;
        lost.base_etag = None;
        lost.base_version = None;
        self.cache.write_data(&lost_key, data).await?;
        self.cache.put_entry(lost).await?;
        self.cache.remove_entry(&entry.path).await?;
        Ok(())
    }

    pub async fn flush(&self) -> Result<usize> {
        let dirty = self.cache.dirty_entries();
        let mut ok = 0usize;
        let mut last_err = None;
        for entry in dirty {
            match self.flush_path(&entry.path).await {
                Ok(()) => ok += 1,
                Err(e) => {
                    tracing::warn!(path=%entry.path, error=%e, "flush failed");
                    last_err = Some(e);
                }
            }
        }
        if let Some(e) = last_err {
            if ok == 0 {
                return Err(e);
            }
        }
        Ok(ok)
    }

    pub async fn expire_cache(&self) -> Result<u64> {
        let days = self
            .config
            .sync
            .expiration
            .days_after_last_access
            .clamp(1, 365) as u128;
        let cutoff = now_ns().saturating_sub(days * 24 * 60 * 60 * 1_000_000_000u128);
        self.cache.expire_data_older_than_ns(cutoff).await
    }
}
