# OpenFiles

OpenFiles is an open source, vendor-neutral object-backed shared file system standard and Rust implementation inspired by the public behavior of Amazon S3 Files.

It gives file-oriented applications, agents, scripts, and WebAssembly components a POSIX-like filesystem view over object stores while keeping the object store as the durable source of truth.

> **Not affiliated with AWS.** OpenFiles does not use AWS proprietary code or private APIs. It implements public, observable semantics: object store as source-of-truth, high-performance local cache, lazy import, batched export, conflict lost+found, POSIX metadata sidecars, and non-atomic object rename behavior.

## What is in this repository

- **OpenFiles Standard (OFSS)**: a written standard for object-backed file semantics, metadata, sync rules, conflict handling, and vendor requirements.
- **Rust core engine**: shared cache, lazy import, range reads, sync/export worker, conflict detection, POSIX metadata encoding, directory inference, and vendor abstraction.
- **Vendor adapters via Apache OpenDAL**: AWS S3, GCP Cloud Storage, Azure Blob, Vercel Blob, Storj Gateway, MinIO, and NetApp StorageGRID/S3-compatible buckets.
- **wasmCloud Rust integration**: a v2-friendly pattern where OpenFiles exposes a host volume that wasmCloud components consume through `wasi:filesystem` preopens, plus a custom WIT interface for direct file operations.
- **CLI and HTTP server**: local development, smoke testing, and non-FUSE access.
- **Optional FUSE mount**: Linux/macOS developer experience for normal file APIs.
- **Examples/bindings**: Python, Node, Go, Rust, and wasmCloud component examples.

## Quick start: MinIO + OpenFiles + normal file APIs

```bash
cd openfiles
cp examples/configs/minio.toml ./openfiles.toml
docker compose -f examples/docker-compose.yml up -d minio createbucket

# Build and run server
cargo run -p openfiles-server -- --config ./openfiles.toml

# In another shell
cargo run -q  -p openfiles-cli -- --config ./openfiles.toml write /hello.txt "hello from OpenFiles"
cargo run -q  -p openfiles-cli -- --config ./openfiles.toml ls /
cargo run -q  -p openfiles-cli -- --config ./openfiles.toml cat /hello.txt
cargo run -q  -p openfiles-cli -- --config ./openfiles.toml flush
```

The MinIO console is available at `http://localhost:9001` with the credentials in `examples/docker-compose.yml`.

## Optional FUSE mount

```bash
# Linux: install libfuse3-dev/fuse3. macOS: install macFUSE.
cargo run -p openfiles-fuse --features fuse -- --config ./openfiles.toml /mnt/openfiles

# Then use regular tools:
echo "standard file IO" > /mnt/openfiles/demo.txt
cat /mnt/openfiles/demo.txt
```

## wasmCloud v2 deployment pattern

OpenFiles runs as a Rust daemon/sidecar on the node or pod, materializes its active working set under a local path, and wasmCloud components receive that path as a `wasi:filesystem` preopen. This aligns with wasmCloud v2's explicit volume model and avoids routing every file read through an RPC boundary.

See:

- `examples/wasmcloud/hostgroup-with-openfiles-volume.yaml`
- `examples/wasmcloud/workload-http-list-files.yaml`
- `examples/wasmcloud/components/http-list-files/`

## Vendor examples

Configuration files are under `examples/configs/`:

| Vendor | Config file | Notes |
|---|---|---|
| AWS S3 | `aws-s3.toml` | Native S3 through OpenDAL. Enable bucket versioning for best conflict behavior. |
| GCP Cloud Storage | `gcp-gcs.toml` | Uses GCS service-account JSON or ADC. |
| Azure Blob | `azure-blob.toml` | Uses account key or SAS token. |
| Vercel Blob | `vercel-blob.toml` | Uses Vercel Blob token. |
| Storj | `storj.toml` | S3-compatible gateway endpoint. |
| MinIO | `minio.toml` | S3-compatible local/dev endpoint. |
| NetApp StorageGRID | `netapp-storagegrid.toml` | S3-compatible endpoint. |

## Performance model

OpenFiles uses the same high-level performance strategy documented for S3 Files:

1. **Low-latency active set**: small/hot files are cached on high-performance local or shared storage.
2. **Direct large reads**: large synchronized reads are served directly from the object store using range requests.
3. **Batched exports**: repeated writes to the same file can be consolidated before object PUT.
4. **Metadata-first directory import**: first directory access indexes object metadata, then selectively imports small files.

OpenFiles cannot guarantee AWS S3 Files' exact throughput or latency because that depends on AWS EFS/S3 internals and region-scale infrastructure. It is designed to reproduce the semantics and developer experience while letting operators choose the cache medium, placement, concurrency, and object backend.

## Repository layout

```text
crates/openfiles-core          Core engine, standard, cache, backend adapters
crates/openfiles-cli           CLI for file operations and smoke tests
crates/openfiles-server        HTTP API + sync daemon
crates/openfiles-fuse          Optional FUSE mount
crates/openfiles-wasmcloud-host Reference custom-host/plugin integration
wit/                           OpenFiles WIT contract
examples/configs/              Vendor configs
examples/bindings/             Python, Node, Go, Rust examples
examples/wasmcloud/            wasmCloud v2 manifests and Rust component
docs/                          Standard, architecture, performance, vendor matrix
```

## Safety and correctness defaults

- The object store is the source of truth on conflicts.
- Conflicting local changes are moved into `.openfiles-lost+found-<fs-id>/` and are not exported until the user explicitly copies them out.
- Rename and move are filesystem-instant but object-store-asynchronous, implemented as copy+delete.
- Hard links are not supported.
- POSIX metadata is stored in portable sidecar metadata objects by default and can additionally be stored as user metadata where the backend supports it.
- Paths are validated against object-store path component and full-key limits.

## License

Apache-2.0. See `LICENSE`.
