# OpenFiles HTTP API

The HTTP API is intentionally tiny so it can be called from agents, build systems, and language bindings.

Run:

```bash
cargo run -p openfiles-server -- --config openfiles.toml --listen 127.0.0.1:8787
```

Endpoints:

| Method | Path | Description |
|---|---|---|
| GET | `/healthz` | Health check. |
| GET | `/v1/list/{path}` | List a directory. Omit `{path}` for root. |
| GET | `/v1/stat/{path}` | Return file metadata. Omit `{path}` for root. |
| GET | `/v1/read/{path}` | Read full file bytes. |
| GET | `/v1/read/{path}?offset=N&len=M` | Read a byte range. |
| PUT | `/v1/write/{path}` | Write request body as file contents. |
| DELETE | `/v1/delete/{path}` | Delete a file. |
| POST | `/v1/rename` | JSON body `{ "from": "/a", "to": "/b" }`. |
| POST | `/v1/flush` | Export dirty cache entries now. |
| POST | `/v1/expire` | Remove inactive cached bytes according to policy. |

Example:

```bash
curl -X PUT --data-binary @README.md http://127.0.0.1:8787/v1/write/docs/readme.txt
curl http://127.0.0.1:8787/v1/list/docs
curl http://127.0.0.1:8787/v1/read/docs/readme.txt
curl -X POST http://127.0.0.1:8787/v1/flush
```
