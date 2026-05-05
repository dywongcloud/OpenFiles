# OpenS3Files Object Layout and Metadata Mapping

This document explains what you are seeing in the MinIO Object Browser when OpenS3Files/OpenFiles is backed by an S3-compatible object store.

It covers:

- how user-visible files are stored as objects,
- why the `.openfiles/` directory exists,
- what `.openfiles/meta/*.json` sidecar objects are,
- how metadata mapping works,
- why CLI/API listings hide internal metadata,
- how this interacts with NATS-distributed server mode.

---

## 1. Bucket root view

The OpenFiles bucket is an ordinary object-store bucket. In this example, the bucket is named `openfiles`.

![MinIO bucket root showing user files and the internal .openfiles directory](./assets/minio-openfiles-root.png)

At the root you can see user-facing objects such as:

```text
a.txt
b.txt
dist-1778006036.txt
dist-1778006413.txt
dist-reverse-1778006425.txt
dist-test.txt
hello.sh
hello.txt
ops.sh
script.sh
```

You can also see:

```text
.openfiles/
```

That `.openfiles/` prefix is internal OpenFiles metadata. It is expected to exist in the object store, but normal OpenFiles user APIs should hide it.

For example, this kind of API or CLI listing should show user data, not implementation metadata:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml ls /

curl -i "http://127.0.0.1:8787/v1/list"
```

The internal `.openfiles/` prefix is useful when debugging the backend directly in MinIO, but applications should treat it as reserved.

---

## 2. Internal metadata view: `.openfiles/meta/`

Inside the bucket, OpenFiles stores portable metadata sidecars under:

```text
.openfiles/meta/
```

![MinIO .openfiles/meta view showing JSON sidecar objects](./assets/minio-openfiles-meta.png)

In the screenshot, the metadata objects look like this:

```text
0503527ab8f548dbd6538ed4b589bf085be8a86e5c005d213c28fed82c992c27.json
116b4e2bac3e35befdfae3f3b6eb88911689c30bfe6696d9e6b0139da8cb5e72.json
18b7cb099a9ea3f50ba899b5ba81e0d377a5f3b16f8feeb8b3e58cd4692b993.json
...
```

Those are sidecar metadata objects. They are separate from the actual file bytes.

A normal user object might be:

```text
hello.txt
```

Its metadata is stored separately as a JSON document under `.openfiles/meta/` using a deterministic encoded key, for example:

```text
.openfiles/meta/<stable-path-id>.json
```

The filename is intentionally opaque. In the current implementation it is a stable hash-like identifier derived from the normalized OpenFiles path or object key. This avoids problems with object-store path limits, unusual characters, nested directories, and backend-specific metadata behavior.

---

## 3. Why OpenFiles uses sidecar metadata

Object stores are not POSIX filesystems. They store objects with keys, bytes, and limited metadata, but they do not natively provide full filesystem metadata such as:

- normalized POSIX path metadata,
- file type,
- mode bits / permissions,
- modified time,
- logical size,
- directory inference,
- OpenFiles cache state,
- sync/version metadata,
- conflict handling state.

OpenFiles therefore keeps the object itself as the file content and writes a portable JSON sidecar for filesystem metadata.

Conceptually:

```text
User-visible file:
/hello.txt
        │
        ├── Object bytes:
        │   hello.txt
        │
        └── Metadata sidecar:
            .openfiles/meta/<stable-path-id>.json
```

This keeps OpenFiles portable across S3-compatible systems and other object backends.

---

## 4. Object-to-metadata mapping model

The object mapping is:

```text
OpenFiles path        Object-store key
------------------------------------------------
/hello.txt            hello.txt
/scripts/job.sh       scripts/job.sh
/dir/file.txt         dir/file.txt
```

The metadata sidecar mapping is:

```text
OpenFiles path        Metadata sidecar key
------------------------------------------------
/hello.txt            .openfiles/meta/<id-for-hello.txt>.json
/scripts/job.sh       .openfiles/meta/<id-for-scripts/job.sh>.json
/dir/file.txt         .openfiles/meta/<id-for-dir/file.txt>.json
```

The `<id-for-...>` value is deterministic. The same normalized logical path maps to the same sidecar object name.

A simplified pseudocode version looks like this:

```text
normalized_path = normalize_openfiles_path("/hello.txt")
object_key      = strip_leading_slash(normalized_path)
metadata_id     = stable_hash(object_key)
metadata_key    = ".openfiles/meta/" + metadata_id + ".json"
```

So:

```text
/hello.txt
```

becomes approximately:

```text
object key:
hello.txt

