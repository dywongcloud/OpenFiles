#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
name="openfiles-$(date +%Y%m%d)"
rm -rf "target/$name" "target/$name.zip"
mkdir -p target
rsync -a --exclude target --exclude .git ./ "target/$name/"
(cd target && zip -qr "$name.zip" "$name")
echo "target/$name.zip"
