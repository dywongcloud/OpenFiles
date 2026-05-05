# Contributing

OpenFiles is designed as both a standard and an implementation.

## Design principles

1. File tools should work without object-store SDK rewrites.
2. Object stores remain the durable source of truth.
3. Semantics should be portable across vendors.
4. Vendor-specific optimizations must not change observable behavior.
5. wasmCloud components should receive explicit, least-privilege filesystem capabilities.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

For local integration testing:

```bash
docker compose -f examples/docker-compose.yml up -d minio
cargo run -p openfiles-cli -- --config examples/configs/minio.toml write /smoke.txt smoke
cargo run -p openfiles-cli -- --config examples/configs/minio.toml flush
```
