# Native libraries

UFlix bundles one native library, **libmpv** (never the system copy). Torrents use librqbit (pure Rust), so nothing else is native.

| Target | Command | Output |
|---|---|---|
| Linux | `Native/mpv/build-linux.sh` (rootless bubblewrap, Ubuntu 24.04 rootfs) | `runtimes/linux-x64/native/` |
| Windows | `Native/mpv/fetch-windows.sh` (LGPL zhongfly build) | `runtimes/win-x64/native/` |

The Linux build is mpv 0.41 + a playback-only FFmpeg 7.1 with zero-copy hardware decoding through mpv's OpenGL render
API: **nvdec** with CUDA interop on NVIDIA (only the user's driver is needed) and **VA-API** with EGL dma-buf interop on
Intel/AMD (Wayland, DRM and X11 displays). mpv's `hwdec=auto-safe` picks it: vulkan is skipped (no GL interop), then
nvdec, then vaapi.

The libraries are not in git. They are mirrored as the assets of a `natives-N` prerelease, pinned by `Native/natives.lock`
(tag + sha256 per OS). `Native/natives.sh fetch` downloads and checks the pinned set; `Native/natives.sh pack <rid>` packs
a local build for a new set.
