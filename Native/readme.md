# TorrentSharp (`tsdl`)

C API over libtorrent, consumed by `Engines/NativeRunner`. Builds one self-contained library per OS
(libtorrent, Boost and OpenSSL linked statically). Plan and rationale: `doc/CrossPlatformDev.md`.

| Target | Command | Output |
|---|---|---|
| Linux (release build, as CI does it) | `podman run --rm -v "$PWD":/src:Z ubuntu:22.04 /src/Native/build-linux.sh` | `Engines/NativeRunner/runtimes/linux-x64/native/libtsdl.so` |
| Linux (dev, host glibc) | `VCPKG_ROOT=~/vcpkg TSDL_MAX_GLIBC=99 Native/build-linux.sh` | same |
| Windows | `$env:VCPKG_ROOT=...; Native\build-windows.ps1` (VS x64 dev shell) | `Engines/NativeRunner/runtimes/win-x64/native/tsdl.dll` |
| Distro / Flatpak | `cmake --preset linux-x64-system` or `flatpak/tsdl.yml` | no vcpkg, uses an installed libtorrent |

Presets read the toolchain from `$VCPKG_ROOT`; overlay triplets live in `triplets/`.
The binaries are not committed (git-ignored): CI builds them in `.github/actions/native-deps`, cached on the hash of `Native/`. Build them locally before running the app from a fresh clone.

Smoke test (Linux and Windows): `dotnet run --project Engines/NativeRunner.Smoke`.
