#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cp examples/configs/minio.toml openfiles.toml
docker compose -f examples/docker-compose.yml up -d minio createbucket
cargo run -p openfiles-server -- --config openfiles.toml --listen 127.0.0.1:8787
