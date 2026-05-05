# http-list-files wasmCloud component

This component lists the OpenFiles mount exposed through a WASI filesystem preopen and returns the listing over WASI HTTP.

```bash
cargo install cargo-component
cargo component build --release
wash app deploy ../../workload-http-list-files.yaml
```

Set `OPENFILES_MOUNT=/mnt/openfiles` in the host workload and mount OpenFiles there with the FUSE adapter or an equivalent node-local volume.
