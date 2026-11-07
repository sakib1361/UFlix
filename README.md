# UFlix

A lightweight desktop media player and library for **Windows and Linux**. You can play local files, torrents (streamed while they download), YouTube, IPTV and web streams, and browse movies and shows through TMDB. Everything runs in one native app with no embedded web browser.

UFlix is built with [Uno Platform](https://platform.uno/) (WinUI XAML) and uses [mpv](https://mpv.io/) for playback, with hardware decoding on every platform.

> **Status:** in active development. Linux (X11/XWayland) is the main tested platform. The native Windows (WinAppSDK) build is still in progress.

## Features

- **Local files**: picks up subtitles next to the video or in `Subs/`, and makes thumbnails for your library
- **Torrents**: plays magnet links and `.torrent` files while they download, using a native libtorrent engine that pauses playback before the buffer runs out
- **YouTube**: plays and downloads through yt-dlp / YoutubeExplode, with a quality picker
- **IPTV and web streams**
- **Discover**: movie and TV metadata from TMDB, plus a watchlist
- **Subtitles**: searches OpenSubtitles and SubDL, and lets you adjust subtitle delay, font, size and colour
- **Low resource use**: hardware decoding (VA-API/Vulkan/D3D11), no web engine, no timers running while the app is idle

## Building

Requirements: the [.NET 10 SDK](https://dotnet.microsoft.com/download). On Linux you also need an X11 session or XWayland.

```bash
git clone <repo-url> && cd UFlix

# Linux (or any OS): the Skia/X11 desktop head
dotnet run --project UFlix.Cross/UFlix.Cross.csproj -f net10.0-desktop

# open a file directly
dotnet run --project UFlix.Cross/UFlix.Cross.csproj -f net10.0-desktop -- /path/to/video.mkv
```

On Windows, the plain .NET SDK also builds the native WinAppSDK head (no Visual Studio needed):

```powershell
dotnet build UFlix.Cross\UFlix.Cross.csproj -f net10.0-windows10.0.26100
```

### Windows installer

```powershell
UFlix.Cross\Platforms\Windows\package-windows.ps1 -Version 1.0.0   # → out\windows\UFlix-win-Setup.exe
```

The installer is built with [Velopack](https://velopack.io). It installs per-user (no admin rights), adds Start menu and desktop shortcuts and an uninstall entry, and bundles the .NET runtime, the Windows App SDK and libmpv.

### Linux package

```bash
UFlix.Cross/Platforms/Desktop/package-linux.sh   # → out/linux/UFlix.AppImage
```

The package includes the .NET runtime and its own libmpv, so it doesn't depend on the mpv version your distro ships. It targets Ubuntu 24.04+ or an equivalent distro.

### Native components

Prebuilt native libraries are committed under `runtimes/<rid>/native/`, each with a `BUILDINFO.txt` that records its exact source version and build options. You only need to rebuild them when you change them:

| Component | Script |
|---|---|
| libmpv (Linux) | `Native/mpv/build-linux.sh` builds mpv and a playback-only FFmpeg from source |
| libmpv (Windows) | `Native/mpv/fetch-windows.sh` downloads a pinned LGPL build |
| tsdl (libtorrent wrapper) | `Native/build-linux.sh`, `Native/build-windows.ps1` (see [Native/readme.md](Native/readme.md)) |

### Tests

```bash
dotnet run --project Engines/MpvSharp.Smoke -- /path/to/clip-with-audio.mkv   # player checks
dotnet run --project Engines/NativeRunner.Smoke                               # torrent engine checks
```

## Project layout

```
UFlix.Cross/        the app: pages, player, services (Uno single project, Windows + Linux heads)
Engines/
  UFlix.Engine/     models, TMDB, LiteDB storage, media providers, subtitle search
  MpvSharp/         UI-independent libmpv binding and player
  NativeRunner/     .NET binding for the native torrent library + seekable torrent stream
Native/
  TorrentSharp/     tsdl: a C API over libtorrent (C++, CMake + vcpkg)
  mpv/              scripts that produce the bundled libmpv
doc/                development reference (architecture, decisions, known issues)
```

For how the pieces fit together, see [doc/CrossPlatformDev.md](doc/CrossPlatformDev.md).

## Contributing

Issues and pull requests are welcome. Before you open a PR, make sure `dotnet build UFlix.Cross/UFlix.Cross.csproj -f net10.0-desktop` passes. If you changed the player or the torrent engine, run the matching smoke test as well.

## License

UFlix is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License, version 3 or (at your option) any later version**. See [LICENSE](LICENSE).

The whole project is GPL because the Linux build bundles a GPL build of libmpv, which is needed for X11 video output. Bundled and linked third-party components keep their own licences:

| Component | Licence |
|---|---|
| mpv / libmpv, Linux build | GPL-2.0-or-later |
| mpv / libmpv, Windows build | LGPL-2.1-or-later |
| FFmpeg, bundled on Linux | LGPL-3.0-or-later |
| libtorrent | BSD-3-Clause |
| Uno Platform | Apache-2.0 |
| NuGet dependencies (LiteDB, TMDbLib, YoutubeDLSharp, YoutubeExplode, AngleSharp, Velopack, HtmlAgilityPack, …) | their respective licences |

The source for each bundled native binary is identified in the `BUILDINFO.txt` next to it.

## Disclaimer

UFlix is a player. It does not host, index or provide any content. You are responsible for making sure that what you stream or download is legal where you live.
