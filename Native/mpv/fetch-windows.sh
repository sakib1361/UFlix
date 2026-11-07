#!/usr/bin/env bash
# Fetches the pinned LGPL libmpv-2.dll (zhongfly/mpv-winbuild) into UFlix.Cross/runtimes/win-x64/native/,
# verifies its sha256 and writes BUILDINFO.txt. Needs curl, sha256sum and 7z. Runs on Linux or Git Bash.
set -euo pipefail

TAG="2026-09-27-e470f8986e"
ASSET="mpv-dev-lgpl-x86_64-20260927-git-e470f8986e.7z"
SHA256="f622d424363b328ac1f7eb7342ffd23ff0b189b9f1de3b91b79b0bb5a34ca86b"
URL="https://github.com/zhongfly/mpv-winbuild/releases/download/$TAG/$ASSET"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/UFlix.Cross/runtimes/win-x64/native"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

curl -fsSL -o "$WORK/$ASSET" "$URL"
echo "$SHA256  $WORK/$ASSET" | sha256sum -c -
7z e -y -o"$WORK/x" "$WORK/$ASSET" libmpv-2.dll >/dev/null

mkdir -p "$DEST"
cp "$WORK/x/libmpv-2.dll" "$DEST/libmpv-2.dll"
cat > "$DEST/BUILDINFO.txt" <<INFO
libmpv-2.dll  (Windows x64, LGPL)
source:   $URL
archive sha256: $SHA256
dll sha256:     $(sha256sum "$DEST/libmpv-2.dll" | cut -d' ' -f1)
version:  $(strings "$DEST/libmpv-2.dll" | grep -E '^mpv v[0-9]' | head -n1)
licence:  LGPLv2.1+ (mpv built with -Dgpl=false; FFmpeg LGPL). Source: https://github.com/mpv-player/mpv, build scripts: https://github.com/zhongfly/mpv-winbuild
fetched:  $(date -u +%Y-%m-%d) by Native/mpv/fetch-windows.sh
INFO
echo "libmpv-2.dll -> $DEST"
