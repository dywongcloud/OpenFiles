# OpenS3Files

OpenS3Files is an open source, vendor-neutral object-backed shared file system standard and Rust implementation inspired by the public behavior of Amazon S3 Files.

It gives file-oriented applications, agents, scripts, HTTP services, FUSE mounts, and WebAssembly components a POSIX-like filesystem view over object stores while keeping the object store as the durable source of truth.

> **Not affiliated with AWS.** OpenS3Files / OpenFiles does not use AWS proprietary code or private APIs. It implements public, observable semantics: object store as source-of-truth, high-performance local cache, lazy import, batched export, conflict lost+found, POSIX metadata sidecars, and non-atomic object rename behavior.

---

## What is in this repository

- **OpenFiles Standard (OFSS)**: a written standard for object-backed file semantics, metadata, sync rules, conflict handling, and vendor requirements.
- **Rust core engine**: shared cache, lazy import, range reads, sync/export worker, conflict detection, POSIX metadata encoding, directory inference, and vendor abstraction.
- **Vendor adapters via Apache OpenDAL**: AWS S3, GCP Cloud Storage, Azure Blob, Vercel Blob, Storj Gateway, MinIO, Tencent COS, and NetApp StorageGRID/S3-compatible buckets.
- **Optional NATS distribution layer**: distribute mutating file operations across multiple OpenFiles server instances using NATS queue groups, plus mutation-event cache invalidation.
- **wasmCloud Rust integration**: a v2-friendly pattern where OpenFiles exposes a host volume that wasmCloud components consume through `wasi:filesystem` preopens, plus a custom WIT interface for direct file operations.
- **CLI and HTTP server**: local development, smoke testing, and non-FUSE access.
- **Optional FUSE mount**: Linux/macOS developer experience for normal file APIs.
- **Examples/bindings**: Python, Node, Go, Rust, and wasmCloud component examples.

---

## Architecture

```text
                  ┌──────────────────────────────┐
                  │ File-oriented apps / agents   │
                  │ CLI / HTTP / FUSE / WASI      │
                  └───────────────┬──────────────┘
                                  │
                                  ▼
                  ┌──────────────────────────────┐
                  │ OpenFiles engine              │
                  │ POSIX-like path semantics      │
                  │ cache + sync + metadata        │
                  └───────────────┬──────────────┘
                                  │
                                  ▼
                  ┌──────────────────────────────┐
                  │ Object store source of truth   │
                  │ S3 / MinIO / COS / GCS / Blob  │
                  └──────────────────────────────┘
```

With optional NATS distribution:

```text
                 ┌──────────────────────┐
                 │ OpenFiles :8787       │
                 │ HTTP gateway + worker │
                 └──────────┬───────────┘
                            │
                            │ NATS queue group
                            │ + mutation events
                            ▼
                 ┌──────────────────────┐
                 │ OpenFiles :8788       │
                 │ HTTP gateway + worker │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ Shared MinIO / S3     │
                 │ durable source truth  │
                 └──────────────────────┘
```

---

## Quick start: MinIO + OpenFiles + normal file APIs

```bash
cd openfiles
cp examples/configs/minio.toml ./openfiles.toml
docker compose -f examples/docker-compose.yml up -d minio createbucket

# Build and run server
cargo run -p openfiles-server -- --config ./openfiles.toml

# In another shell
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /hello.txt "hello from OpenFiles"
cargo run -q -p openfiles-cli -- --config ./openfiles.toml ls /
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /hello.txt
cargo run -q -p openfiles-cli -- --config ./openfiles.toml flush
```

The MinIO console is available at:

```text
http://localhost:9001
```

Use the credentials from `examples/docker-compose.yml`.

---

## Quick start without Docker Desktop

On macOS, if Docker Hub or Podman image pulls are unavailable, run MinIO directly with Homebrew.

```bash
brew install minio/stable/minio
mkdir -p /tmp/openfiles-minio

MINIO_ROOT_USER=minioadmin MINIO_ROOT_PASSWORD=minioadmin \
  minio server /tmp/openfiles-minio --address 127.0.0.1:9000 --console-address 127.0.0.1:9001
```

In another terminal, create the bucket:

```bash
brew install minio/stable/mc

mc alias set local http://127.0.0.1:9000 minioadmin minioadmin
mc mb --ignore-existing local/openfiles
```

Then run OpenFiles:

```bash
cargo run -p openfiles-server -- --config ./openfiles.toml --listen 127.0.0.1:8787
```

---

## Optional NATS distributed mode

OpenS3Files can use NATS to distribute mutating work across multiple `openfiles-server` instances.

This mode is useful when you want multiple HTTP gateways or workers sharing one object-backed filesystem.

### What NATS distribution does

