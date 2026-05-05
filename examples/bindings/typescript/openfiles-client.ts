export type FileKind = "file" | "directory" | "symlink";

export interface DirEntry {
  name: string;
  path: string;
  kind: FileKind;
  size: number;
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

export type WriteBody = string | Uint8Array | ArrayBuffer | Blob;

function pathComponent(path: string): string {
  return path
    .replace(/^\/+/, "")
    .split("/")
    .filter(Boolean)
    .map(encodeURIComponent)
    .join("/");
}

function toBodyInit(data: WriteBody): BodyInit {
  if (typeof data === "string") return data;
  if (data instanceof Blob) return data;
  if (data instanceof ArrayBuffer) return data;

  // Copy into a fresh ArrayBuffer so strict DOM typings never see
  // Uint8Array<ArrayBufferLike> as an invalid BodyInit.
  const copy = new Uint8Array(data.byteLength);
  copy.set(data);
  return copy.buffer;
}

export class OpenFilesClient {
  constructor(public baseUrl = "http://127.0.0.1:8787") {}

  private path(prefix: string, path = "") {
    const clean = pathComponent(path);
    return clean ? `${this.baseUrl}${prefix}/${clean}` : `${this.baseUrl}${prefix}`;
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

  async write(path: string, data: WriteBody): Promise<void> {
    const res = await fetch(this.path("/v1/write", path), {
      method: "PUT",
      body: toBodyInit(data),
      headers: {
        "content-type": typeof data === "string" ? "text/plain; charset=utf-8" : "application/octet-stream",
      },
    });
    if (!res.ok) throw new Error(await res.text());
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
