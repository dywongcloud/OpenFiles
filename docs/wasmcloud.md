# wasmCloud integration

OpenFiles is Rust-native and wasmCloud-friendly. The highest-performance deployment is:

1. Run OpenFiles as a node/pod service using `openfiles-server` and, when normal file APIs are needed, `openfiles-fuse --features fuse`.
2. Put the OpenFiles mount/cache directory on a volume that the wasmCloud host can see.
3. Grant components explicit `wasi:filesystem` preopens for that path.

This matches wasmCloud v2's capability model: components do not receive filesystem access by default, and hosts make resources explicit through preopens/links.

## Direct WIT API

`wit/openfiles.wit` defines the OpenFiles contract for direct operations:

- `stat(path)`
- `list(path)`
- `read(path, offset, len)`
- `write(path, bytes)`
- `delete(path)`
- `rename(from, to)`
- `flush()`

A wasmCloud host plugin can implement this interface and call `openfiles-core`. The `openfiles-wasmcloud-host` crate validates configs and emits an example manifest; the WIT contract is ready for a native wasmCloud host plugin once your deployment chooses an exact plugin lifecycle.

## Example component

`examples/wasmcloud/components/http-list-files` is a Rust WASI HTTP component that reads a preopened OpenFiles directory with `std::fs::read_dir`.
