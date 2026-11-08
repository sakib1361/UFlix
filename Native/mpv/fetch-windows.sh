#!/usr/bin/env bash
# Fetches an LGPL libmpv-2.dll (zhongfly/mpv-winbuild) into runtimes/win-x64/native/ and writes BUILDINFO.txt.
# zhongfly keeps only about a month of nightly releases, so this takes the newest one (x86_64, not the -v3 build); the
# Natives workflow then mirrors the result into this repo's own natives release (Native/natives.lock), which is what builds use.
# MPV_URL (+ optional MPV_SHA256) picks a specific archive instead. Needs curl, sha256sum and 7z. Runs on Linux or Git Bash.
set -euo pipefail

if [ -z "${MPV_URL:-}" ]; then
    MPV_URL=$(curl -fsSL https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest \
        | grep -o '"browser_download_url": *"[^"]*/mpv-dev-lgpl-x86_64-[0-9][^"]*\.7z"' | head -n1 | sed 's/.*"\(https[^"]*\)"$/\1/')
    [ -n "$MPV_URL" ] || { echo "no mpv-dev-lgpl-x86_64 archive in the latest zhongfly/mpv-winbuild release" >&2; exit 1; }
fi
ASSET=$(basename "$MPV_URL")

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/runtimes/win-x64/native"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

curl -fsSL -o "$WORK/$ASSET" "$MPV_URL"
SHA256=$(sha256sum "$WORK/$ASSET" | cut -d' ' -f1)
if [ -n "${MPV_SHA256:-}" ]; then echo "$MPV_SHA256  $WORK/$ASSET" | sha256sum -c -; fi
7z e -y -o"$WORK/x" "$WORK/$ASSET" libmpv-2.dll >/dev/null

mkdir -p "$DEST"
cp "$WORK/x/libmpv-2.dll" "$DEST/libmpv-2.dll"
VERSION=$( (strings "$DEST/libmpv-2.dll" 2>/dev/null || grep -a -o 'mpv v[0-9][^[:space:]]*' "$DEST/libmpv-2.dll") | grep -E '^mpv v[0-9]' | head -n1 || true)
cat > "$DEST/BUILDINFO.txt" <<INFO
libmpv-2.dll  (Windows x64, LGPL)
source:   $MPV_URL
archive sha256: $SHA256
dll sha256:     $(sha256sum "$DEST/libmpv-2.dll" | cut -d' ' -f1)
version:  ${VERSION:-unknown}
licence:  LGPLv2.1+ (mpv built with -Dgpl=false; FFmpeg LGPL). Source: https://github.com/mpv-player/mpv, build scripts: https://github.com/zhongfly/mpv-winbuild
fetched:  $(date -u +%Y-%m-%d) by Native/mpv/fetch-windows.sh
INFO
echo "libmpv-2.dll ($ASSET) -> $DEST"
