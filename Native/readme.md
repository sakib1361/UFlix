# TorrentSharp (`tsdl`)

C API over libtorrent, consumed by `Engines/NativeRunner`. Builds one self-contained library per OS
(libtorrent, Boost and OpenSSL linked statically). Plan and rationale: `doc/CrossPlatformDev.md`.

| Target | Command | Output |
|---|---|---|
| Linux (release build, as CI does it) | `podman run --rm -v "$PWD":/src:Z ubuntu:24.04 bash /src/Native/build-linux.sh` | `Engines/NativeRunner/runtimes/linux-x64/native/libtsdl.so` |
| Linux (dev, host glibc) | `VCPKG_ROOT=~/vcpkg TSDL_MAX_GLIBC=99 Native/build-linux.sh` | same |
| Windows | `$env:VCPKG_ROOT=...; Native\build-windows.ps1` (VS x64 dev shell) | `Engines/NativeRunner/runtimes/win-x64/native/tsdl.dll` |
| Distro / Flatpak | `cmake --preset linux-x64-system` or `flatpak/tsdl.yml` | no vcpkg, uses an installed libtorrent |

Presets read the toolchain from `$VCPKG_ROOT`; overlay triplets live in `triplets/`.
The native libraries (libmpv, tsdl) are not in git. They are mirrored as the assets of a `natives-N` prerelease in this repo, pinned by `Native/natives.lock` (tag + sha256 per OS). `Native/natives.sh fetch` downloads and checks the pinned set (CI does this on every build); the **Natives** workflow (`.github/workflows/natives.yml`, run by hand) builds Linux libmpv and tsdl from source, fetches the newest Windows libmpv, publishes a new set and commits the new pin. After changing `Native/`, run the Natives workflow (or build locally, `Native/natives.sh pack <rid>`, and upload the tarball to a new `natives-N` prerelease by hand, updating `natives.lock`).

Smoke test (Linux and Windows): `dotnet run --project Engines/NativeRunner.Smoke`.