metadata key:
.openfiles/meta/<hash-of-hello.txt>.json
```

The exact hash value is an implementation detail. User code should not depend on the specific filename under `.openfiles/meta/`.

---

## 5. Why metadata names are hashed instead of path-shaped

OpenFiles could have stored metadata as something like:

```text
.openfiles/meta/hello.txt.json
```

But that creates several problems:

1. **Path ambiguity**  
   A file called `hello.txt` and a directory prefix called `hello.txt/` can be difficult to distinguish in object stores.

2. **Escaping complexity**  
   Paths can contain spaces, Unicode, URL-sensitive characters, or object-store-sensitive characters.

3. **Backend portability**  
   Different object stores have different key, metadata, and listing behavior.

4. **Rename behavior**  
   Rename in object stores is implemented as copy + delete. A hashed sidecar makes the logical metadata record explicit and easy to rewrite.

5. **Predictable fixed-length metadata keys**  
   Hash-like IDs keep metadata key length bounded even for deeply nested user paths.

That is why the metadata files in MinIO look like opaque hex JSON filenames.

---

## 6. What a sidecar JSON object represents

Each `.openfiles/meta/*.json` object represents filesystem metadata for one logical OpenFiles path.

A sidecar can contain information equivalent to:

```json
{
  "path": "/hello.txt",
  "key": "hello.txt",
  "kind": "file",
  "size": 20,
  "mode": 420,
  "mtime": 1778000000000000000,
  "version": "...",
  "metadata_version": 1
}
```

The exact JSON fields may evolve, but the role is stable: it maps a logical filesystem path to metadata needed to present object storage as a filesystem.

The actual file bytes are not stored in the sidecar. They live in the normal object key:

```text
hello.txt
```

The sidecar stores the information OpenFiles needs to interpret that object as a file.

---

## 7. Why the bucket shows more objects than the OpenFiles listing

MinIO shows every object prefix in the bucket, including OpenFiles internal metadata.

OpenFiles user listings are intentionally filtered.

For example, MinIO may show:

```text
.openfiles/
a.txt
b.txt
hello.txt
script.sh
```

But OpenFiles should show:

```text
/a.txt
/b.txt
/hello.txt
/script.sh
```

and hide:

```text
/.openfiles
/.openfiles/meta
/.openfiles/meta/<id>.json
```

This keeps internal implementation data from leaking into the user filesystem namespace.

---

## 8. Directory inference

Object stores do not really have directories. They have object keys with prefixes.

For example:

```text
scripts/job.sh
scripts/build.sh
```

OpenFiles can infer a logical directory:

```text
/scripts
```

because multiple object keys share the `scripts/` prefix.

Metadata sidecars help make this more filesystem-like by preserving path, type, and stat information even when the object backend only understands flat keys.

---

## 9. Write flow

When a client writes a file through the CLI or HTTP API:

```bash
cargo run -q -p openfiles-cli -- --config ./openfiles.toml write /hello.txt "hello from OpenFiles"
```

or:

```bash
curl -i -X PUT --data-binary "hello from OpenFiles" \
  "http://127.0.0.1:8787/v1/write/hello.txt"
```

OpenFiles does roughly this:

```text
1. Normalize path:
   /hello.txt

2. Write/update local cache entry:
   content bytes = "hello from OpenFiles"

3. Update metadata entry:
   path, key, kind, size, mtime, dirty state

4. Export/flush to object store:
   hello.txt
   .openfiles/meta/<stable-path-id>.json
```

Depending on configuration and operation, export may happen immediately or through a flush/sync path.

In the NATS-distributed server mode tested here, mutating operations call `flush()` after successful remote write/delete/rename work so changes become visible through the shared MinIO/S3 backend.

---

## 10. Read flow

When a client reads:

```bash
curl -i "http://127.0.0.1:8788/v1/read/hello.txt"
```

OpenFiles does roughly this:

```text
1. Normalize path:
   /hello.txt

2. Check local cache metadata.

3. If clean cached bytes exist locally:
   read from local cache.

4. Otherwise:
   read object bytes from MinIO/S3:
   hello.txt

5. Optionally populate local cache.

6. Return bytes to caller.
```

In distributed mode, reads intentionally stay local to the server process that received the HTTP request. The server still reads from the shared object store when necessary.

This avoids a cross-process cache issue where server A has metadata but server B receives a NATS-distributed read and does not have the same local cache blob.

---

## 11. Delete flow

When deleting:

```bash
curl -i -X DELETE "http://127.0.0.1:8787/v1/delete/hello.txt"
```

OpenFiles removes or marks the logical path as deleted and exports that change to the backend. It also publishes a NATS mutation event when distribution is enabled.

Peer servers receive the mutation event and invalidate clean local cache entries for that path.

Dirty entries are preserved to avoid losing unsynced local work.

---

## 12. Rename flow

Object stores do not provide true atomic POSIX rename.

A logical rename such as:

```text
/from.txt -> /to.txt
```

is implemented as object-store copy + delete plus metadata updates.

Conceptually:

```text
1. Read/copy object:
   from.txt -> to.txt

2. Write/update metadata sidecar:
   .openfiles/meta/<id-for-to.txt>.json

3. Remove old object and old metadata:
   from.txt
   .openfiles/meta/<id-for-from.txt>.json

4. Publish invalidation event:
   invalidate /from.txt and /to.txt on peers
```

This gives filesystem-like behavior to OpenFiles clients, but it should not be treated as a single atomic object-store operation.

---

## 13. NATS distribution and metadata

In the distributed mode tested here:

```text
OpenFiles :8787  ─┐
                  ├── NATS queue group + mutation events
OpenFiles :8788  ─┘
        │
        ▼
Shared MinIO/S3 bucket
```

The object bucket and `.openfiles/meta/` mapping remain the shared source of truth.

NATS does not store files and does not replace the object backend. It is used for:

- distributing mutating work,
- publishing mutation events,
- invalidating clean cache entries across peer instances.

The durable data still lives in:

```text
<user object key>
.openfiles/meta/<stable-path-id>.json
```

---

## 14. Example distributed test result

A successful distributed test looks like this:

```bash
name="dist-$(date +%s).txt"

curl -i -X PUT --data-binary "hello $name" \
  "http://127.0.0.1:8787/v1/write/$name"

for i in $(seq 1 20); do
  curl -s "http://127.0.0.1:8788/v1/read/$name"
  echo
done
```

Expected:

```text
HTTP/1.1 200 OK
{"ok":true,"path":"/dist-<timestamp>.txt"}

hello dist-<timestamp>.txt
hello dist-<timestamp>.txt
hello dist-<timestamp>.txt
...
```

Reverse direction should also work:

```bash
name="dist-reverse-$(date +%s).txt"

curl -i -X PUT --data-binary "hello $name" \
  "http://127.0.0.1:8788/v1/write/$name"

for i in $(seq 1 20); do
  curl -s "http://127.0.0.1:8787/v1/read/$name"
  echo
done
```

That validates:

- both OpenFiles instances can write to the shared backend,
- both can read each other's writes,
- cache invalidation and shared object metadata are working,
- NATS distribution does not compromise the object-store source-of-truth model.

---

## 15. How to inspect metadata manually

You can inspect the metadata prefix through MinIO or `mc`.

List root objects:

```bash
mc ls local/openfiles
```

List OpenFiles metadata:

```bash
mc ls local/openfiles/.openfiles/meta/
```

Read a metadata object:

```bash
mc cat local/openfiles/.openfiles/meta/<metadata-id>.json | jq .
```

Find recently modified metadata:

```bash
mc ls --recursive local/openfiles/.openfiles/meta/ | sort
```

Compare user objects and metadata sidecars:

```bash
mc ls local/openfiles
mc ls local/openfiles/.openfiles/meta/
```

The count does not need to be identical in every moment because:

- directory metadata may exist,
- deleted/conflict records may exist depending on implementation state,
- lost+found or sync bookkeeping may exist,
- writes may be in cache before flush,
- metadata can update independently from file byte size.

---

## 16. Operational rules

Treat the following prefix as reserved:

```text
.openfiles/
```

Do not manually edit, rename, or delete objects under:

```text
.openfiles/meta/
```

unless you are intentionally repairing or debugging the filesystem.

Application data should live outside `.openfiles/`.

Good:

```text
hello.txt
scripts/job.sh
data/input.json
```

Reserved/internal:

```text
.openfiles/
.openfiles/meta/<id>.json
.openfiles-lost+found-<fs-id>/
```

---

## 17. Summary

OpenS3Files uses the object store as the durable source of truth and overlays filesystem semantics using cache plus metadata sidecars.

The root bucket contains user file objects:

```text
hello.txt
script.sh
dist-1778006413.txt
```

The `.openfiles/meta/` prefix contains JSON metadata records:

```text
.openfiles/meta/<stable-path-id>.json
```

The user-facing filesystem hides `.openfiles/`, but MinIO shows it because MinIO displays raw object keys.

This design gives OpenFiles:

- portable POSIX-like metadata,
- vendor-neutral object backend support,
- deterministic path-to-metadata mapping,
- lazy import and range reads,
- batched export,
- safe cache invalidation,
- distributed NATS-based mutation workers,
- a clear object-store source-of-truth model.
