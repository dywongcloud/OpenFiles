use anyhow::{anyhow, Context};
use bytes::Bytes;
use futures::StreamExt;
use openfiles_core::{DirEntry, FileStat, OpenFilesConfig, OpenFilesEngine, OpenFilesError};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc, time::Duration};

#[derive(Debug)]
pub enum DistributedError {
    OpenFiles(OpenFilesError),
    Transport(anyhow::Error),
}

impl fmt::Display for DistributedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenFiles(err) => write!(f, "{err}"),
            Self::Transport(err) => write!(f, "distributed transport error: {err}"),
        }
    }
}

impl std::error::Error for DistributedError {}

impl From<OpenFilesError> for DistributedError {
    fn from(value: OpenFilesError) -> Self {
        Self::OpenFiles(value)
    }
}

impl From<anyhow::Error> for DistributedError {
    fn from(value: anyhow::Error) -> Self {
        Self::Transport(value)
    }
}

pub type Result<T> = std::result::Result<T, DistributedError>;

#[derive(Clone)]
pub enum DistributedOpenFiles {
    Local(Arc<OpenFilesEngine>),
    Nats(Arc<NatsOpenFiles>),
}

impl DistributedOpenFiles {
    pub async fn new(engine: Arc<OpenFilesEngine>, config: &OpenFilesConfig) -> Result<Self> {
        if !config.nats.enabled {
            return Ok(Self::Local(engine));
        }

        let nats = NatsOpenFiles::connect(engine, config).await?;
        Ok(Self::Nats(Arc::new(nats)))
    }

    pub async fn stat(&self, path: &str) -> Result<FileStat> {
        match self {
            Self::Local(engine) => Ok(engine.stat(path).await?),
            Self::Nats(nats) => nats.stat(path).await,
        }
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        match self {
            Self::Local(engine) => Ok(engine.list_dir(path).await?),
            Self::Nats(nats) => nats.list_dir(path).await,
        }
    }

    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Bytes> {
        match self {
            Self::Local(engine) => Ok(engine.read_range(path, offset, len).await?),

            // Important:
            // Reads are intentionally local even when NATS is enabled.
            // The shared object backend is the source of truth, while cache files are per process.
            // Routing reads through a NATS queue group can hit another worker whose cache metadata
            // references a cache object that does not exist on that specific process.
            Self::Nats(nats) => Ok(nats.engine.read_range(path, offset, len).await?),
        }
    }

    pub async fn read_all(&self, path: &str) -> Result<Bytes> {
        match self {
            Self::Local(engine) => Ok(engine.read_all(path).await?),

            // Same as read_range: keep reads local to avoid cross-process cache-file mismatch.
            Self::Nats(nats) => Ok(nats.engine.read_all(path).await?),
        }
    }

    pub async fn write_file(&self, path: &str, data: Bytes) -> Result<()> {
        match self {
            Self::Local(engine) => Ok(engine.write_file(path, data).await?),
            Self::Nats(nats) => nats.write_file(path, data).await,
        }
    }

    pub async fn delete_path(&self, path: &str) -> Result<()> {
        match self {
            Self::Local(engine) => Ok(engine.delete_path(path).await?),
            Self::Nats(nats) => nats.delete_path(path).await,
        }
    }

    pub async fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        match self {
            Self::Local(engine) => Ok(engine.rename_path(from, to).await?),
            Self::Nats(nats) => nats.rename_path(from, to).await,
        }
    }

    pub async fn flush(&self) -> Result<usize> {
        match self {
            Self::Local(engine) => Ok(engine.flush().await?),
            Self::Nats(nats) => nats.flush().await,
        }
    }

    pub async fn expire_cache(&self) -> Result<u64> {
        match self {
            Self::Local(engine) => Ok(engine.expire_cache().await?),
            Self::Nats(nats) => nats.expire_cache().await,
        }
    }
}

#[derive(Clone)]
pub struct NatsOpenFiles {
    engine: Arc<OpenFilesEngine>,
    client: async_nats::Client,
    subjects: Subjects,
    request_timeout: Duration,
    max_payload_bytes: usize,
}

