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
pub struct NatsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_nats_url")]
    pub url: String,
    #[serde(default = "default_nats_subject_prefix")]
    pub subject_prefix: String,
    #[serde(default)]
    pub queue_group: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default = "default_nats_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_nats_max_payload_bytes")]
    pub max_payload_bytes: usize,
    #[serde(default = "default_nats_publish_events")]
    pub publish_events: bool,
}

fn default_nats_url() -> String {
    "nats://127.0.0.1:4222".to_string()
}

fn default_nats_subject_prefix() -> String {
    "openfiles".to_string()
}

fn default_nats_request_timeout_ms() -> u64 {
    30_000
}

fn default_nats_max_payload_bytes() -> usize {
    10 * 1024 * 1024
}

fn default_nats_publish_events() -> bool {
    true
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_nats_url(),
            subject_prefix: default_nats_subject_prefix(),
            queue_group: None,
            instance_id: None,
            request_timeout_ms: default_nats_request_timeout_ms(),
            max_payload_bytes: default_nats_max_payload_bytes(),
            publish_events: true,
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
    #[serde(default)]
    pub nats: NatsConfig,
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
            nats: NatsConfig::default(),
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
