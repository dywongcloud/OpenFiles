//! Tiny async Rust client for the OpenFiles HTTP API.
//! Add dependencies: reqwest = { version = "0.12", features = ["json"] }, serde = { version = "1", features = ["derive"] }

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct OpenFilesClient {
    base: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
}

impl OpenFilesClient {
    pub fn new(base: impl Into<String>) -> Self {
        Self { base: base.into().trim_end_matches('/').to_string(), client: reqwest::Client::new() }
    }

    fn url(&self, prefix: &str, path: &str) -> String {
        let clean = path.trim_start_matches('/');
        if clean.is_empty() { format!("{}{}", self.base, prefix) } else { format!("{}{}/{}", self.base, prefix, urlencoding::encode(clean)) }
    }

    pub async fn list(&self, path: &str) -> reqwest::Result<Vec<DirEntry>> {
        self.client.get(self.url("/v1/list", path)).send().await?.error_for_status()?.json().await
    }

    pub async fn read(&self, path: &str) -> reqwest::Result<bytes::Bytes> {
        self.client.get(self.url("/v1/read", path)).send().await?.error_for_status()?.bytes().await
    }

    pub async fn write(&self, path: &str, data: impl Into<bytes::Bytes>) -> reqwest::Result<()> {
        self.client.put(self.url("/v1/write", path)).body(data.into()).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn flush(&self) -> reqwest::Result<()> {
        self.client.post(format!("{}/v1/flush", self.base)).send().await?.error_for_status()?;
        Ok(())
    }
}
