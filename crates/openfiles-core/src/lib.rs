//! OpenFiles core engine.
//!
//! This crate implements the OpenFiles Standard semantics: object-backed paths,
//! lazy import, high-performance cache, batched export, conflict lost+found,
//! and portable POSIX metadata sidecars.

pub mod backend;
pub mod cache;
pub mod config;
pub mod engine;
pub mod error;
pub mod metadata;
pub mod sync;
pub mod types;
pub mod vendor;

pub use backend::{LocalFsBackend, ObjectBackend, ObjectMeta, ObjectVersion};
pub use cache::{Cache, CacheEntry};
pub use config::{BackendConfig, OpenFilesConfig, ProviderKind};
pub use engine::OpenFilesEngine;
pub use error::{OpenFilesError, Result};
pub use types::{DirEntry, FileKind, FileStat, ImportDataRule, ImportTrigger};