impl NatsOpenFiles {
    pub async fn connect(engine: Arc<OpenFilesEngine>, config: &OpenFilesConfig) -> Result<Self> {
        let nats = &config.nats;
        let client = async_nats::connect(nats.url.as_str())
            .await
            .map_err(|err| anyhow!(err))
            .with_context(|| format!("failed to connect to NATS at {}", nats.url))?;

        let subjects = Subjects::new(config);
        let queue_group = nats
            .queue_group
            .clone()
            .unwrap_or_else(|| format!("{}.{}.workers", subjects.prefix, subjects.fs_token));

        let instance_id = nats
            .instance_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let request_timeout = Duration::from_millis(nats.request_timeout_ms.max(1));
        let max_payload_bytes = nats.max_payload_bytes.max(1024);

        spawn_worker(
            client.clone(),
            engine.clone(),
            subjects.clone(),
            queue_group,
            instance_id.clone(),
            max_payload_bytes,
            nats.publish_events,
        )
        .await?;

        spawn_event_listener(
            client.clone(),
            engine.clone(),
            subjects.clone(),
            instance_id,
        )
        .await?;

        tracing::info!(
            work_subject = %subjects.work,
            event_subject = %subjects.events,
            "NATS distribution enabled"
        );

        Ok(Self {
            engine,
            client,
            subjects,
            request_timeout,
            max_payload_bytes,
        })
    }

