#!/usr/bin/env bash
# The native library (libmpv) are not in git: they are the assets of a prerelease in this repo, pinned in natives.lock.
#   Native/natives.sh fetch [rid]   download the pinned set, check its sha256, unpack it into the runtimes/ folders
#   Native/natives.sh pack [rid]    pack the local runtimes/ folders into out/natives/natives-<rid>.tar.gz (a new set)
# rid: linux-x64 or win-x64, default this machine's. Runs on Linux and in Git Bash on Windows.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
die() { echo "natives: $*" >&2; exit 1; }

. "$here/natives.lock"
rid=${2:-}
if [ -z "$rid" ]; then
    case "$(uname -s)" in
        Linux) rid=linux-x64 ;;
        MINGW*|MSYS*|CYGWIN*) rid=win-x64 ;;
        *) die "unsupported OS $(uname -s)" ;;
    esac
fi
case $rid in linux-x64|win-x64) ;; *) die "unknown rid $rid (linux-x64 or win-x64)" ;; esac

dirs=("runtimes/$rid/native")
asset="natives-$rid.tar.gz"
shavar="SHA256_${rid//-/_}"

case ${1:-} in
    fetch)
        [ -n "$NATIVES_TAG" ] && [ -n "${!shavar}" ] || die "no native set pinned in natives.lock yet: run the Natives workflow"
        tmp=$(mktemp -d)
        trap 'rm -rf "$tmp"' EXIT
        url="https://github.com/$NATIVES_REPO/releases/download/$NATIVES_TAG/$asset"
        echo "natives: $url"
        curl -fL --retry 3 -o "$tmp/$asset" "$url"
        echo "${!shavar}  $tmp/$asset" | sha256sum -c --quiet - || die "sha256 mismatch for $asset"
        for d in "${dirs[@]}"; do rm -rf "${repo:?}/$d"; done
        tar -C "$repo" -xzf "$tmp/$asset"
        for d in "${dirs[@]}"; do echo "  $d: $(ls "$repo/$d" | tr '\n' ' ')"; done
        ;;
    pack)
        for d in "${dirs[@]}"; do [ -n "$(ls -A "$repo/$d" 2>/dev/null)" ] || die "$d is empty: build or fetch it first"; done
        mkdir -p "$repo/out/natives"
        tar -C "$repo" -czf "$repo/out/natives/$asset" "${dirs[@]}"
        echo "$(sha256sum "$repo/out/natives/$asset" | cut -d' ' -f1)  out/natives/$asset"
        ;;
    *)
        die "usage: natives.sh fetch|pack [linux-x64|win-x64]"
        ;;
esac
