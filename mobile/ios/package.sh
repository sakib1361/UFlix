#!/usr/bin/env bash
# Builds out/ios/UFlix-mobile.ipa (unsigned) on macOS with Xcode's iOS SDK. Sign and install it from any OS with a
# free Apple ID (Impactor / SideStore / xtool); the signature lasts 7 days.
#   mobile/ios/package.sh            (VERSION=x.y.z BUILD=n override the VERSION file / 1)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="$repo/out/ios"
libs="$out/xcframeworks"
version="${VERSION:-$(tr -d '[:space:]' < "$repo/VERSION")}"
build="${BUILD:-1}"

# 1. MPVKit's static libraries (pinned + checksummed), unpacked once
mkdir -p "$libs" "$out/zips"
grep -v '^#' "$here/mpvkit.lock" | while read -r sum url; do
    zip="$out/zips/$(basename "$url")"
    if [ ! -f "$zip" ] || [ "$(shasum -a 256 "$zip" | cut -c1-64)" != "$sum" ]; then
        curl -fsSL --retry 3 -o "$zip" "$url"
        [ "$(shasum -a 256 "$zip" | cut -c1-64)" = "$sum" ] || { echo "checksum mismatch: $url" >&2; exit 1; }
    fi
    unzip -oq "$zip" -d "$libs" # <Name>.xcframework/; build.rs picks its ios-arm64 slice
done

# toolchain facts for the CI log (OpenGLES: libmpv's GL interop references EAGLContext, see eagl_stub.m)
xcodebuild -version | head -1
sdk="$(xcrun --sdk iphoneos --show-sdk-path)"; echo "iOS SDK: $sdk"
grep -c EAGLContext "$sdk/System/Library/Frameworks/OpenGLES.framework/OpenGLES.tbd" 2>/dev/null || echo "OpenGLES.tbd: no EAGLContext"

# 2. the app binary (the Slint UI uses Skia on Metal; skia-bindings fetches its prebuilt iOS library)
rustup target add aarch64-apple-ios >/dev/null
export IPHONEOS_DEPLOYMENT_TARGET=15.0
UFLIX_IOS_LIBS="$libs" cargo build --release --target aarch64-apple-ios --manifest-path "$repo/mobile/Cargo.toml"
bin="$repo/mobile/target/aarch64-apple-ios/release/uflix-mobile"

# 3. Payload/UFlix.app → .ipa
app="$out/Payload/UFlix.app"
rm -rf "$out/Payload" "$out/UFlix-mobile.ipa"
mkdir -p "$app"
cp "$bin" "$app/UFlix"
sed -e "s/__VERSION__/$version/" -e "s/__BUILD__/$build/" "$here/Info.plist" > "$app/Info.plist"
plutil -lint "$app/Info.plist"
printf 'APPL????' > "$app/PkgInfo"
(cd "$out" && zip -qry UFlix-mobile.ipa Payload)
ls -lh "$out/UFlix-mobile.ipa"
