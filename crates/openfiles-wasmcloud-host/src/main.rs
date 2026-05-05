//! wasmCloud integration bootstrap.
//!
//! wasmCloud v2 can expose filesystems to components through `wasi:filesystem`
//! preopens and can be extended through host plugins that implement WIT worlds.
//! This binary validates an OpenFiles config and prints the ready-to-apply
//! wasmCloud/Kubernetes wiring used by the examples. The actual filesystem
//! behavior lives in `openfiles-core`, `openfiles-server`, and the optional
//! FUSE adapter.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use openfiles_core::{vendor::build_backend, OpenFilesConfig, OpenFilesEngine};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, default_value = "openfiles.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate backend credentials and cache configuration.
    Validate,
    /// Print a wasmCloud manifest that mounts an OpenFiles volume into a component.
    Manifest {
        #[arg(long, default_value = "/mnt/openfiles")]
        mount: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "openfiles_wasmcloud_host=info".to_string()),
        )
        .init();
    let args = Args::parse();
    let config = OpenFilesConfig::from_toml_file(&args.config)
        .with_context(|| format!("failed to load {}", args.config.display()))?;
    match args.command.unwrap_or(Command::Validate) {
        Command::Validate => {
            let backend = build_backend(&config.backend)?;
            let engine = OpenFilesEngine::new(config, backend).await?;
            let root = engine.stat("/").await?;
            println!("OpenFiles wasmCloud adapter config is valid: {}", root.path);
        }
        Command::Manifest { mount } => print_manifest(&config, &mount),
    }
    Ok(())
}

fn print_manifest(config: &OpenFilesConfig, mount: &str) {
    println!(
        r#"# Generated OpenFiles wasmCloud workload manifest excerpt.
# 1. Run openfiles-server or openfiles-fuse as a sidecar/init container.
# 2. Mount the resulting directory into the wasmCloud host.
# 3. Grant the component a WASI filesystem preopen at the same path.
apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: openfiles-wasmcloud-demo
spec:
  components:
    - name: openfiles-http-list-files
      type: component
      properties:
        image: ghcr.io/openfiles/examples/http-list-files:0.1.0
      traits:
        - type: spreadscaler
          properties:
            replicas: 1
        - type: link
          properties:
            target: wasi-filesystem
            namespace: wasi
            package: filesystem
            interfaces: [types, preopens]
        - type: link
          properties:
            target: wasi-http
            namespace: wasi
            package: http
            interfaces: [incoming-handler]
---
# Kubernetes-style host resource sketch:
apiVersion: k8s.wasmcloud.dev/v1alpha1
kind: WasmCloudHostConfig
metadata:
  name: openfiles-host
spec:
  workload:
    localResources:
      volumeMounts:
        - name: openfiles-cache
          mountPath: "{mount}"
          readOnly: false
      env:
        OPENFILES_MOUNT: "{mount}"
        OPENFILES_FS_ID: "{fs_id}"
"#,
        mount = mount,
        fs_id = config.fs_id
    );
}
