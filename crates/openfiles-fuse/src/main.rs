use anyhow::{Context, Result};
use clap::Parser;
use openfiles_core::{vendor::build_backend, OpenFilesConfig, OpenFilesEngine};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, default_value = "openfiles.toml")]
    config: PathBuf,
    mountpoint: PathBuf,
    #[arg(long, default_value_t = false)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "openfiles_fuse=info".to_string()),
        )
        .init();
    let args = Args::parse();
    let config = OpenFilesConfig::from_toml_file(&args.config)
        .with_context(|| format!("failed to load {}", args.config.display()))?;
    let backend = build_backend(&config.backend)?;
    let engine = OpenFilesEngine::new(config, backend).await?;
    mount(engine, args.mountpoint, args.foreground).await
}

#[cfg(not(feature = "fuse"))]
async fn mount(_engine: OpenFilesEngine, mountpoint: PathBuf, _foreground: bool) -> Result<()> {
    println!(
        "FUSE support is source-included but not enabled in the default build.\n\n\
         Rebuild with:\n  cargo run -p openfiles-fuse --features fuse -- <config> {}\n\n\
         The CLI and HTTP gateway work without FUSE.",
        mountpoint.display()
    );
    Ok(())
}

#[cfg(feature = "fuse")]
async fn mount(engine: OpenFilesEngine, mountpoint: PathBuf, foreground: bool) -> Result<()> {
    use fuse_impl::OpenFilesFuse;
    use fuser::MountOption;
    let fs = OpenFilesFuse::new(engine);
    let mut options = vec![
        MountOption::FSName("openfiles".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
    ];
    if foreground {
        options.push(MountOption::DefaultPermissions);
    }
    fuser::mount2(fs, mountpoint, &options)?;
    Ok(())
}

#[cfg(feature = "fuse")]
mod fuse_impl {
    use bytes::Bytes;
    use fuser::{
        FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
        ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, Request,
    };
    use openfiles_core::{DirEntry, FileKind, FileStat, OpenFilesEngine};
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, UNIX_EPOCH};
    use tokio::runtime::Runtime;

    const TTL: Duration = Duration::from_secs(1);

    pub struct OpenFilesFuse {
        engine: OpenFilesEngine,
        rt: Runtime,
        inodes: Arc<Mutex<HashMap<u64, String>>>,
        paths: Arc<Mutex<HashMap<String, u64>>>,
        next_ino: Arc<Mutex<u64>>,
    }

    impl OpenFilesFuse {
        pub fn new(engine: OpenFilesEngine) -> Self {
            let mut inodes = HashMap::new();
            let mut paths = HashMap::new();
            inodes.insert(1, "/".to_string());
            paths.insert("/".to_string(), 1);
            Self {
                engine,
                rt: Runtime::new().expect("tokio runtime"),
                inodes: Arc::new(Mutex::new(inodes)),
                paths: Arc::new(Mutex::new(paths)),
                next_ino: Arc::new(Mutex::new(2)),
            }
        }

        fn path_for(&self, ino: u64) -> Option<String> {
            self.inodes.lock().ok()?.get(&ino).cloned()
        }

        fn ino_for(&self, path: &str) -> u64 {
            if let Some(ino) = self.paths.lock().unwrap().get(path).copied() {
                return ino;
            }
            let mut next = self.next_ino.lock().unwrap();
            let ino = *next;
            *next += 1;
            self.paths.lock().unwrap().insert(path.to_string(), ino);
            self.inodes.lock().unwrap().insert(ino, path.to_string());
            ino
        }

        fn join(parent: &str, name: &OsStr) -> String {
            let name = name.to_string_lossy();
            if parent == "/" {
                format!("/{name}")
            } else {
                format!("{parent}/{name}")
            }
        }

        fn attr(&self, stat: FileStat) -> FileAttr {
            let ino = self.ino_for(&stat.path);
            let kind = match stat.kind {
                FileKind::Directory => FileType::Directory,
                FileKind::File => FileType::RegularFile,
                FileKind::Symlink => FileType::Symlink,
            };
            let mtime =
                UNIX_EPOCH + Duration::from_nanos(stat.mtime_ns.min(u64::MAX as u128) as u64);
            let ctime =
                UNIX_EPOCH + Duration::from_nanos(stat.ctime_ns.min(u64::MAX as u128) as u64);
            FileAttr {
                ino,
                size: stat.size,
                blocks: (stat.size + 511) / 512,
                atime: mtime,
                mtime,
                ctime,
                crtime: ctime,
                kind,
                perm: (stat.mode & 0o7777) as u16,
                nlink: if matches!(stat.kind, FileKind::Directory) {
                    2
                } else {
                    1
                },
                uid: stat.uid,
                gid: stat.gid,
                rdev: 0,
                flags: 0,
                blksize: 4096,
            }
        }

        fn entry_attr(&self, path: &str) -> Option<FileAttr> {
            match self.rt.block_on(self.engine.stat(path)) {
                Ok(stat) => Some(self.attr(stat)),
                Err(_) => None,
            }
        }
    }

    impl Filesystem for OpenFilesFuse {
        fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
            let Some(parent_path) = self.path_for(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let path = Self::join(&parent_path, name);
            match self.entry_attr(&path) {
                Some(attr) => reply.entry(&TTL, &attr, 0),
                None => reply.error(libc::ENOENT),
            }
        }

        fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
            let Some(path) = self.path_for(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            match self.entry_attr(&path) {
                Some(attr) => reply.attr(&TTL, &attr),
                None => reply.error(libc::ENOENT),
            }
        }

        fn readdir(
            &mut self,
            _req: &Request<'_>,
            ino: u64,
            _fh: u64,
            offset: i64,
            mut reply: ReplyDirectory,
        ) {
            let Some(path) = self.path_for(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            if offset == 0 {
                let _ = reply.add(ino, 1, FileType::Directory, ".");
                let _ = reply.add(1, 2, FileType::Directory, "..");
            }
            let entries: Vec<DirEntry> = match self.rt.block_on(self.engine.list_dir(&path)) {
                Ok(v) => v,
                Err(_) => {
                    reply.error(libc::ENOENT);
                    return;
                }
            };
            for (i, entry) in entries.into_iter().enumerate().skip(offset.max(0) as usize) {
                let ino = self.ino_for(&entry.path);
                let ty = match entry.kind {
                    FileKind::Directory => FileType::Directory,
                    FileKind::File => FileType::RegularFile,
                    FileKind::Symlink => FileType::Symlink,
                };
                if reply.add(ino, (i + 3) as i64, ty, entry.name) {
                    break;
                }
            }
            reply.ok();
        }

        fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
            reply.opened(0, 0);
        }

        fn read(
            &mut self,
            _req: &Request<'_>,
            ino: u64,
            _fh: u64,
            offset: i64,
            size: u32,
            _flags: i32,
            _lock_owner: Option<u64>,
            reply: ReplyData,
        ) {
            let Some(path) = self.path_for(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            match self.rt.block_on(
                self.engine
                    .read_range(&path, offset.max(0) as u64, size as u64),
            ) {
                Ok(bytes) => reply.data(&bytes),
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn write(
            &mut self,
            _req: &Request<'_>,
            ino: u64,
            _fh: u64,
            offset: i64,
            data: &[u8],
            _write_flags: u32,
            _flags: i32,
            _lock_owner: Option<u64>,
            reply: ReplyWrite,
        ) {
            let Some(path) = self.path_for(ino) else {
                reply.error(libc::ENOENT);
                return;
            };
            let existing = if offset > 0 {
                self.rt
                    .block_on(self.engine.read_all(&path))
                    .unwrap_or_default()
                    .to_vec()
            } else {
                Vec::new()
            };
            let mut merged = existing;
            let start = offset.max(0) as usize;
            if merged.len() < start {
                merged.resize(start, 0);
            }
            if merged.len() < start + data.len() {
                merged.resize(start + data.len(), 0);
            }
            merged[start..start + data.len()].copy_from_slice(data);
            match self
                .rt
                .block_on(self.engine.write_file(&path, Bytes::from(merged)))
            {
                Ok(()) => reply.written(data.len() as u32),
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn create(
            &mut self,
            _req: &Request<'_>,
            parent: u64,
            name: &OsStr,
            _mode: u32,
            _umask: u32,
            _flags: i32,
            reply: ReplyCreate,
        ) {
            let Some(parent_path) = self.path_for(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let path = Self::join(&parent_path, name);
            match self
                .rt
                .block_on(self.engine.write_file(&path, Bytes::new()))
            {
                Ok(()) => match self.entry_attr(&path) {
                    Some(attr) => reply.created(&TTL, &attr, 0, 0, 0),
                    None => reply.error(libc::EIO),
                },
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
            let Some(parent_path) = self.path_for(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let path = Self::join(&parent_path, name);
            match self.rt.block_on(self.engine.delete_path(&path)) {
                Ok(()) => reply.ok(),
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn rename(
            &mut self,
            _req: &Request<'_>,
            parent: u64,
            name: &OsStr,
            newparent: u64,
            newname: &OsStr,
            _flags: u32,
            reply: ReplyEmpty,
        ) {
            let Some(parent_path) = self.path_for(parent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let Some(new_parent_path) = self.path_for(newparent) else {
                reply.error(libc::ENOENT);
                return;
            };
            let from = Self::join(&parent_path, name);
            let to = Self::join(&new_parent_path, newname);
            match self.rt.block_on(self.engine.rename_path(&from, &to)) {
                Ok(()) => reply.ok(),
                Err(_) => reply.error(libc::EIO),
            }
        }

        fn flush(
            &mut self,
            _req: &Request<'_>,
            _ino: u64,
            _fh: u64,
            _lock_owner: u64,
            reply: ReplyEmpty,
        ) {
            match self.rt.block_on(self.engine.flush()) {
                Ok(_) => reply.ok(),
                Err(_) => reply.error(libc::EIO),
            }
        }
    }
}
