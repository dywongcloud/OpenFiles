use crate::error::{OpenFilesError, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    #[default]
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ImportTrigger {
    #[default]
    OnDirectoryFirstAccess,
    OnFileAccess,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportDataRule {
    pub prefix: String,
    #[serde(default)]
    pub trigger: ImportTrigger,
    pub size_less_than: u64,
}

impl Default for ImportDataRule {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            trigger: ImportTrigger::OnDirectoryFirstAccess,
            size_less_than: 128 * 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpirationRule {
    pub days_after_last_access: u32,
}

impl Default for ExpirationRule {
    fn default() -> Self {
        Self {
            days_after_last_access: 30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileStat {
    pub path: String,
    pub key: String,
    pub kind: FileKind,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime_ns: u128,
    pub ctime_ns: u128,
    pub cached: bool,
    pub dirty: bool,
    pub etag: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosixMetadata {
    pub path: String,
    pub kind: FileKind,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub mtime_ns: u128,
    pub ctime_ns: u128,
    pub symlink_target: Option<String>,
    #[serde(default)]
    pub xattrs: std::collections::BTreeMap<String, String>,
}

impl PosixMetadata {
    pub fn new_file(path: impl Into<String>) -> Self {
        let now = now_ns();
        Self {
            path: path.into(),
            kind: FileKind::File,
            uid: 0,
            gid: 0,
            mode: 0o644,
            mtime_ns: now,
            ctime_ns: now,
            symlink_target: None,
            xattrs: Default::default(),
        }
    }

    pub fn new_dir(path: impl Into<String>) -> Self {
        let now = now_ns();
        Self {
            path: path.into(),
            kind: FileKind::Directory,
            uid: 0,
            gid: 0,
            mode: 0o755,
            mtime_ns: now,
            ctime_ns: now,
            symlink_target: None,
            xattrs: Default::default(),
        }
    }
}

pub fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn normalize_path(path: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return Ok(String::new());
    }
    let no_prefix = path.trim_start_matches('/');
    let mut components = Vec::new();
    for comp in no_prefix.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            return Err(OpenFilesError::InvalidPath(path.to_string()));
        }
        if comp.len() > 255 {
            return Err(OpenFilesError::InvalidPath(format!(
                "path component exceeds 255 bytes: {comp}"
            )));
        }
        components.push(comp);
    }
    let joined = components.join("/");
    if joined.len() > 1024 {
        return Err(OpenFilesError::InvalidPath(format!(
            "object key path exceeds 1024 bytes: {joined}"
        )));
    }
    Ok(joined)
}

pub fn display_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    }
}

pub fn dir_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

pub fn parent_dir(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

pub fn file_name(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        path.rsplit('/').next().unwrap_or(path).to_string()
    }
}
