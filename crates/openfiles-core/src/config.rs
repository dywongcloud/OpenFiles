use crate::types::{ExpirationRule, ImportDataRule};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    #[default]
    LocalFs,
    AwsS3,
    GcpGcs,
    AzureBlob,
    VercelBlob,
    Storj,
    Minio,
    NetappStorageGrid,
    S3Compatible,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BackendConfig {
    #[serde(default)]
    pub provider: ProviderKind,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub container: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub account_key: Option<String>,
    #[serde(default)]
    pub sas_token: Option<String>,
    #[serde(default)]
    pub credential: Option<String>,
    #[serde(default)]
    pub credential_path: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    pub dir: PathBuf,
    #[serde(default = "default_direct_read_threshold")]
    pub direct_read_threshold_bytes: u64,
    #[serde(default = "default_cache_capacity")]
    pub capacity_bytes: u64,
}

fn default_direct_read_threshold() -> u64 {
    1024 * 1024
}

fn default_cache_capacity() -> u64 {
    64 * 1024 * 1024 * 1024
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from(".openfiles-cache"),
            direct_read_threshold_bytes: default_direct_read_threshold(),
            capacity_bytes: default_cache_capacity(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "default_export_batch")]
    pub export_batch_window_secs: u64,
    #[serde(default)]
    pub import_rules: Vec<ImportDataRule>,
    #[serde(default)]
    pub expiration: ExpirationRule,
}

fn default_export_batch() -> u64 {
    60
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            export_batch_window_secs: default_export_batch(),
            import_rules: vec![ImportDataRule::default()],
            expiration: ExpirationRule::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenFilesConfig {
    #[serde(default = "default_fs_id")]
    pub fs_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub object_prefix: String,
    #[serde(default)]
    pub backend: BackendConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

fn default_fs_id() -> String {
    "ofss-local".to_string()
}

impl Default for OpenFilesConfig {
    fn default() -> Self {
        Self {
            fs_id: default_fs_id(),
            name: "openfiles".to_string(),
            object_prefix: String::new(),
            backend: BackendConfig::default(),
            cache: CacheConfig::default(),
            sync: SyncConfig::default(),
        }
    }
}

impl OpenFilesConfig {
    pub fn from_toml_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn normalized_prefix(&self) -> String {
        let p = self.object_prefix.trim_matches('/');
        if p.is_empty() {
            String::new()
        } else {
            format!("{p}/")
        }
    }

    pub fn ensure_root_import_rule(&mut self) {
        if !self.sync.import_rules.iter().any(|r| r.prefix.is_empty()) {
            self.sync.import_rules.push(ImportDataRule::default());
        }
    }
}
