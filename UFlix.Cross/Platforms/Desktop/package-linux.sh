#!/usr/bin/env bash
# Builds out/linux/ from the Skia head: UFlix.AppImage (Velopack, updates itself from the GitHub releases) and the
# release feed files (UFlix-<version>-linux-full.nupkg, releases.linux.json). The AppImage is the only Linux package.
# Ships the bundled libmpv from runtimes/linux-x64/native (Native/mpv/build-linux.sh); runs on Ubuntu 24.04+.
# Version: $VERSION, else the VERSION file at the repo root. Velopack needs SemVer.
# $UFlixUpdateRepo (a GitHub repo URL, set by CI) turns on the in-app update check against its releases.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$ROOT/out/linux"
PUBLISH="$OUT/publish"
ICON="$ROOT/UFlix.Cross/Assets/uflix.png"

[ -n "${VERSION:-}" ] || VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "'$VERSION' is not a SemVer version (x.y.z[-pre])" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$PUBLISH"
dotnet publish "$ROOT/UFlix.Cross/UFlix.Cross.csproj" -f net10.0-desktop -c Release -r linux-x64 --self-contained -o "$PUBLISH" -p:Version="$VERSION"
[ -f "$PUBLISH/runtimes/linux-x64/native/libmpv.so.2" ] || { echo "libmpv.so.2 not bundled: run Native/mpv/build-linux.sh" >&2; exit 1; }
[ -f "$PUBLISH/runtimes/linux-x64/native/libtsdl.so" ] || { echo "libtsdl.so not bundled: run Native/build-linux.sh" >&2; exit 1; }
if [ -d "$PUBLISH/runtimes" ] && ls -d "$PUBLISH"/runtimes/* | grep -qv linux-x64; then echo "non-Linux runtimes in the publish output" >&2; exit 1; fi
chmod +x "$PUBLISH/UFlix.Cross"

# AppImage + release feed; vpk is pinned in dotnet-tools.json
(
    cd "$ROOT"
    dotnet tool restore
    dotnet vpk pack --runtime linux-x64 --packId UFlix --packTitle UFlix --packAuthors UFlix --packVersion "$VERSION" \
        --packDir "$PUBLISH" --mainExe UFlix.Cross --icon "$ICON" --categories "AudioVideo;Video;Player" --outputDir "$OUT"
)
rm -rf "$PUBLISH"

echo
echo "UFlix $VERSION"
ls -lh "$OUT"