- Uses a NATS work subject and queue group.
- Lets multiple `openfiles-server` processes subscribe as workers.
- Distributes mutating operations such as write, delete, rename, flush, and expire.
- Publishes mutation events so peer processes can invalidate clean local cache entries.
- Keeps the object store as the durable source of truth.

### What stays local

Reads are intentionally served by the local process engine, even when NATS is enabled.

Reason: OpenFiles cache files are per process, while the object store is shared. Routing reads through a NATS queue group can send the read to a worker whose cache metadata references a local cache blob that does not exist on that specific process. Local reads avoid that cross-process cache-file mismatch while still reading from the shared MinIO/S3 source of truth.

### Install and start NATS

Using Homebrew:

```bash
brew install nats-server
nats-server -p 4222
```

Verify NATS is listening:

```bash
nc -vz 127.0.0.1 4222
```

Expected:

```text
Connection to 127.0.0.1 port 4222 succeeded
```

### Enable NATS in `openfiles.toml`

Add or update:

```toml
[nats]
enabled = true
url = "nats://127.0.0.1:4222"
subject_prefix = "openfiles"
request_timeout_ms = 30000
max_payload_bytes = 10485760
publish_events = true

# Optional. If omitted, the server derives a queue group from subject_prefix and fs_id.
# queue_group = "openfiles.default.workers"

# Optional. If omitted, each server generates a UUID.
# instance_id = "node-1"
```

### Start two OpenFiles server instances

Terminal 1:

```bash
nats-server -p 4222
```

Terminal 2:

```bash
cargo run -p openfiles-server -- --config ./openfiles.toml --listen 127.0.0.1:8787
```

Terminal 3:

```bash
cargo run -p openfiles-server -- --config ./openfiles.toml --listen 127.0.0.1:8788
```

### Distributed validation test

Write through node `8787`, then read through node `8788`:

```bash
name="dist-$(date +%s).txt"

curl -i -X PUT --data-binary "hello $name" \
  "http://127.0.0.1:8787/v1/write/$name"

for i in $(seq 1 20); do
  curl -s "http://127.0.0.1:8788/v1/read/$name"
  echo
done
```

Expected output:

```text
hello dist-<timestamp>.txt
hello dist-<timestamp>.txt
hello dist-<timestamp>.txt
...
```

Reverse direction:

```bash
name="dist-reverse-$(date +%s).txt"

curl -i -X PUT --data-binary "hello $name" \
  "http://127.0.0.1:8788/v1/write/$name"

for i in $(seq 1 20); do
  curl -s "http://127.0.0.1:8787/v1/read/$name"
  echo
done
```

Expected output:

```text
hello dist-reverse-<timestamp>.txt
hello dist-reverse-<timestamp>.txt
hello dist-reverse-<timestamp>.txt
...
```

### Useful distributed-mode checks

List files through each instance:

```bash
curl -i "http://127.0.0.1:8787/v1/list"
curl -i "http://127.0.0.1:8788/v1/list"
```

Read a file through each instance:

```bash
curl -i "http://127.0.0.1:8787/v1/read/hello.txt"
curl -i "http://127.0.0.1:8788/v1/read/hello.txt"
```

Write through one instance and flush through another:

```bash
curl -i -X PUT --data-binary "distributed flush test" \
  "http://127.0.0.1:8787/v1/write/flush-test.txt"

curl -i -X POST "http://127.0.0.1:8788/v1/flush"

curl -i "http://127.0.0.1:8787/v1/read/flush-test.txt"
```

### Troubleshooting distributed mode

#### `failed to connect to NATS at nats://127.0.0.1:4222`

NATS is not running or not reachable.

```bash
nats-server -p 4222
nc -vz 127.0.0.1 4222
```

#### `storage error ... http://127.0.0.1:9000/openfiles/...`

MinIO/S3 is not running, the bucket does not exist, or `openfiles.toml` points at the wrong endpoint.

```bash
mc alias set local http://127.0.0.1:9000 minioadmin minioadmin
mc mb --ignore-existing local/openfiles
mc ls local/openfiles
```

#### `404 Not Found`

Use the actual HTTP routes:

```bash
/v1/write/{path}
/v1/read/{path}
/v1/list
/v1/list/{path}
/v1/stat
/v1/stat/{path}
/v1/delete/{path}
/v1/rename
/v1/flush
/v1/expire
/healthz
```

Example:

```bash
curl -i -X PUT --data-binary "hello" \
  "http://127.0.0.1:8787/v1/write/demo.txt"

curl -i "http://127.0.0.1:8788/v1/read/demo.txt"
```

#### `remote io error: No such file or directory`

This can happen if distributed reads are routed through NATS and a remote worker has stale cache metadata but not the local cache blob.

The current recommended behavior is:

