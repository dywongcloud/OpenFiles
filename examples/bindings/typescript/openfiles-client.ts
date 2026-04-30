export type FileKind = "file" | "directory" | "symlink";

export interface DirEntry {
  name: string;
  path: string;
  kind: FileKind;
  size: number;
}
type WriteBody = string | Uint8Array | ArrayBuffer | Blob;

function toBodyInit(body: WriteBody): BodyInit {
  if (typeof body === "string") return body;
  if (body instanceof Blob) return body;
  if (body instanceof ArrayBuffer) return body;

  // Uint8Array -> ArrayBuffer slice with correct byteOffset/length
  return body.buffer.slice(
    body.byteOffset,
    body.byteOffset + body.byteLength,
  ) as ArrayBuffer;
}


export interface FileStat extends DirEntry {
  key: string;
  mode: number;
  uid: number;
  gid: number;
  mtime_ns: number;
  ctime_ns: number;
  cached: boolean;
  dirty: boolean;
  etag?: string;
  version?: string;
}

export class OpenFilesClient {
  constructor(public baseUrl = "http://127.0.0.1:8787") {}

  private path(prefix: string, path = "") {
    const clean = path.replace(/^\/+/, "");
    return clean ? `${this.baseUrl}${prefix}/${encodeURIComponent(clean)}` : `${this.baseUrl}${prefix}`;
  }

  async list(path = "/"): Promise<DirEntry[]> {
    const res = await fetch(this.path("/v1/list", path));
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  async stat(path: string): Promise<FileStat> {
    const res = await fetch(this.path("/v1/stat", path));
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  async read(path: string): Promise<Uint8Array> {
    const res = await fetch(this.path("/v1/read", path));
    if (!res.ok) throw new Error(await res.text());
    return new Uint8Array(await res.arrayBuffer());
  }

  async  write(
  baseUrl: string,
  path: string,
  body: WriteBody,
): Promise<void> {
  const res = await fetch(`${baseUrl}/v1/file/${encodeURIComponent(path)}`, {
    method: "PUT",
    body: toBodyInit(body),
    headers: {
      "content-type": typeof body === "string"
        ? "text/plain; charset=utf-8"
        : "application/octet-stream",
    },
  });

  if (!res.ok) {
    throw new Error(`OpenFiles write failed: ${res.status} ${await res.text()}`);
  }
}
  async delete(path: string): Promise<void> {
    const res = await fetch(this.path("/v1/delete", path), { method: "DELETE" });
    if (!res.ok) throw new Error(await res.text());
  }

  async flush(): Promise<number> {
    const res = await fetch(`${this.baseUrl}/v1/flush`, { method: "POST" });
    if (!res.ok) throw new Error(await res.text());
    return (await res.json()).flushed;
  }
}
