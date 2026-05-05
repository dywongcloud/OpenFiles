# Architecture

```text
                    ┌──────────────────────────┐
                    │ wasmCloud Rust component │
                    │  wasi:filesystem preopen │
                    └─────────────┬────────────┘
                                  │ normal file APIs
┌───────────────┐        ┌────────▼─────────┐        ┌─────────────────────┐
│ local tools   │ FUSE   │ OpenFiles cache  │ sync   │ object store backend │
│ Python, Go,   ├────────► metadata + data  ├────────► S3/GCS/Azure/etc.   │
│ CLI, agents   │        │ dirty queue      │        │                     │
└───────────────┘        └──────────────────┘        └─────────────────────┘
```

## Components

### Core engine

`openfiles-core` owns path mapping, cache policy, conflict detection, import/export, and vendor abstraction.

### Cache

The cache has two files per cached path:

- `objects/<sha256>.bin` for cached data.
- `meta/<sha256>.json` for local cache metadata.

Object-store POSIX metadata sidecars are under `.openfiles/meta/` inside the bucket/container.

### Backend

The backend trait is intentionally small:

- `head`
- `read`
- `read_range`
- `write`
- `delete`
- `copy`
- `list`

OpenDAL provides unified drivers for most cloud and S3-compatible stores.

### Sync worker

The sync worker periodically:

1. Flushes dirty cache entries after the batch window.
2. Detects conflicts using etag/version/generation.
3. Writes data and metadata sidecars.
4. Expires inactive cached data while preserving metadata.

## Deployment modes

### Local developer

Use `openfiles-fuse` and a local cache directory.

### wasmCloud/Kubernetes

Run `openfiles-server` as a sidecar or DaemonSet. Mount the cache directory into the wasmCloud host pod and then into components through `localResources.volumeMounts`.

### Multi-node production

Use a shared high-performance cache (NVMe per node plus sticky scheduling, EFS/NFS, CephFS, Lustre, or another POSIX volume). Use a distributed lock/lease service and object-store event integration for faster external-change detection.