- NATS distributes writes, delete, rename, flush, expire, and cache invalidation.
- Reads stay local and use the shared object store as source of truth.

---

# OpenFiles Script Execution Examples

## Mental model

```text
OpenFiles object store / MinIO / S3 / provider
        ↓ cat
Local shell: sh / bash / zsh
        ↓ executes
OpenFiles CLI
        ↓ performs filesystem operations
OpenFiles object store / MinIO / S3 / provider
```

Scripts are stored in OpenFiles, but executed by the host shell.

---

## 1. Create and run a simple script

Create a script in OpenFiles:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /hello.sh '#!/bin/sh
echo "Hello from OpenFiles!"
'
```

Execute it:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /hello.sh | sh
```

---

## 2. Script that interacts with OpenFiles

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /ops.sh '#!/bin/sh

OF="cargo run -q -p openfiles-cli -- --config ./openfiles.toml"

echo "Creating files..."
$OF write /a.txt "A"
$OF write /b.txt "B"

echo "Listing OpenFiles root:"
$OF ls /

echo "Reading files:"
$OF cat /a.txt
echo
$OF cat /b.txt
echo

$OF flush
'
```

Run:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /ops.sh | sh
```

---

## 3. Execute script and persist output back to OpenFiles

Create a job script:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /job.sh '#!/bin/sh
echo "Job started"
date
echo "Processing..."
sleep 1
echo "Done"
'
```

Run and capture output:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /job.sh | sh > /tmp/job.out
```

Upload output back into OpenFiles:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /job-output.txt "$(cat /tmp/job.out)"
```

Read the result:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /job-output.txt
```

---

## 4. Pipe OpenFiles data into Unix tools

Word count:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /hello.txt | wc -w
```

Uppercase transform:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /hello.txt | tr a-z A-Z
```

Grep:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /hello.txt | grep OpenFiles
```

---

## 5. One-liner remote execution pattern

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /script.sh | sh
```

---

## 6. Multi-step pipeline script

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /pipeline.sh '#!/bin/sh

OF="cargo run -q -p openfiles-cli -- --config ./openfiles.toml"

echo "Generating data..."
$OF write /data.txt "alpha beta gamma delta"

echo "Processing data..."
RESULT=$($OF cat /data.txt | wc -w)

echo "Word count: $RESULT"

$OF write /result.txt "count=$RESULT"
$OF flush
'
```

Run:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /pipeline.sh | sh
```

Verify:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /result.txt
```

---

## 7. Developer ergonomics with an alias

```bash
alias of='cargo run -q -p openfiles-cli -- --config ./openfiles.toml'
```

Then:

```bash
of write /test.sh '#!/bin/sh
echo hi
'

of cat /test.sh | sh
of ls /
```

---

## Important script execution note

Scripts are stored in OpenFiles but executed by the host shell.

- `ls /` means the host filesystem.
- `of ls /` means the OpenFiles filesystem.
- `cat /file.txt` means the host filesystem.
- `of cat /file.txt` means the OpenFiles filesystem.

Always use the OpenFiles CLI inside scripts when you want to interact with OpenFiles.

---

## HTTP API

The server exposes simple HTTP routes:

| Method | Route | Description |
|---|---|---|
| `GET` | `/healthz` | Health check |
| `GET` | `/v1/stat` | Stat root |
| `GET` | `/v1/stat/{path}` | Stat path |
| `GET` | `/v1/list` | List root |
| `GET` | `/v1/list/{path}` | List directory |
| `GET` | `/v1/read/{path}` | Read file |
| `PUT` | `/v1/write/{path}` | Write file body |
| `DELETE` | `/v1/delete/{path}` | Delete path |
| `POST` | `/v1/rename` | Rename path |
| `POST` | `/v1/flush` | Flush dirty cache to backend |
| `POST` | `/v1/expire` | Expire inactive clean cache data |

Examples:

```bash
curl -i "http://127.0.0.1:8787/healthz"

curl -i -X PUT --data-binary "hello http" \
  "http://127.0.0.1:8787/v1/write/http-demo.txt"

curl -i "http://127.0.0.1:8787/v1/read/http-demo.txt"

curl -i "http://127.0.0.1:8787/v1/list"

curl -i -X POST "http://127.0.0.1:8787/v1/flush"
```

---

## Optional FUSE mount

```bash
# Linux: install libfuse3-dev/fuse3.
# macOS: install macFUSE.
cargo run -p openfiles-fuse --features fuse -- --config ./openfiles.toml /mnt/openfiles

