# Developer experience

OpenFiles supports three ways for developers to use object data as files.

## 1. Plain filesystem path

Use FUSE or a shared POSIX cache mount:

```bash
ls /mnt/openfiles
python script.py /mnt/openfiles/data/input.json
```

## 2. wasmCloud WASI filesystem preopen

Components read `/openfiles` exactly like any other preopened WASI directory.

```rust
let text = std::fs::read_to_string("/openfiles/config/app.toml")?;
```

## 3. Direct WIT API

Components can import `openfiles:fs/files` from `wit/openfiles.wit` when they need direct operation-level control.

```wit
read-range: func(path: string, offset: u64, len: u64) -> result<list<u8>, error>
```
