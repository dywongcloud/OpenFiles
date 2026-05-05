"""Tiny stdlib OpenFiles HTTP client."""
from __future__ import annotations

import json
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any


@dataclass
class OpenFilesClient:
    base_url: str = "http://127.0.0.1:8787"

    def _url(self, prefix: str, path: str = "") -> str:
        path = path.strip("/")
        if path:
            return f"{self.base_url}{prefix}/{urllib.parse.quote(path)}"
        return f"{self.base_url}{prefix}"

    def list(self, path: str = "/") -> list[dict[str, Any]]:
        with urllib.request.urlopen(self._url("/v1/list", path)) as resp:
            return json.loads(resp.read().decode())

    def stat(self, path: str) -> dict[str, Any]:
        with urllib.request.urlopen(self._url("/v1/stat", path)) as resp:
            return json.loads(resp.read().decode())

    def read(self, path: str, offset: int | None = None, length: int | None = None) -> bytes:
        url = self._url("/v1/read", path)
        if offset is not None and length is not None:
            url += f"?offset={offset}&len={length}"
        with urllib.request.urlopen(url) as resp:
            return resp.read()

    def write(self, path: str, data: bytes) -> None:
        req = urllib.request.Request(self._url("/v1/write", path), data=data, method="PUT")
        with urllib.request.urlopen(req) as resp:
            resp.read()

    def delete(self, path: str) -> None:
        req = urllib.request.Request(self._url("/v1/delete", path), method="DELETE")
        with urllib.request.urlopen(req) as resp:
            resp.read()

    def flush(self) -> int:
        req = urllib.request.Request(f"{self.base_url}/v1/flush", data=b"", method="POST")
        with urllib.request.urlopen(req) as resp:
            return int(json.loads(resp.read().decode())["flushed"])


if __name__ == "__main__":
    fs = OpenFilesClient()
    fs.write("/python-demo.txt", b"hello from python\n")
    print(fs.list("/"))
    print(fs.read("/python-demo.txt").decode())
