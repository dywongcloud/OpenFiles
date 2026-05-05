use crate::types::{FileKind, PosixMetadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const SIDECAR_PREFIX: &str = ".openfiles/meta/";
pub const DIR_MARKER_SUFFIX: &str = ".openfiles-dir.json";

pub const META_UID: &str = "ofss-uid";
pub const META_GID: &str = "ofss-gid";
pub const META_MODE: &str = "ofss-mode";
pub const META_MTIME_NS: &str = "ofss-mtime-ns";
pub const META_CTIME_NS: &str = "ofss-ctime-ns";
pub const META_KIND: &str = "ofss-kind";
pub const META_SYMLINK_TARGET: &str = "ofss-symlink-target";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidecarMetadata {
    pub schema: String,
    pub posix: PosixMetadata,
}

impl SidecarMetadata {
    pub fn new(posix: PosixMetadata) -> Self {
        Self {
            schema: "openfiles.sidecar.v1".to_string(),
            posix,
        }
    }
}

pub fn sidecar_key(object_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(object_key.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{SIDECAR_PREFIX}{digest}.json")
}

pub fn encode_user_metadata(meta: &PosixMetadata) -> HashMap<String, String> {
    let mut out = HashMap::new();
    out.insert(META_UID.to_string(), meta.uid.to_string());
    out.insert(META_GID.to_string(), meta.gid.to_string());
    out.insert(META_MODE.to_string(), meta.mode.to_string());
    out.insert(META_MTIME_NS.to_string(), meta.mtime_ns.to_string());
    out.insert(META_CTIME_NS.to_string(), meta.ctime_ns.to_string());
    out.insert(
        META_KIND.to_string(),
        match meta.kind {
            FileKind::File => "file",
            FileKind::Directory => "directory",
            FileKind::Symlink => "symlink",
        }
        .to_string(),
    );
    if let Some(target) = &meta.symlink_target {
        out.insert(META_SYMLINK_TARGET.to_string(), target.clone());
    }
    out
}

pub fn decode_user_metadata(path: &str, input: &HashMap<String, String>) -> Option<PosixMetadata> {
    let uid = input.get(META_UID)?.parse().ok()?;
    let gid = input.get(META_GID)?.parse().ok()?;
    let mode = input.get(META_MODE)?.parse().ok()?;
    let mtime_ns = input.get(META_MTIME_NS)?.parse().ok()?;
    let ctime_ns = input.get(META_CTIME_NS)?.parse().ok()?;
    let kind = match input.get(META_KIND).map(String::as_str) {
        Some("directory") => FileKind::Directory,
        Some("symlink") => FileKind::Symlink,
        _ => FileKind::File,
    };
    Some(PosixMetadata {
        path: path.to_string(),
        kind,
        uid,
        gid,
        mode,
        mtime_ns,
        ctime_ns,
        symlink_target: input.get(META_SYMLINK_TARGET).cloned(),
        xattrs: Default::default(),
    })
}

pub fn is_internal_key(key: &str) -> bool {
    key.starts_with(SIDECAR_PREFIX) || key.contains("/.openfiles/")
}
