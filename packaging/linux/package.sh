#!/usr/bin/env bash
# Linux package: out/linux/UFlix-x86_64.AppImage (release build + the bundled libmpv in runtimes/linux-x64/native).
# VERSION=x.y.z overrides the VERSION file. Needs the natives (Native/natives.sh fetch) and curl.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
version=${VERSION:-$(tr -d '[:space:]' < VERSION)}
natives=runtimes/linux-x64/native
[ -f "$natives/libmpv.so.2" ] || { echo "package: $natives/libmpv.so.2 missing (run Native/natives.sh fetch)" >&2; exit 1; }
# appimagetool validates the .desktop file with it and fails without it: check before the long build
command -v desktop-file-validate >/dev/null || { echo "package: desktop-file-validate missing (install desktop-file-utils)" >&2; exit 1; }

UFLIX_VERSION="$version" cargo build --release --locked -p uflix

app=out/linux/AppDir
rm -rf "$app"; mkdir -p "$app/usr/bin/runtimes/linux-x64" "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps"
cp target/release/uflix "$app/usr/bin/"
cp -a "$natives" "$app/usr/bin/runtimes/linux-x64/"
cp LICENSE "$app/usr/bin/"
cp packaging/linux/uflix.desktop "$app/usr/share/applications/"
cp packaging/linux/uflix.desktop "$app/"
cp app/ui/uflix.png "$app/usr/share/icons/hicolor/256x256/apps/uflix.png"
cp app/ui/uflix.png "$app/uflix.png"
cat > "$app/AppRun" <<'RUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/uflix" "$@"
RUN
chmod +x "$app/AppRun"

tool=out/linux/appimagetool
if [ ! -x "$tool" ]; then
    curl -fsSL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage
    chmod +x "$tool"
fi
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$app" out/linux/UFlix-x86_64.AppImage
rm -rf "$app"
echo "UFlix $version → out/linux/UFlix-x86_64.AppImage"
