# Performance guide

OpenFiles is designed around four hot paths.

## 1. Directory discovery

First access to a directory lists object metadata. This can be slow for large prefixes. After import, metadata reads are local.

Tune:

```toml
[[sync.import_rules]]
prefix = "hot/"
trigger = "on-directory-first-access"
size_less_than = 10485760
```

## 2. Small hot files

Small files benefit from local cache. Use a higher `size_less_than` for prefixes with repeated reads.

## 3. Large sequential reads

Large files should stream from object storage. Keep `direct_read_threshold_bytes` at 1 MiB or lower for analytics/media workloads.

```toml
[cache]
direct_read_threshold_bytes = 1048576
```

## 4. Writes

Repeated writes to the same path are batched before export.

```toml
[sync]
export_batch_window_secs = 60
```

Set the window to `0` for tests or workloads that require immediate object-store visibility.

## Cache placement

Best options:

1. Local NVMe for single-node or sticky workloads.
2. Shared network filesystem for multi-node close-to-open semantics.
3. tmpfs for ephemeral agent workloads.

## Scaling

- Use multiple OpenFiles daemons per prefix for isolation.
- Use prefix-scoped import rules.
- Use object-store native events to trigger external-change refreshes.
- Keep recursive directory renames rare.
