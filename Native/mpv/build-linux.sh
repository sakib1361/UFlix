#!/usr/bin/env bash
# Builds the bundled Linux libmpv into runtimes/linux-x64/native/ (the Linux counterpart of fetch-windows.sh:
# upstream mpv ships no Linux binaries). Everything is built inside an Ubuntu 24.04 rootfs run rootless with bubblewrap, so the
# result runs on Ubuntu 24.04+ (glibc 2.39). Needs bwrap, curl, tar; no root, podman or docker.
#
# Zero-copy hardware decoding through mpv's OpenGL render API: nvdec (CUDA interop) on NVIDIA, VA-API (EGL dma-buf,
# Wayland/DRM/X11 displays) on Intel/AMD. libmpv 0.41 (GPL: X11 --wid output needs -Dgpl=true) + a playback-only shared FFmpeg 7.1. Every library in the dependency
# closure is bundled next to libmpv.so.2 (RUNPATH=$ORIGIN) unless its package is on BOTH the stock Ubuntu 24.04 and 26.04
# desktop images (X11, Vulkan loader, EGL, audio, fontconfig/freetype/harfbuzz, OpenSSL, ...), which are left to the system.
set -euo pipefail

UBUNTU_BASE="https://cdimage.ubuntu.com/ubuntu-base/releases/24.04/release/ubuntu-base-24.04.3-base-amd64.tar.gz"
MANIFESTS=(https://releases.ubuntu.com/24.04/ubuntu-24.04.3-desktop-amd64.manifest
           https://releases.ubuntu.com/26.04/ubuntu-26.04-desktop-amd64.manifest)
MPV_TAG="v0.41.0"
FFMPEG_TAG="n7.1.2"
VKHEADERS_TAG="v1.3.290"   # FFmpeg 7.1 Vulkan hwdec needs >= 1.3.277 headers; 24.04 has 1.3.275 (headers only, the loader is the system's)

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/runtimes/linux-x64/native"
WORK="${WORK:-$ROOT/out/mpv-linux}"
RFS="$WORK/rootfs"

run() {
    bwrap --bind "$RFS" / --dev /dev --proc /proc --tmpfs /tmp --unshare-user --uid 0 --gid 0 --share-net \
          --setenv LC_ALL C --setenv DEBIAN_FRONTEND noninteractive --setenv HOME /root \
          --setenv PATH /usr/sbin:/usr/bin:/sbin:/bin /bin/bash -euo pipefail -c "$1"
}

if [ ! -e "$RFS/.ready" ]; then
    rm -rf "$RFS"; mkdir -p "$RFS"
    curl -fsSL "$UBUNTU_BASE" | tar -C "$RFS" -xz
    rm -f "$RFS/etc/resolv.conf"; grep -v '^#' /etc/resolv.conf > "$RFS/etc/resolv.conf"
    echo 'APT::Sandbox::User "root";' > "$RFS/etc/apt/apt.conf.d/99sandbox"
    # only uid 0 is mapped, so maintainer scripts' chown/chgrp to other users must not fail
    run 'for c in chown chgrp; do dpkg-divert --quiet --local --rename --add /usr/bin/$c
             printf "#!/bin/sh\n/usr/bin/$c.distrib \"\$@\" 2>/dev/null; exit 0\n" > /usr/bin/$c; chmod +x /usr/bin/$c; done
         apt-get update -q
         apt-get install -y -q --no-install-recommends build-essential meson ninja-build pkg-config git ca-certificates \
             patchelf nasm python3 libdav1d-dev libva-dev libvulkan-dev libssl-dev zlib1g-dev libbz2-dev \
             libass-dev libplacebo-dev liblcms2-dev libx11-dev libxext-dev libxrandr-dev libxpresent-dev libxss-dev \
             libxinerama-dev libxfixes-dev libegl-dev libgl-dev libasound2-dev libpulse-dev libpipewire-0.3-dev \
             libdrm-dev libwayland-dev wayland-protocols libxkbcommon-dev libdisplay-info-dev'
    touch "$RFS/.ready"
fi

run "cd /root
     [ -d vkh ] || git clone -q --depth 1 -b $VKHEADERS_TAG https://github.com/KhronosGroup/Vulkan-Headers.git vkh
     [ -d ffmpeg ] || git clone -q --depth 1 -b $FFMPEG_TAG https://git.ffmpeg.org/ffmpeg.git ffmpeg
     [ -d mpv ] || git clone -q --depth 1 -b $MPV_TAG https://github.com/mpv-player/mpv.git mpv

     # NVIDIA codec headers (MIT, compile-time only): FFmpeg loads libcuda/libnvcuvid from the user's driver at runtime
     [ -d nvh ] || git clone -q --depth 1 -b n12.2.72.0 https://github.com/FFmpeg/nv-codec-headers.git nvh
     make -C nvh install PREFIX=/usr >/dev/null

     cd /root/ffmpeg
     ./configure --prefix=/opt/ff --extra-cflags=-I/root/vkh/include --enable-shared --disable-static --enable-pic \
         --disable-programs --disable-doc --disable-debug --enable-version3 --enable-openssl --enable-libdav1d \
         --enable-vaapi --enable-vulkan --disable-vdpau --disable-cuda-llvm \
         --disable-encoders --enable-encoder=png,mjpeg,ac3,eac3,aac --disable-muxers --enable-muxer=spdif,matroska,mp4,image2 \
         --disable-indevs --disable-outdevs --disable-xlib --disable-libxcb --disable-sdl2 --disable-alsa --disable-sndio \
         --disable-iconv --disable-lzma --disable-libdrm >/dev/null
     make -j\$(nproc) >/dev/null && make install >/dev/null

     cd /root/mpv && rm -rf build
     PKG_CONFIG_PATH=/opt/ff/lib/pkgconfig meson setup build --buildtype=release -Dc_args=-I/root/vkh/include \
         -Dlibmpv=true -Dcplayer=false -Dgpl=true -Dx11=enabled -Dwayland=enabled -Dvulkan=enabled -Degl-x11=enabled \
         -Dvaapi=enabled -Dvaapi-x11=enabled -Dvaapi-drm=enabled -Dvaapi-wayland=enabled -Ddrm=enabled -Dgbm=disabled \
         -Dcuda-hwaccel=enabled -Dcuda-interop=enabled -Dvdpau=disabled -Dxv=disabled -Dpulse=enabled -Dpipewire=enabled -Dalsa=enabled -Djack=disabled -Dopenal=disabled \
         -Dsndio=disabled -Dsdl2-audio=disabled -Dsdl2-video=disabled -Dsdl2-gamepad=disabled -Dlua=disabled \
         -Djavascript=disabled -Dlibarchive=disabled -Dcdda=disabled -Ddvdnav=disabled -Dlibbluray=disabled -Ddvbin=disabled \
         -Dcaca=disabled -Dsixel=disabled -Djpeg=disabled -Dzimg=disabled -Duchardet=disabled -Drubberband=disabled \
         -Dvapoursynth=disabled -Dlcms2=enabled -Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled >/dev/null
     ninja -C build >/dev/null"

# packages every stock 24.04 and 26.04 desktop has
for m in "${MANIFESTS[@]}"; do curl -fsSL "$m" | awk '{sub(/:amd64$/, "", $1); print $1}' | LC_ALL=C sort -u > "$WORK/$(basename "$m")"; done
LC_ALL=C comm -12 "$WORK/$(basename "${MANIFESTS[0]}")" "$WORK/$(basename "${MANIFESTS[1]}")" > "$RFS/tmp.system-packages"

# walk DT_NEEDED from libmpv, stopping at system libraries
run 'rm -rf /stage; mkdir /stage
     declare -A seen; queue=(/root/mpv/build/libmpv.so.2)
     while [ ${#queue[@]} -gt 0 ]; do
         lib=${queue[0]}; queue=("${queue[@]:1}")
         name=$(basename "$lib"); [ -n "${seen[$name]:-}" ] && continue; seen[$name]=1
         cp -L "$lib" "/stage/$name"
         for dep in $(patchelf --print-needed "$lib" | grep -v "^ld-linux"); do
             path=$(ldd /root/mpv/build/libmpv.so.2 | awk -v d="$dep" "\$1 == d {print \$3}")
             [ -n "$path" ] || { echo "unresolved $dep"; exit 1; }
             case $path in /opt/ff/*) queue+=("$path"); continue;; esac
             pkg=$(dpkg -S "$(readlink -f "$path")" | head -n1 | cut -d: -f1)
             grep -qx "$pkg" /tmp.system-packages || queue+=("$path")
         done
     done
     cd /stage && for f in *; do patchelf --set-rpath "\$ORIGIN" "$f"; strip --strip-unneeded "$f"; done'

rm -rf "$DEST"; mkdir -p "$DEST"
cp "$RFS"/stage/* "$DEST/"
GLIBC_FLOOR=$(run 'objdump -T /stage/* | grep -o "GLIBC_[0-9.]*" | sort -uV | tail -n1')
{
    echo "libmpv.so.2  (Linux x86_64, GPLv2+)"
    echo "built:     $(date -u +%Y-%m-%d) by Native/mpv/build-linux.sh"
    echo "base:      $(basename "$UBUNTU_BASE") (runs on Ubuntu 24.04+, glibc floor ${GLIBC_FLOOR#GLIBC_})"
    echo "mpv:       $MPV_TAG $(git -C "$RFS/root/mpv" rev-parse HEAD) (-Dgpl=true: X11 --wid output is GPL-only)"
    echo "ffmpeg:    $FFMPEG_TAG $(git -C "$RFS/root/ffmpeg" rev-parse HEAD) (shared, playback-only, --enable-version3)"
    echo "system:    everything else libmpv needs (X11, Vulkan, EGL/GL, ALSA/Pulse/PipeWire, fontconfig, OpenSSL, ...)"
    echo "sources:   https://github.com/mpv-player/mpv  https://git.ffmpeg.org/ffmpeg.git  Ubuntu 24.04 source packages"
    echo
    (cd "$DEST" && for f in *.so*; do printf '%-26s %s\n' "$f" "$(sha256sum "$f" | cut -d' ' -f1)"; done)
} > "$DEST/BUILDINFO.txt"
du -sh "$DEST"; ls "$DEST"
