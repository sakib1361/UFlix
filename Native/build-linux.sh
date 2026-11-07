#!/usr/bin/env bash
# Builds the self-contained libtsdl.so, checks it and installs it with BUILDINFO.txt into
# Engines/NativeRunner/runtimes/linux-x64/native.
#
# Committed builds run inside ubuntu:22.04 (glibc 2.35 is the support floor):
#   podman run --rm -v "$PWD":/src:Z ubuntu:22.04 /src/Native/build-linux.sh
# Dev build on the host (the glibc floor is then the host's, so relax the check):
#   VCPKG_ROOT=~/vcpkg TSDL_MAX_GLIBC=99 Native/build-linux.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
out="$repo/Engines/NativeRunner/runtimes/linux-x64/native"
max_glibc=${TSDL_MAX_GLIBC:-2.35}
preset=linux-x64-release

. /etc/os-release
base_image="$ID:$VERSION_ID"

if [ -f /.dockerenv ] || [ -f /run/.containerenv ]; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq build-essential cmake ninja-build git curl zip unzip tar pkg-config python3 >/dev/null
  export VCPKG_ROOT=${VCPKG_ROOT:-$here/build/vcpkg}
  export VCPKG_DEFAULT_BINARY_CACHE=${VCPKG_DEFAULT_BINARY_CACHE:-$here/build/vcpkg-cache}
  mkdir -p "$VCPKG_DEFAULT_BINARY_CACHE"
fi

git_() { git -c safe.directory='*' "$@"; }

: "${VCPKG_ROOT:?set VCPKG_ROOT to a vcpkg checkout}"
if [ ! -x "$VCPKG_ROOT/vcpkg" ]; then
  [ -d "$VCPKG_ROOT/.git" ] || git_ clone -q https://github.com/microsoft/vcpkg "$VCPKG_ROOT"
  "$VCPKG_ROOT/bootstrap-vcpkg.sh" -disableMetrics
fi
export VCPKG_ROOT

build_dir="$here/build/$preset-$base_image"
cd "$here"
cmake --preset "$preset" -B "$build_dir"
cmake --build "$build_dir"

rm -rf "$out"
mkdir -p "$out"
cmake --install "$build_dir" --strip --prefix "$out"
lib="$out/libtsdl.so"

# Definition of done (doc/CrossPlatformDev.md)
fail() { echo "CHECK FAILED: $*" >&2; exit 1; }

needed=$(readelf -d "$lib" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | sort | tr '\n' ' ')
for n in $needed; do
  case "$n" in libc.so.6|libm.so.6|ld-linux-x86-64.so.2) ;; *) fail "unexpected dependency $n" ;; esac
done

foreign=$(nm -D --defined-only "$lib" | awk '{print $3}' | grep -v '^tsdl_' || true)
[ -z "$foreign" ] || fail "exports outside tsdl_*: $foreign"

glibc=$(objdump -T "$lib" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
[ "$(printf '%s\n%s\n' "$glibc" "$max_glibc" | sort -V | tail -1)" = "$max_glibc" ] \
  || fail "needs glibc $glibc, newer than the $max_glibc floor (build inside ubuntu:22.04)"

commit=$(git_ -C "$repo" rev-parse HEAD)
git_ -C "$repo" diff --quiet HEAD -- Native || commit="$commit+dirty"

cat > "$out/BUILDINFO.txt" <<EOF
libtsdl.so $(sed -n 's/^set(PRODUCT_VERSION_MAJOR \(.*\))/\1/p' CMakeLists.txt).$(sed -n 's/^set(PRODUCT_VERSION_MINOR \(.*\))/\1/p' CMakeLists.txt).$(sed -n 's/^set(PRODUCT_VERSION_PATCH \(.*\))/\1/p' CMakeLists.txt)
built:          $(date -u +%Y-%m-%dT%H:%M:%SZ)
source commit:  $commit
vcpkg commit:   $(git_ -C "$VCPKG_ROOT" rev-parse HEAD)
vcpkg baseline: $(sed -n 's/.*"builtin-baseline": *"\([^"]*\)".*/\1/p' vcpkg.json)
triplet:        x64-linux-tsdl (static deps, release)
compiler:       $(c++ --version | head -1)
base image:     $base_image ($PRETTY_NAME)
glibc floor:    $glibc (highest GLIBC_ symbol version referenced)
needed:         $needed
size:           $(stat -c %s "$lib") bytes
sha256:         $(sha256sum "$lib" | cut -d' ' -f1)
EOF

cat "$out/BUILDINFO.txt"
