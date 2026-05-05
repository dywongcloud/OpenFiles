# Amazon S3 Files public behavior analysis

This document captures the publicly documented behavior OpenFiles emulates.

## Public feature shape

Amazon S3 Files is described as a shared file system that connects AWS compute resources directly with data in S3. It exposes S3 data as files with file-system semantics and low-latency performance while keeping authoritative data in S3.

The feature is built using Amazon EFS. Active metadata and file contents are placed onto high-performance storage on demand. Many large reads are streamed directly from S3, while small/hot data is served from the low-latency layer.

## Semantics implemented in OpenFiles

| S3 Files behavior | OpenFiles implementation |
|---|---|
| Link file system to bucket/prefix | `object_prefix` and vendor config |
| NFS/POSIX-like access | Optional FUSE mount + wasmCloud `wasi:filesystem` volume preopens |
| Low-latency active storage | local/NVMe/shared cache directory |
| Lazy metadata import | `list_dir` imports metadata and optionally small data |
| Large direct reads | `read_range` bypasses cache above `direct_read_threshold_bytes` |
| Import rules | prefix, trigger, and size threshold in `OpenFilesConfig` |
| Expiration rules | data expiration after `sync.expiration.days_after_last_access`; metadata remains |
| 60s batched exports | configurable `export_batch_window_secs` |
| Conflict source of truth | object store wins; local change moves to lost+found |
| Rename/move | local immediate view; export as copy+delete |
| POSIX permissions | portable sidecar metadata plus optional backend metadata |
| Unsupported hard links | hard links are not exposed |

## Where OpenFiles differs

S3 Files' exact performance depends on AWS's internal S3/EFS integration. OpenFiles cannot reproduce infrastructure that it does not control. It instead exposes the knobs needed to approximate the same experience: cache medium, concurrency, direct-read threshold, import rules, export batch window, and colocated wasmCloud volume mounts.

OpenFiles also deliberately uses portable sidecar metadata objects because not all target stores preserve custom object metadata the same way after rewrite operations.
