# OpenFiles Standard 0.1 (OFSS)

OFSS defines portable file-system semantics over object stores.

## 1. Namespaces

An OpenFiles file system is bound to one object storage namespace:

```text
vendor + bucket/container + optional root prefix
```

Every file path maps to an object key:

```text
/path/to/file.txt -> <root-prefix>/path/to/file.txt
```

Directory entries are inferred from common prefixes. Directory metadata MAY be stored as sidecar metadata when a directory has no objects beneath it.

## 2. Path limits

Implementations MUST reject:

- `..` path traversal.
- Empty path components except root normalization.
- A component longer than 255 bytes.
- A resulting object key longer than 1024 bytes unless the backend explicitly supports a larger limit and the user opted in.

## 3. File kinds

Required kinds:

- `file`
- `directory`
- `symlink`

Hard links are not supported. A file maps to exactly one object key.

## 4. POSIX metadata

OpenFiles metadata is stored as JSON in sidecar objects under:

```text
.openfiles/meta/<sha256-of-object-key>.json
```

Metadata keys:

```json
{
  "path": "path/to/file.txt",
  "kind": "file",
  "uid": 1000,
  "gid": 1000,
  "mode": 420,
  "mtime_ns": 1780000000000000000,
  "ctime_ns": 1780000000000000000,
  "symlink_target": null,
  "xattrs": {}
}
```

Implementations MAY also write equivalent user-defined object metadata:

- `ofss-uid`
- `ofss-gid`
- `ofss-mode`
- `ofss-mtime-ns`
- `ofss-ctime-ns`
- `ofss-kind`
- `ofss-symlink-target`

The sidecar is authoritative when present because sidecars are portable across object stores that do not preserve custom metadata.

## 5. Import rules

An import rule has:

- `prefix`: matching object prefix. Empty string matches the whole filesystem.
- `trigger`: `on-directory-first-access` or `on-file-access`.
- `size_less_than`: maximum object size automatically copied into the cache.

The most specific matching prefix wins. Exactly one root rule MUST exist.

Default:

```toml
[[sync.import_rules]]
prefix = ""
trigger = "on-directory-first-access"
size_less_than = 131072
```

## 6. Expiration rules

Cached data MAY be removed after it has not been read for `days_after_last_access`. Metadata MUST remain browsable. Default: 30 days. Range: 1 to 365 days.

## 7. Read path

- If data is dirty or recently written in cache, serve from cache.
- If the requested read is larger than `direct_read_threshold_bytes` and the object is synchronized, serve directly from the object store using a range read.
- Otherwise, import data into cache according to the matching import rule.

## 8. Write path

Writes are durable in the cache immediately. Export to the object store is asynchronous and batched. A 60 second export batch window is recommended by default for workloads with repeated writes.

## 9. Conflict handling

A conflict occurs when:

1. A local cached file is dirty, and
2. the object store's version/etag/generation differs from the local base version before export.

Conflict resolution:

- The object store wins.
- The local dirty copy is moved to `.openfiles-lost+found-<fs-id>/`.
- The latest object store version is imported at the original path.
- Lost+found files are not automatically exported.

## 10. Rename and move

A file rename is locally visible immediately but is exported as:

1. copy old object to new key,
2. write new metadata sidecar,
3. delete old object,
4. delete old metadata sidecar.

A directory rename repeats that process for every object under the prefix. Implementations MUST NOT claim object-store atomicity for recursive rename.

## 11. Locks

Locks are advisory. Single-daemon deployments MAY use an in-memory lock table. Multi-node deployments SHOULD use a shared lock backend such as NATS, Redis, etcd, or a vendor-native conditional-write lease object.

## 12. wasmCloud integration

The preferred wasmCloud v2 integration is:

- Run OpenFiles as a Rust service/sidecar or host plugin.
- Expose the cache/mount directory as a Kubernetes Volume or hostPath.
- Grant components explicit `wasi:filesystem` preopens.

A custom WIT API is also defined in `wit/openfiles.wit` for components that should invoke file operations directly rather than using preopened filesystem paths.
