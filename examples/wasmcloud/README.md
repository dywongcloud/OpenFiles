# wasmCloud examples

OpenFiles is designed for wasmCloud v2 in two modes:

1. **Preopened filesystem mode**: run `openfiles-fuse` or a node-side OpenFiles daemon, then give components a `wasi:filesystem` preopen. This is the highest-throughput path because normal file reads stay inside WASI filesystem calls.
2. **Direct WIT mode**: use `wit/openfiles.wit` when components need explicit object-backed file operations rather than POSIX-style paths.

The manifests here show the preopen mode. Build the sample component with `cargo component build --release` from `components/http-list-files`.
