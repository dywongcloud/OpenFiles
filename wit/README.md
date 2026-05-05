# OpenFiles WIT

`openfiles.wit` defines a component-model contract for direct file operations.

Most wasmCloud workloads should use `wasi:filesystem` preopens for the simplest developer experience. Use this WIT interface when a component needs explicit sync, stat, range-read, or admin operations.
