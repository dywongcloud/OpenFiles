use anyhow::{Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use openfiles_core::{vendor::build_backend, OpenFilesConfig, OpenFilesEngine};
use std::{io::Read, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "openfiles")]
#[command(about = "Object-backed file-system semantics compatible with the OpenFiles Standard")]
struct Cli {
    /// TOML config file. Defaults to ./openfiles.toml when it exists, otherwise a local demo backend.
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List a directory.
    Ls { path: String },
    /// Print file bytes to stdout.
    Cat { path: String },
    /// Write local file bytes into OpenFiles.
    Put { local: PathBuf, path: String },
    /// Write a UTF-8 string into OpenFiles.
    Write { path: String, data: String },
    /// Write stdin into OpenFiles.
    PutStdin { path: String },
    /// Remove a file.
    Rm { path: String },
    /// Rename a file or directory using copy+delete object semantics.
    Mv { from: String, to: String },
    /// Show file metadata as JSON.
    Stat { path: String },
    /// Export all dirty cached changes to the backend now.
    Flush,
    /// Expire inactive hot-cache bytes according to the config.
    Expire,
    /// Print the resolved config.
    Doctor,
}

fn load_config(path: Option<PathBuf>) -> Result<OpenFilesConfig> {
    let candidate = path.or_else(|| {
        let p = PathBuf::from("openfiles.toml");
        p.exists().then_some(p)
    });
    if let Some(path) = candidate {
        OpenFilesConfig::from_toml_file(&path)
            .with_context(|| format!("failed to load config {}", path.display()))
    } else {
        let mut cfg = OpenFilesConfig::default();
        cfg.backend.root = "./object-store".to_string();
        cfg.cache.dir = PathBuf::from("./.openfiles-cache");
        Ok(cfg)
    }
}

async fn engine(config: OpenFilesConfig) -> Result<OpenFilesEngine> {
    let backend = build_backend(&config.backend)?;
    OpenFilesEngine::new(config, backend)
        .await
        .map_err(Into::into)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "openfiles=info".to_string()))
        .init();

    let cli = Cli::parse();
    let config = load_config(cli.config)?;

    if matches!(&cli.command, Command::Doctor) {
        println!("{}", serde_json::to_string_pretty(&config)?);
        return Ok(());
    }

    let engine = engine(config).await?;

    match cli.command {
        Command::Ls { path } => {
            for entry in engine.list_dir(&path).await? {
                let kind = match entry.kind {
                    openfiles_core::FileKind::Directory => "dir",
                    openfiles_core::FileKind::File => "file",
                    openfiles_core::FileKind::Symlink => "symlink",
                };
                println!("{kind:7} {:>12} {}", entry.size, entry.path);
            }
        }
        Command::Cat { path } => {
            let bytes = engine.read_all(&path).await?;
            print!("{}", String::from_utf8_lossy(&bytes));
        }
        Command::Put { local, path } => {
            let bytes = tokio::fs::read(&local)
                .await
                .with_context(|| format!("failed to read {}", local.display()))?;
            engine.write_file(&path, Bytes::from(bytes)).await?;
            println!("wrote {path}; run `openfiles flush` or wait for the export window");
        }
        Command::Write { path, data } => {
            engine.write_file(&path, Bytes::from(data)).await?;
            println!("wrote {path}; run `openfiles flush` or wait for the export window");
        }
        Command::PutStdin { path } => {
            let mut stdin = std::io::stdin();
            let mut buf = Vec::new();
            stdin.read_to_end(&mut buf)?;
            engine.write_file(&path, Bytes::from(buf)).await?;
            println!("wrote {path}; run `openfiles flush` or wait for the export window");
        }
        Command::Rm { path } => {
            engine.delete_path(&path).await?;
            println!("removed {path}");
        }
        Command::Mv { from, to } => {
            engine.rename_path(&from, &to).await?;
            println!("renamed {from} -> {to}");
        }
        Command::Stat { path } => {
            let stat = engine.stat(&path).await?;
            println!("{}", serde_json::to_string_pretty(&stat)?);
        }
        Command::Flush => {
            let n = engine.flush().await?;
            println!("flushed {n} dirty entries");
        }
        Command::Expire => {
            let n = engine.expire_cache().await?;
            println!("expired {n} cached data files");
        }
        Command::Doctor => unreachable!(),
    }
    Ok(())
}