# Then use regular tools:
echo "standard file IO" > /mnt/openfiles/demo.txt
cat /mnt/openfiles/demo.txt
```

---

## wasmCloud v2 deployment pattern

OpenFiles runs as a Rust daemon or sidecar on the node or pod, materializes its active working set under a local path, and wasmCloud components receive that path as a `wasi:filesystem` preopen.

This aligns with wasmCloud v2's explicit volume model and avoids routing every file read through an RPC boundary.

See:

- `examples/wasmcloud/hostgroup-with-openfiles-volume.yaml`
- `examples/wasmcloud/workload-http-list-files.yaml`
- `examples/wasmcloud/components/http-list-files/`

---

## Vendor examples

Configuration files are under `examples/configs/`.

| Vendor | Config file | Notes |
|---|---|---|
| Tencent COS | `tencent-cos.toml` | S3-compatible gateway endpoint. |
| AWS S3 | `aws-s3.toml` | Native S3 through OpenDAL. Enable bucket versioning for best conflict behavior. |
| GCP Cloud Storage | `gcp-gcs.toml` | Uses GCS service-account JSON or ADC. |
| Azure Blob | `azure-blob.toml` | Uses account key or SAS token. |
| Vercel Blob | `vercel-blob.toml` | Uses Vercel Blob token. |
| Storj | `storj.toml` | S3-compatible gateway endpoint. |
| MinIO | `minio.toml` | S3-compatible local/dev endpoint. |
| NetApp StorageGRID | `netapp-storagegrid.toml` | S3-compatible endpoint. |

---

## Performance model

OpenFiles uses the same high-level performance strategy documented for S3 Files:

1. **Low-latency active set**: small and hot files are cached on high-performance local or shared storage.
2. **Direct large reads**: large synchronized reads are served directly from the object store using range requests.
3. **Batched exports**: repeated writes to the same file can be consolidated before object PUT.
4. **Metadata-first directory import**: first directory access indexes object metadata, then selectively imports small files.
5. **Distributed mutation workers**: when NATS is enabled, mutating work can be spread across multiple OpenFiles server instances.

OpenFiles cannot guarantee AWS S3 Files' exact throughput or latency because that depends on AWS EFS/S3 internals and region-scale infrastructure. It is designed to reproduce the semantics and developer experience while letting operators choose the cache medium, placement, concurrency, and object backend.

---

## Correctness and conflict model

OpenFiles follows these correctness defaults:

- The object store is the source of truth on conflicts.
- Conflicting local changes are moved into `.openfiles-lost+found-<fs-id>/` and are not exported until the user explicitly copies them out.
- Rename and move are filesystem-instant but object-store-asynchronous, implemented as copy+delete.
- Hard links are not supported.
- POSIX metadata is stored in portable sidecar metadata objects by default and can additionally be stored as user metadata where the backend supports it.
- Paths are validated against object-store path component and full-key limits.
- NATS mutation events invalidate clean cache entries on peer instances.
- Dirty local cache entries are preserved to avoid losing unsynced work.

---

## CRDT notes

OpenFiles does not use CRDTs for arbitrary file bytes by default.

That is intentional. For arbitrary binary files, automatic conflict-free merge is unsafe without application-specific merge semantics. OpenFiles instead uses:

- object store as the source of truth,
- sidecar metadata,
- explicit conflict detection,
- lost+found recovery for conflicting local writes,
- optional distributed cache invalidation with NATS.

A CRDT layer can be added above OpenFiles for specific file types such as JSON documents, text documents, collaborative editor states, or append-only logs, but it should not be applied blindly to all file contents.

---

## Repository layout

```text
crates/openfiles-core                Core engine, standard, cache, backend adapters
crates/openfiles-cli                 CLI for file operations and smoke tests
crates/openfiles-server              HTTP API, sync daemon, optional NATS distribution
crates/openfiles-fuse                Optional FUSE mount
crates/openfiles-wasmcloud-host      Reference custom-host/plugin integration
wit/                                 OpenFiles WIT contract
examples/configs/                    Vendor configs
examples/bindings/                   Python, Node, Go, Rust examples
examples/wasmcloud/                  wasmCloud v2 manifests and Rust component
docs/                                Standard, architecture, performance, vendor matrix
```

---

## Development

Format:

```bash
cargo fmt
```

Check:

```bash
cargo check
```

Check only the server:

```bash
cargo check -p openfiles-server
```

Run CLI smoke test:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /smoke.txt "smoke"
cargo run -q -p openfiles-cli -- --config ./openfiles.toml cat /smoke.txt
cargo run -q -p openfiles-cli -- --config ./openfiles.toml flush
```

Run server smoke test:

```bash
cargo run -p openfiles-server -- --config ./openfiles.toml --listen 127.0.0.1:8787
```

In another terminal:

```bash
curl -i -X PUT --data-binary "hello server" \
  "http://127.0.0.1:8787/v1/write/server-smoke.txt"

curl -i "http://127.0.0.1:8787/v1/read/server-smoke.txt"
```

---

## License

Apache-2.0. See `LICENSE`.
