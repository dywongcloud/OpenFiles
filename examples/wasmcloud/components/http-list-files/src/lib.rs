//! wasmCloud component that lists a preopened OpenFiles directory.
//!
//! Build with:
//!   cargo component build --release
//!
//! The component expects the host to preopen `/mnt/openfiles` or the path in
//! OPENFILES_MOUNT. At runtime, that preopen is backed by OpenFiles.

wit_bindgen::generate!({ world: "http-list-files", path: "wit" });

use exports::wasi::http::incoming_handler::{Guest, IncomingRequest, ResponseOutparam};
use wasi::http::types::{Fields, OutgoingBody, OutgoingResponse};

struct Component;

impl Guest for Component {
    fn handle(_request: IncomingRequest, response_out: ResponseOutparam) {
        let mount = std::env::var("OPENFILES_MOUNT").unwrap_or_else(|_| "/mnt/openfiles".to_string());
        let body = match std::fs::read_dir(&mount) {
            Ok(entries) => {
                let mut out = String::new();
                for entry in entries.flatten() {
                    let meta = entry.metadata().ok();
                    let kind = meta.as_ref().map(|m| if m.is_dir() { "dir" } else { "file" }).unwrap_or("unknown");
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    out.push_str(&format!("{kind:7} {size:>12} {}\n", entry.path().display()));
                }
                out
            }
            Err(err) => format!("failed to list {mount}: {err}\n"),
        };

        let headers = Fields::new();
        headers.set(&"content-type".to_string(), &["text/plain; charset=utf-8".as_bytes().to_vec()]).ok();
        let response = OutgoingResponse::new(headers);
        response.set_status_code(200).ok();
        let out_body = response.body().expect("response body");
        ResponseOutparam::set(response_out, Ok(response));
        let stream = out_body.write().expect("body stream");
        stream.blocking_write_and_flush(body.as_bytes()).ok();
        drop(stream);
        OutgoingBody::finish(out_body, None).ok();
    }
}

export!(Component);
