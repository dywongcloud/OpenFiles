#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cp examples/configs/local.toml openfiles.toml
rm -rf object-store .openfiles-cache
cargo run -q -p openfiles-cli -- --config openfiles.toml write /hello.txt "hello from smoke"
cargo run -q -p openfiles-cli -- --config openfiles.toml flush
cargo run -q -p openfiles-cli -- --config openfiles.toml ls /
cargo run -q -p openfiles-cli -- --config openfiles.toml cat /hello.txt
printf '\nsmoke-local ok\n'