    pub async fn stat(&self, path: &str) -> Result<FileStat> {
        match self
            .request(WorkRequest::Stat {
                path: path.to_string(),
            })
            .await?
        {
            WorkResponse::Stat { stat } => Ok(stat),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        match self
            .request(WorkRequest::ListDir {
                path: path.to_string(),
            })
            .await?
        {
            WorkResponse::List { entries } => Ok(entries),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Bytes> {
        match self
            .request(WorkRequest::ReadRange {
                path: path.to_string(),
                offset,
                len,
            })
            .await?
        {
            WorkResponse::Bytes { data } => Ok(data),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn read_all(&self, path: &str) -> Result<Bytes> {
        match self
            .request(WorkRequest::ReadAll {
                path: path.to_string(),
            })
            .await?
        {
            WorkResponse::Bytes { data } => Ok(data),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn write_file(&self, path: &str, data: Bytes) -> Result<()> {
        match self
            .request(WorkRequest::WriteFile {
                path: path.to_string(),
                data,
            })
            .await?
        {
            WorkResponse::Unit => Ok(()),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn delete_path(&self, path: &str) -> Result<()> {
        match self
            .request(WorkRequest::DeletePath {
                path: path.to_string(),
            })
            .await?
        {
            WorkResponse::Unit => Ok(()),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        match self
            .request(WorkRequest::RenamePath {
                from: from.to_string(),
                to: to.to_string(),
            })
            .await?
        {
            WorkResponse::Unit => Ok(()),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn flush(&self) -> Result<usize> {
        match self.request(WorkRequest::Flush).await? {
            WorkResponse::Usize { value } => Ok(value),
            other => Err(unexpected_response(other)),
        }
    }

    pub async fn expire_cache(&self) -> Result<u64> {
        match self.request(WorkRequest::ExpireCache).await? {
            WorkResponse::U64 { value } => Ok(value),
            other => Err(unexpected_response(other)),
        }
    }

    async fn request(&self, request: WorkRequest) -> Result<WorkResponse> {
        let payload = serde_json::to_vec(&request).map_err(OpenFilesError::from)?;

        if payload.len() > self.max_payload_bytes {
            return Err(OpenFilesError::Unsupported(format!(
                "NATS payload is {} bytes, above configured max {} bytes",
                payload.len(),
                self.max_payload_bytes
            ))
            .into());
        }

        let response = tokio::time::timeout(
            self.request_timeout,
            self.client
                .request(self.subjects.work.clone(), Bytes::from(payload)),
        )
        .await
        .map_err(|_| anyhow!("NATS request timed out after {:?}", self.request_timeout))?
        .map_err(|err| anyhow!(err))?;

        let response: WorkResponse =
            serde_json::from_slice(&response.payload).map_err(OpenFilesError::from)?;

        match response {
            WorkResponse::Error { code, detail } => Err(error_from_wire(&code, detail).into()),
            other => Ok(other),
        }
    }
}

#[derive(Clone)]
struct Subjects {
    prefix: String,
    fs_token: String,
    work: String,
    events: String,
}

impl Subjects {
    fn new(config: &OpenFilesConfig) -> Self {
        let prefix = clean_subject_prefix(&config.nats.subject_prefix);
        let fs_token = subject_token(&config.fs_id);

        Self {
            work: format!("{prefix}.{fs_token}.work"),
            events: format!("{prefix}.{fs_token}.events"),
            prefix,
            fs_token,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum WorkRequest {
    Stat { path: String },
    ListDir { path: String },
    ReadAll { path: String },
    ReadRange { path: String, offset: u64, len: u64 },
    WriteFile { path: String, data: Bytes },
    DeletePath { path: String },
    RenamePath { from: String, to: String },
    Flush,
    ExpireCache,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum WorkResponse {
    Unit,
    Stat { stat: FileStat },
    List { entries: Vec<DirEntry> },
    Bytes { data: Bytes },
    Usize { value: usize },
    U64 { value: u64 },
    Error { code: String, detail: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct MutationEvent {
    origin: String,
    kind: String,
    paths: Vec<String>,
    prefixes: Vec<String>,
}

async fn spawn_worker(
    client: async_nats::Client,
    engine: Arc<OpenFilesEngine>,
    subjects: Subjects,
    queue_group: String,
    instance_id: String,
    max_payload_bytes: usize,
    publish_events: bool,
) -> Result<()> {
    let mut subscription = client
        .queue_subscribe(subjects.work.clone(), queue_group.clone())
        .await
        .map_err(|err| anyhow!(err))
        .with_context(|| format!("failed to subscribe to NATS queue group {queue_group}"))?;

    tokio::spawn(async move {
        while let Some(message) = subscription.next().await {
            let Some(reply) = message.reply.clone() else {
                continue;
            };

            let response = match serde_json::from_slice::<WorkRequest>(&message.payload) {
                Ok(request) => {
                    let event = mutation_event_for_request(&request, &instance_id);
                    let response = execute_work(engine.clone(), request).await;

                    if publish_events && !matches!(response, WorkResponse::Error { .. }) {
                        if let Some(event) = event {
                            if let Err(err) = publish_event(&client, &subjects.events, &event).await
                            {
                                tracing::warn!(
                                    error = %err,
                                    "failed to publish NATS mutation event"
                                );
                            }
                        }
                    }

                    response
                }
                Err(err) => WorkResponse::Error {
                    code: "json".to_string(),
                    detail: err.to_string(),
                },
            };

            let payload = encode_response(response, max_payload_bytes);

            if let Err(err) = client.publish(reply, Bytes::from(payload)).await {
                tracing::warn!(error = %err, "failed to send NATS work response");
            }
        }
    });

    Ok(())
}

async fn spawn_event_listener(
    client: async_nats::Client,
    engine: Arc<OpenFilesEngine>,
    subjects: Subjects,
    instance_id: String,
) -> Result<()> {
    let mut subscription = client
        .subscribe(subjects.events.clone())
        .await
        .map_err(|err| anyhow!(err))
        .with_context(|| {
            format!(
                "failed to subscribe to NATS events subject {}",
                subjects.events
            )
        })?;

    tokio::spawn(async move {
        while let Some(message) = subscription.next().await {
            match serde_json::from_slice::<MutationEvent>(&message.payload) {
                Ok(event) if event.origin == instance_id => {}

                Ok(event) => {
                    for path in &event.paths {
                        match engine.invalidate_path(path).await {
                            Ok(true) => tracing::debug!(
                                path = %path,
                                kind = %event.kind,
                                "invalidated cached path"
                            ),
                            Ok(false) => {}
                            Err(err) => tracing::warn!(
                                path = %path,
                                error = %err,
                                "failed to invalidate cached path"
                            ),
                        }
                    }

                    for prefix in &event.prefixes {
                        match engine.invalidate_prefix(prefix).await {
                            Ok(count) if count > 0 => tracing::debug!(
                                prefix = %prefix,
                                count,
                                kind = %event.kind,
                                "invalidated cached prefix"
                            ),
                            Ok(_) => {}
                            Err(err) => tracing::warn!(
                                prefix = %prefix,
                                error = %err,
                                "failed to invalidate cached prefix"
                            ),
                        }
                    }
                }

                Err(err) => {
                    tracing::warn!(error = %err, "failed to decode NATS mutation event");
                }
            }
        }
    });

    Ok(())
}

async fn execute_work(engine: Arc<OpenFilesEngine>, request: WorkRequest) -> WorkResponse {
    let result = match request {
        WorkRequest::Stat { path } => engine
            .stat(&path)
            .await
            .map(|stat| WorkResponse::Stat { stat }),

        WorkRequest::ListDir { path } => engine
            .list_dir(&path)
            .await
            .map(|entries| WorkResponse::List { entries }),

        WorkRequest::ReadAll { path } => engine
            .read_all(&path)
            .await
            .map(|data| WorkResponse::Bytes { data }),

        WorkRequest::ReadRange { path, offset, len } => engine
            .read_range(&path, offset, len)
            .await
            .map(|data| WorkResponse::Bytes { data }),

        WorkRequest::WriteFile { path, data } => match engine.write_file(&path, data).await {
            Ok(()) => engine.flush().await.map(|_| WorkResponse::Unit),
            Err(err) => Err(err),
        },

        WorkRequest::DeletePath { path } => match engine.delete_path(&path).await {
            Ok(()) => engine.flush().await.map(|_| WorkResponse::Unit),
            Err(err) => Err(err),
        },

        WorkRequest::RenamePath { from, to } => match engine.rename_path(&from, &to).await {
            Ok(()) => engine.flush().await.map(|_| WorkResponse::Unit),
            Err(err) => Err(err),
        },

        WorkRequest::Flush => engine
            .flush()
            .await
            .map(|value| WorkResponse::Usize { value }),

        WorkRequest::ExpireCache => engine
            .expire_cache()
            .await
            .map(|value| WorkResponse::U64 { value }),
    };

    match result {
        Ok(response) => response,
        Err(err) => WorkResponse::Error {
            code: error_code(&err).to_string(),
            detail: error_detail(&err),
        },
    }
}

fn mutation_event_for_request(request: &WorkRequest, origin: &str) -> Option<MutationEvent> {
    match request {
        WorkRequest::WriteFile { path, .. } => Some(MutationEvent {
            origin: origin.to_string(),
            kind: "write".to_string(),
            paths: vec![path.clone()],
            prefixes: Vec::new(),
        }),

        WorkRequest::DeletePath { path } => Some(MutationEvent {
            origin: origin.to_string(),
            kind: "delete".to_string(),
            paths: vec![path.clone()],
            prefixes: Vec::new(),
        }),

        WorkRequest::RenamePath { from, to } => Some(MutationEvent {
            origin: origin.to_string(),
            kind: "rename".to_string(),
            paths: Vec::new(),
            prefixes: vec![from.clone(), to.clone()],
        }),

        _ => None,
    }
}

async fn publish_event(
    client: &async_nats::Client,
    subject: &str,
    event: &MutationEvent,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(event)?;

    client
        .publish(subject.to_string(), Bytes::from(payload))
        .await
        .map_err(|err| anyhow!(err))?;

    Ok(())
}

fn encode_response(response: WorkResponse, max_payload_bytes: usize) -> Vec<u8> {
    match serde_json::to_vec(&response) {
        Ok(payload) if payload.len() <= max_payload_bytes => payload,

        Ok(payload) => serde_json::to_vec(&WorkResponse::Error {
            code: "unsupported".to_string(),
            detail: format!(
                "NATS response is {} bytes, above configured max {} bytes",
                payload.len(),
                max_payload_bytes
            ),
        })
        .expect("serializing fixed error response should not fail"),

        Err(err) => serde_json::to_vec(&WorkResponse::Error {
            code: "json".to_string(),
            detail: err.to_string(),
        })
        .expect("serializing fixed error response should not fail"),
    }
}

fn unexpected_response(response: WorkResponse) -> DistributedError {
    DistributedError::Transport(anyhow!("unexpected NATS work response: {response:?}"))
}

fn clean_subject_prefix(prefix: &str) -> String {
    let prefix = prefix.trim_matches('.');

    if prefix.is_empty() {
        "openfiles".to_string()
    } else {
        prefix
            .split('.')
            .map(subject_token)
            .collect::<Vec<_>>()
            .join(".")
    }
}

fn subject_token(token: &str) -> String {
    let cleaned: String = token
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

fn error_code(err: &OpenFilesError) -> &'static str {
    match err {
        OpenFilesError::InvalidPath(_) => "invalid-path",
        OpenFilesError::NotFound(_) => "not-found",
        OpenFilesError::Conflict(_) => "conflict",
        OpenFilesError::Unsupported(_) => "unsupported",
        OpenFilesError::Storage(_) => "storage",
        OpenFilesError::Io(_) => "io",
        OpenFilesError::Json(_) => "json",
        OpenFilesError::Toml(_) => "toml",
        OpenFilesError::Internal(_) => "internal",
    }
}

fn error_detail(err: &OpenFilesError) -> String {
    match err {
        OpenFilesError::InvalidPath(detail)
        | OpenFilesError::NotFound(detail)
        | OpenFilesError::Conflict(detail)
        | OpenFilesError::Unsupported(detail)
        | OpenFilesError::Storage(detail)
        | OpenFilesError::Internal(detail) => detail.clone(),

        OpenFilesError::Io(err) => err.to_string(),
        OpenFilesError::Json(err) => err.to_string(),
        OpenFilesError::Toml(err) => err.to_string(),
    }
}

fn error_from_wire(code: &str, detail: String) -> OpenFilesError {
    match code {
        "invalid-path" => OpenFilesError::InvalidPath(detail),
        "not-found" => OpenFilesError::NotFound(detail),
        "conflict" => OpenFilesError::Conflict(detail),
        "unsupported" => OpenFilesError::Unsupported(detail),
        "storage" => OpenFilesError::Storage(detail),
        "internal" => OpenFilesError::Internal(detail),
        other => OpenFilesError::Internal(format!("remote {other} error: {detail}")),
    }
}
