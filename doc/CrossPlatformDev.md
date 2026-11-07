# UFlix — Development reference

`UFlix.Cross` is the UFlix app: one XAML code base on **Uno Platform**, **mpv** as the only player, running on **Windows and Linux**. It has no web engine, uses hardware decoding everywhere and keeps RAM and CPU low. Licence: GPL-3.0-or-later (`LICENSE`, §6).

This file is the single reference for the app: architecture, code map, native builds, how to build and test, the rules learned the hard way, decisions, and what is still open.

---

## 1. Architecture

```
Engines/
  UFlix.Engine/          net10.0   models, TMDB, LiteDB, providers, torrent/subtitle search, YouTube, subtitle finder, torrent utils  (shared by every app)
  NativeRunner/          net10.0   libtorrent P/Invoke (tsdl) + LibStream                     (shared)
  NativeRunner.Smoke/    net10.0   headless torrent test (console)
  MpvSharp/              net10.0   libmpv binding + MpvPlayer; no UI types
  MpvSharp.Smoke/        net10.0   headless player test (console)
UFlix.Cross/             Uno single project, two heads:
                           net10.0-windows10.0.26100 → WinAppSDK head = real WinUI 3 (built on Windows hosts only)
                           net10.0-desktop           → Skia head; X11 host on Linux
Native/
  TorrentSharp/          C++ tsdl (libtorrent wrapper), CMake + vcpkg; build-linux.sh / build-windows.ps1
  mpv/                   fetch-windows.sh (newest LGPL libmpv-2.dll) / build-linux.sh (GPL libmpv.so.2)
```

Layering: `UFlix.Cross` → `MpvSharp`, `NativeRunner`, `UFlix.Engine`. `MpvSharp` and `NativeRunner` know nothing about each other or about any UI.

### Why two heads

Measured on the same PC (RTX 3060 Ti, 1080p H.264, hardware decode):

| | Windows: WinAppSDK head | Linux: Skia/X11 head |
|---|---|---|
| Video path | mpv composition swapchain → `SwapChainPanel` | mpv `--wid` → `X11NativeWindow`, `gpu-context=x11vk` |
| Playback cost | 3.1 % of one core · 112 MB | 3.9 % · 220 MB |
| 25 fps UI animation | 1.8 % (Skia head on Windows: 31.5 %) | 3.0 % |
| XAML over the video | translucent, XAML owns input | **opaque, square corners** |

The Skia head also runs on Windows but costs 17× more for UI animation, so Windows uses the WinUI head.

### The player seam

```
PlayerPage (XAML) ──► MediaContainer ──► MpvSharp.MpvPlayer ──► libmpv
      │                     │
      └── VideoSurface ◄────┘  (IVideoSurface: options before mpv_initialize + Attach/Release)
```

- `Controls/VideoSurface.cs` has one class per head, split with `#if WINDOWS10_0_17763_0_OR_GREATER`:
  - **WinUI head:** a `SwapChainPanel`. mpv runs with `gpu-context=d3d11`, `d3d11-output-mode=composition` and `d3d11-composition-size=WxH`. `MpvPlayer.WatchSwapChain()` reads `display-swapchain` when watching starts, when the property notifies and on every `VIDEO_RECONFIG` (no timer), and the panel binds it through `ISwapChainPanelNative::SetSwapChain` and scales it by `1/RasterizationScale` (`IDXGISwapChain2::SetMatrixTransform`). On resize it updates `d3d11-composition-size`.
  - **Skia head:** a `ContentControl` hosting a native window (`X11NativeWindow` on Linux, `Win32NativeWindow` on Windows). mpv gets `wid=<xid>` and, on Linux, `gpu-context=x11vk`. A thread blocked on X events (`CreateNotify`) maps mpv's child window (see §5). The idle cursor is a blank X cursor on the host window, which mpv's window inherits.
- The app shell: `NavigationHelper.Initialize(MainWindow, IPlayerControl)` in `App.OnLaunched`, and every page calls `NavigationHelper.OpenPlayer(mediaItem)`. `PlayerController` creates `Player/PlayerPage`.

---

## 2. Code map

### `Engines/MpvSharp`

| File | Role |
|---|---|
| `Native/MpvNative.cs` | `LibraryImport` bindings, event/property structs, `stream_cb` struct. Library resolver: `$MPV_LIB` → `runtimes/<rid>/native/` → app folder → system copy (sets `MpvPlayer.UsesSystemLibrary`). `mpv_create()` sets `LC_NUMERIC=C` first. `Command(params string[])` builds a UTF-8 argv. |
| `MpvPlayer.cs` | One mpv instance. `Create(MpvOptions)` sets the defaults and applies `Extra` (the head's options), initializes, observes `time-pos`, `duration`, `pause`, `paused-for-cache`, `eof-reached`, `track-list`, `volume` and `sub-delay`, and starts the `mpv-events` thread. It has `LoadAsync(url, LoadOptions)` and `LoadAsync(Stream, name, options, cancel)`, which complete on `FILE_LOADED` and throw `MpvException` on a load error. Transport is `Play`, `Pause`, `TogglePause`, `Stop`, `Seek(exact)`, `Volume` (0–200), `SubtitleDelay`, `AddSubtitle`, `SelectTrack` and `Tracks`. Raw access is `GetProperty`, `SetProperty` and `Command`. `DisposeAsync` does quit → join → destroy. |
| `MpvStreamSource.cs` | `uflix://<id>` protocol via `mpv_stream_cb_add_ro`; read/seek/size/close/**cancel** callbacks (`[UnmanagedCallersOnly]`, never call libmpv, never throw). |
| `MpvTypes.cs` | `MpvOptions`, `LoadOptions` (AudioUrl, Subtitles, Start, Title), `MpvTrack`, `TrackType`, `EndReason`, `ICancellableStream`, `MpvException`. |

**Defaults set by `MpvPlayer`:** `hwdec=auto-safe`, `vo=gpu-next`, `keep-open=yes`, `idle=yes`, `config=no`, `ytdl=no` (the app runs yt-dlp itself), `osc=no`, `input-default-bindings=no`, `input-vo-keyboard=no`, `input-cursor=no`, `stop-screensaver=yes`, `demuxer-max-bytes=32MiB`, `demuxer-max-back-bytes=8MiB`, `volume-max=200`.

**Events** are raised on the `mpv-events` thread, and the UI marshals them. With `keep-open=yes`, a file that plays to the end raises `Ended(Eof)` from `eof-reached`, not from `END_FILE`.

### `UFlix.Cross`

| Path | Role |
|---|---|
| `App.xaml(.cs)` | Dark theme, maroon `SystemAccentColor*` overrides, the shared styles and converters, `PlayerBarBrush`. `PlayerController : IPlayerControl`. `UFlix.Cross <file>` opens a local file at startup. |
| `Platforms/Desktop/Program.cs` | Uno host: `.UseX11().UseWin32()`. Calls `VelopackApp.Build().Run()` first. |
| `Platforms/Desktop/package-linux.sh` | Linux package: Velopack AppImage (the only Linux package) + release feed in `out/linux/`. |
| `Platforms/Windows/app.manifest` | PerMonitorV2 for the unpackaged WinAppSDK head. |
| `Platforms/Windows/Program.cs` | WinAppSDK `Main` (the XAML-generated one is off via `DISABLE_XAML_GENERATED_MAIN`): `VelopackApp.Build().Run()`, then `Application.Start`. |
| `Platforms/Windows/package-windows.ps1` | Windows installer: Velopack `UFlix-win-Setup.exe` (the only Windows package; `--noPortable`) + release feed in `out/windows/`. |
| `MainWindow.xaml(.cs)` | `NavigationView` shell + player overlay + notification. Title-bar colours only on the WinUI head; fullscreen via `AppWindow.SetPresenter`. |
| `Pages/`, `Templates/`, `Services/`, `TorrentEngines/` | Namespace `UFlix.Cross.*`; Uno-specific patches in §3. `UFlix.Engine.Services`/`.Torrents` are global usings (`UFlix.Cross.csproj`). |
| `Services/PlayerSettings.cs` | mpv settings in the LiteDB permanent store: `SubtitleFontSize`, `SubtitleColor`, `SubtitleFont` (shown in `SettingsDialog`). |
| `Services/KeepAwake.cs` | Keeps the screen on while video plays (loaded and not paused; `MediaContainer.UpdateKeepAwake`). Windows: `SetThreadExecutionState` on the UI thread. Linux: libdbus P/Invoke on a private session-bus connection → `org.freedesktop.ScreenSaver.Inhibit`, falling back to the xdg-desktop-portal `Inhibit` (idle + suspend). mpv's `stop-screensaver` isn't enough: the WinUI head has no mpv window, and its X11 calls don't reach a Wayland compositor through XWayland. KDE applies the inhibit after ~5 s. |
| `Engines/UFlix.Engine/Services/YoutubeService.cs` | YouTube data for the page and player (YoutubeExplode): `YtItem` tiles, `YtPager` (interleaves result streams, dedups, pulls a page only when asked), search with filters, "For you" feed from the last 3 searches (history in the permanent store, `YtSearchHistory`), channel uploads, link resolving, video details/channel avatars (in-memory cache), closed captions → SRT. |
| `Pages/YoutubePage.xaml(.cs)` | Home (continue watching, recent searches, topic chips, For you feed), search with All/Videos/Playlists/Channels chips, channel view; in-page back stack; pages in from the `ScrollViewer` (Uno's `GridView` never calls `ISupportIncrementalLoading`). Card menu: play, go to channel, copy link, open in browser. |
| `Services/ThumbnailService.cs` | Local-video thumbnails: ffprobe duration → ffmpeg frame at 10 % → cached JPEG in `<data>/Thumbs`. |
| `Controls/VideoSurface.cs` | The per-head video surface (§1). |
| `Player/PlayerPage.xaml(.cs)` | Video + control bar + side panel (YouTube details: title, channel, views/date/likes, description; up next / playlist with autoplay switch) + info panel + torrent buffering panel; speed menu (all media), YouTube captions menu, theater mode (hides the side panel); keyboard accelerators. |
| `Player/MediaContainer*.cs` | Playback per `MediaType` on `MpvPlayer` (below). |
| `Player/MediaInfoPanel.xaml` | Stats panel bound to `PlayerInformation`. |
| `runtimes/win-x64/native/` | `libmpv-2.dll` + `BUILDINFO.txt` (`Native/mpv/fetch-windows.sh`). `runtimes/linux-x64/native/`: `libmpv.so.2` + bundled deps + `BUILDINFO.txt` (`Native/mpv/build-linux.sh`). Not committed: `Native/natives.sh fetch` downloads the set pinned in `Native/natives.lock` (see §6). Only the target RID's folder is copied (`NativeRid` in `UFlix.Cross.csproj`/`NativeRunner.csproj`), so a Linux package carries no Windows DLLs and vice versa. |

**`MediaContainer` by source:**

| MediaType | How it plays |
|---|---|
| Local | `LoadAsync(path)` with the subtitles (`.srt/.ass/.ssa/.vtt`) found by `UFlix.Engine/Services/SubtitleFinder`: next to the video, in a `Subs`/`Sub`/`Subtitles` folder beside it (files named after the video win in a season pack), or in a folder inside it named after the video or with the same SxxEyy (RARBG `Subs/<video>/2_English.srt`) |
| Web / Unknown (IPTV, URLs) | `LoadAsync(url)` + side-by-side `.srt`/`.en.srt` downloaded from the web |
| Torrent | `LibTorrentEngine` (handler from `TorrentManager.Open`) → `Stream` → `LoadAsync(stream, …, cancel: LibStream.Cancel)` + the **runway policy**; the selected file's subtitles (same `SubtitleFinder` rules on the torrent's file list) are downloaded first |
| Youtube / PlayList | yt-dlp or YoutubeExplode stream list; video URL + `AudioUrl` (added with `audio-add`); quality menu ("1080p60 · VP9", "· HLS" = yt-dlp m3u8) reloads at the current position; captions are downloaded as SRT and added with `sub-add` once per file (`MediaContainer.ShowCaption`) |

**Runway policy (torrents):** a thread-pool loop every 100 ms estimates the playhead's byte offset (`time-pos / duration × length`) and reads `LibStream.ContiguousBytesFrom(offset)`. It pauses mpv below max(2 MiB, 1.4 s of bitrate) and resumes at max(6 MiB, 4 s), with hysteresis. It never runs inside a stream callback. The buffering panel shows peers, rate and progress while it holds playback.

**Player page layout:** in a window, the opaque control bar sits **below** the video, so nothing overlays the picture and the controls work even if pointer events don't reach XAML over the native video window. In fullscreen the video spans the bar's row, and the opaque bar overlays the video and auto-hides after 3 s. It comes back on pointer move or any shortcut key. While it shows, subtitles are raised with `sub-margin-y`. Subtitles are rendered by mpv (libass) and placed in the letterbox.

Shortcuts: Space play/pause · ←/→ ±10 s · ↑/↓ volume · F fullscreen · T theater (side panel off) · Esc leave fullscreen, then stop · Z/X subtitle delay ±0.5 s.

### `Engines/NativeRunner` + `Native/TorrentSharp`

- `tsdl` is a self-contained C API over libtorrent (static libtorrent/Boost/OpenSSL, only `tsdl_*` exported, exceptions caught at the boundary, `tsdl_last_error`). It has an alert thread with piece-arrived events (`tsdl_poll_events`), asynchronous atomic resume data, and `tsdl_session_destroy`.
- `NativeTS` loads `tsdl` with `LibraryName = "tsdl"` through a resolver: `$TSDL_LIB` → `runtimes/<rid>/native` → app folder.
- `LibStream` computes the piece from the stream position and never returns bytes past the contiguous run. `Cancel()` wakes blocked reads, and mpv's `cancel_fn` calls it. It has `ContiguousBytesFrom`, and it raises its buffering events on the thread pool. `LibTorrentHandler` is `IAsyncDisposable`, and the session is disposed on process exit.
- `LibSession`'s 1 s status timer runs only while some handler `IsActive` (downloading and not paused, or watched by a player through `SubscribeMeta`); `AddTorrent`, `Resume` and `SubscribeMeta` wake it and it stops itself. Each tick raises `StatusUpdated(handler, status)`. `LibTorrentHandler.Pause/Resume` map to `tsdl_stop/start_torrent`.

### `Engines/UFlix.Engine` (cross-platform patch)

- `Services/AppPaths.cs`: `DataFolder` (Windows `Documents\<AppName>`, unchanged; Linux `$XDG_DATA_HOME/<AppName>`) and `DownloadFolder` (Windows unchanged; Linux `XDG_DOWNLOAD_DIR/UFlix` from `user-dirs.dirs`). `DBUtilities` and `Utilites` use it. `<AppName>` is `AppConstants.AppName` (`UFlix` by default), read on every access.
- `Services/FFmpegUtils.cs`: on Linux and macOS it links the system `ffmpeg`/`ffprobe` into the tools folder when present. Otherwise it downloads them. It marks downloaded binaries executable. `UFlix.Cross`'s `Downloader` uses it.

**Data locations (Linux):** DB `~/.local/share/UFlix/LocalData.liteDB` · yt-dlp/ffmpeg `~/.local/share/UFlix/YT/` · thumbnails `~/.local/share/UFlix/Thumbs/` · torrents `~/Downloads/UFlix/Torrents/` (or the Settings path).

---

## 3. Uno patches

Where the code differs from what plain WinUI 3 would accept or need:

| Where | Change |
|---|---|
| `SettingsDialog.xaml` | Three `Pivot` tabs: Player (subtitles), Downloads (folders, size cap, YTDL first), Torrent Search. Uno rejects `{x:Bind}` to static types → bound to instance properties `Settings`, `Player`, `SubtitleProviders`. No hwdec switch: mpv's `auto-safe` default is always used. |
| `Services/TorrentManager.cs` + `Pages/TorrentsDialog` | The app-wide torrent session, created on the thread pool on the main window's first activation, right after the size cleaner (`Utilites.ClearFolder`) runs. Each torrent has a `TrackedTorrent` record (`Url`, `Title`, `Kind` Play/Download, `Progress`, `SavePath`) in `DBUtilities.TrackedTorrents`, saved on add/remove/kind change and each whole-percent step. At start, completed records are dropped and the rest re-added. A play request reuses the session's torrent; closing the player removes a **Play** torrent that isn't complete (files stay on disk for the size cleaner). The download icon on a torrent row in the movie/show dialogs adds the whole torrent as **Download** (or promotes a playing one). The "Torrents" sidebar item has two tabs: **Downloading** lists them with pause/resume and remove; **Search** sends a typed movie name to the chosen provider (as a bare `Movie { Title }`, falling back to the TMDB movie when that finds nothing, since the Stremio providers need its IMDb id). Both Play and Download there add the torrent as **Download** first, so closing the player never stops it. |
| Pages (7×) | `this.Content.XamlRoot` → `this.XamlRoot` (`Page.Content` is `object` in Uno). |
| `HorizontalView.FindDescendant<T>` | `return null` → `return default`; the delete button's `OpacityTransition` removed. |
| `HorizontalView.SourceProvider` | was registered as `typeof(string)` while holding an `IMediaProvider`: WinUI tolerates it, Uno doesn't (the Home rows stayed empty). Now `typeof(IMediaProvider)`. The inner `ScrollViewer` is looked up on first use, because Uno sets the property before the ListView template exists. |
| `{x:Bind}` with an empty path (HomePage row, SubtitleDialog button `Tag`) | → `{Binding}`; the empty-path form isn't reliable in Uno's generator. |
| `LoadingCollection` | local `IIncrementalSource<T>` instead of the CommunityToolkit package. |
| `MainWindow` | no `DisplayRequest` (replaced by `Services/KeepAwake`); title-bar colours only under `#if WINDOWS…`. |
| `OpenFileDialog` | `InitializeWithWindow` only on the WinUI head; more extensions. |
| `CustomImage` | `StorageFile.GetThumbnailAsync` → `ThumbnailService`. Loads through `BitmapImage.UriSource` (a `file://` into the `ImageCache` disk cache) with `DecodeWidth` → `DecodePixelWidth` (logical px) set per template; collapsed images are not loaded; stale loads on recycled containers are dropped (see §5 Memory). |
| `MediaConverter`, `IWindowService` | dropped (dead code). |
| All `AcrylicBrush` in the player | replaced by the opaque `PlayerBarBrush`. |

---

## 4. Build, run, test

```bash
# Linux (or any OS): the Skia head
dotnet build UFlix.Cross/UFlix.Cross.csproj -f net10.0-desktop
dotnet run  --project UFlix.Cross/UFlix.Cross.csproj -f net10.0-desktop [-- /path/video.mkv]

# Windows: both heads (the WinAppSDK TFM is added automatically on Windows hosts)
dotnet build UFlix.Cross/UFlix.Cross.csproj

# Packages are built with Velopack (vpk pinned in dotnet-tools.json; `dotnet tool restore` runs it). Version: $VERSION / -Version,
# else the VERSION file at the repo root (SemVer; Directory.Build.props gives it to every project). Besides the installer, out/<os>/
# holds the Velopack release feed (UFlix-<version>[-linux]-full.nupkg, releases.<channel>.json) that release.yml uploads.

# Linux: out/linux/UFlix.AppImage (self-contained .NET, UFlix icon, bundled libmpv; fails if libmpv is missing)
UFlix.Cross/Platforms/Desktop/package-linux.sh

# Windows: out/windows/UFlix-win-Setup.exe (per-user, no admin, Start menu + desktop shortcuts, uninstall entry)
# (unpackaged, self-contained .NET + WinAppSDK, bundled libmpv + tsdl; fails if either or resources.pri is missing)
UFlix.Cross\Platforms\Windows\package-windows.ps1 [-Version x.y.z]

# headless tests
dotnet run --project Engines/MpvSharp.Smoke -- /path/clip-with-audio.mkv
dotnet run --project Engines/NativeRunner.Smoke -- [download folder]
```

- **API keys:** TMDB, OpenSubtitles and SubDL keys live in the git-ignored `secrets.env` at the repo root (template: `secrets.env.example`). `UFlix.Engine.csproj` embeds it into `UFlix.Engine.dll`, so every build and package carries them; `Services/AppSecrets.cs` reads it, and an environment variable of the same name overrides a value. Without the file the build warns and TMDB/subtitles stop working.
- `MpvSharp.Smoke` checks load, playback, duration, tracks, title, pause, exact seek, volume > 100, sub-delay, EOF → `Ended(Eof)`, a missing file failing the load, `stream_cb` playback, **stop while a read is blocked** (`cancel_fn`), and dispose. It prints PASS/FAIL per check and exits with 1 on any failure.
- A test clip: `ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=24 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 10 -c:v libx264 -c:a aac -shortest test.mkv`.
- **Memory probe:** `UFLIX_MEMPROBE=/tmp/mem.log dotnet run …` drives the app by itself (Home loads, scrolls to the end, cycles Discover/YouTube/TV/Home `UFLIX_MEMPROBE_ROUNDS` times, exits) and logs, after a forced GC, managed MB, RSS before/after `malloc_trim`, live pages and images (weak references) and the Skia GPU cache. `UFLIX_NO_IMAGES=1` gives the baseline, `UFLIX_SOFTWARE_RENDER=1` forces Uno's X11 software renderer, `UFLIX_MEMPROBE_HOLD=1` keeps it open 60 s for `dotnet-gcdump`/`dotnet-dump`. Code: `Services/MemoryProbe.cs`.
- Diagnostics: `UFLIX_MPV_LOG=/tmp/mpv.log` writes mpv's verbose log (the surface and options are the first line). `MPV_LIB=/path/libmpv.so.2` overrides the library. `TSDL_LIB` overrides tsdl.
- Native: `Native/build-linux.sh` (run inside `ubuntu:24.04` for a release build, as CI does), `Native/build-windows.ps1`, `Native/mpv/fetch-windows.sh`, `Native/mpv/build-linux.sh`.

---

## 5. Rules learned the hard way

**libmpv**
- `LC_NUMERIC` must be `"C"` before `mpv_create`, or it returns NULL on non-C locales (`MpvNative.mpv_create` does it).
- Shutdown is `quit` → join the event thread → `mpv_terminate_destroy`. Destroy first and it can hang or crash with a live VO or stream callback. Dispose the torrent stream **after** mpv is destroyed.
- Never call libmpv from a `stream_cb` callback or from mpv's render thread: it deadlocks. Pause and resume for torrents from a separate thread.
- Always give `stream_cb` a `cancel_fn`. Without it, stop while a read is blocked hangs for about 6.5 s. With it, stop returns in about 100 ms (measured).
- `stream_cb` is documented "not stable yet". The fallback is a localhost HTTP range server (it works, with a known handler leak).
- Per-file `loadfile` options use `%len%value` quoting, because URLs and titles contain commas. External audio and subtitles are added with `audio-add`/`sub-add` after `FILE_LOADED` (path-list options split on `:` on Linux).
- `keep-open=yes` ⇒ EOF comes from `eof-reached`.

**No idle polling**
- Nothing may tick while the user is idle. The timeline follows mpv's `time-pos` events (`MediaContainer.PositionTick`, once per whole second); auto-hide of the bar and cursor is a one-shot 3 s timer restarted on pointer activity; the X11 and swapchain handling are event-driven. Loops that only run while something is active are acceptable: the torrent runway check during torrent playback, and the info panel's 1 s refresh while it is open or a torrent is loading.

**Uno vs WinUI XAML**
- Register every dependency property with its real type. WinUI lets a `typeof(string)` DP hold any object; Uno doesn't.
- Use `{Binding}` rather than an empty `{x:Bind}` inside `DataTemplate`s, and never `{x:Bind}` to static types.
- Don't walk a control's template (`VisualTreeHelper.GetChild`) from a property-changed callback: on Uno it can run before the template is applied.
- Don't refill a `Pivot` built from `ItemsSource`: on Uno the old tabs' content stays drawn under the new tabs (overlapping text) and the headers vanish, whether the collection is cleared or the `ItemsSource` is replaced. The movie/show dialogs list torrents with `Templates/TorrentQualityView` (a horizontal quality `ListView` + one torrent `ListView`) instead.

**Memory (images)** — measured with the memory probe, Linux Skia/X11, 2026-09-29

| | images off | old `CustomImage` (stream `SetSourceAsync`, full size) | now (`UriSource` + `DecodePixelWidth`) |
|---|---|---|---|
| Home loaded | 276 MB | 311 MB | 301 MB |
| Home scrolled to the end (~775 images) | 375 MB | 688 MB | 508 MB |
| each page cycle afterwards | ±0 | +25–40 MB, never returned | ±0 |
| parked after 5 cycles | 380 MB | ~660 MB | 458 MB |

- Pages are released (only the current page is alive after a GC). What grew was image memory: `SetSourceAsync(stream)` bypasses Uno's decoded-image LRU (100 entries keyed by uri + decode size), so every visit decoded the same posters again at full size, and the native memory isn't given back even after the `SKImage`s are finalized (same with the software renderer, so not GPU-specific; `malloc_trim` barely helps). With `UriSource` the shared posters hit the LRU.
- Skia's GPU resource cache (256 MB budget, not configured by Uno) keeps textures of dead images; purging it frees the cache but not RSS, so it isn't worth hacking.
- A local in an `async` method that holds a UI element (e.g. a `ScrollViewer`) keeps the whole page alive for as long as the method is suspended: the probe did this once and looked like a page leak.

**Linux / X11**
- **mpv 0.41 leaves its `--wid` child window unmapped** inside Uno's X11 host, so the video area stays black while mpv reports 0 drops. This also broke the session-2 spike on this machine. `VideoSurface` maps each child mpv creates, from a thread blocked in `XNextEvent` on its own display connection; it exits on the host window's `DestroyNotify`.
- Use `gpu-context=x11vk`: `x11egl` fails next to Uno's GLX renderer on NVIDIA. On a GPU without Vulkan, fall back to Uno's software renderer + `x11egl` (4.4 %).
- XAML over the native video window is opaque with square corners, so design every overlay to look right opaque.
- **Exit segfaults on NVIDIA** (even with no playback): `libGLX_nvidia`'s `atexit` handler crashes on the GLX context Uno's X11 host leaves behind. `Program.Main` registers a last `ProcessExit` handler on Linux that calls `_exit`, so the managed handlers (torrent session) still run but the native `atexit` handlers don't.
- `XGetImage` captures of the window don't show mpv's Vulkan output. Check the picture with a compositor screenshot of the raised window.
- Pointer input over the video works on Linux (checked by hand on KDE/XWayland): moving the mouse shows the bar and cursor, and double-click toggles fullscreen. Double-click is handled on the page's root grid, not on `VideoSurface`, because Uno may not attribute clicks over the native window to that element. If a desktop ever doesn't deliver them, mpv's `keybind` → `script-message` is the fallback (not implemented).

**Windows**
- The composition swapchain needs libmpv ≥ 0.41. Rebind whenever `display-swapchain` changes (mpv re-creates it with the VO). Scale by `1/RasterizationScale`.

**Torrents**
- `LibTorrent` is the only engine (BitSwarm playback was removed as too slow). The BitSwarm package is gone: its file-sorting and size-formatting helpers live in `Engines/UFlix.Engine/Torrents/TorrentUtils.cs`.
- A blocked read is not starvation: a read-ahead player's reader waits at the download frontier while the playhead still has seconds of runway. Trigger only on runway past the playhead, with a low/high hysteresis gap (a shared threshold flapped 23–147 times in 40 s).

---

## 6. Decisions (2026-09-28)

- `UFlix.Engine` portability patch approved (`AppPaths`, OS-aware ffmpeg). Windows behaviour unchanged.
- Packages approved: `Uno.Sdk` (pinned 6.7.30; Skia/X11 via the SDK), `YoutubeDLSharp`, `Velopack` (MIT; installer + future updates, `vpk` CLI pinned in `dotnet-tools.json`). CommunityToolkit was replaced by a local shim. mpv and libtorrent are P/Invoke only.
- **mpv is always bundled**, never the system copy in a release: binaries under `runtimes/<rid>/native/` with a `BUILDINFO.txt`, as for `tsdl`. No native binary is committed. The native libraries (libmpv, tsdl) are not in git. They are mirrored as the assets of a `natives-N` prerelease in this repo, pinned by `Native/natives.lock` (tag + sha256 per OS). `Native/natives.sh fetch` downloads and checks the pinned set (CI does this on every build); the **Natives** workflow (`.github/workflows/natives.yml`, run by hand) builds Linux libmpv and tsdl from source, fetches the newest Windows libmpv, publishes a new set and commits the new pin. Why a mirror: committing them grows every clone by ~157 MB per update (libmpv-2.dll alone is 100.4 MB, at GitHub's 100 MiB file limit), zhongfly/mpv-winbuild deletes builds after about a month (a sha256 pin to it 404'd within six weeks), and building everything in each CI run took 30–60 min.
- **Licensing: LGPL unless a GPL-only feature is used.**
  - **Windows:** LGPL libmpv (zhongfly `mpv-dev-lgpl-x86_64`), dynamically linked, replaceable, shipped with the licence and a source link.
  - **Linux:** X11 video output (`--wid`, `x11vk`) is GPL-only in mpv (`-Dgpl=true`), so a distributed Linux build is a combined work under **GPLv2+**. The repository is therefore licensed **GPL-3.0-or-later** (`LICENSE`); every dependency and bundled binary must stay GPLv3-compatible.
  - **Escape route** if Linux must be LGPL: render through the render API into Uno's `GLCanvasElement` (9.4 % vs 3.9 % of a core on NVIDIA; iGPUs only get `vaapi-copy`).
- Minimum Linux target: **Ubuntu 24.04** (glibc 2.39; also Debian 13, Fedora 40+ era). libmpv is built in an Ubuntu 24.04 rootfs and release `libtsdl.so` in `ubuntu:24.04` (CI). Ubuntu 22.04 was dropped (2026-11-07): the bundled libmpv never ran there, so a 22.04 tsdl floor bought nothing.
- Packaging: Velopack on both OSes (Windows `Setup.exe`, Linux AppImage). Updates: bumping `VERSION` on master/main runs `.github/workflows/release.yml` (builds both OSes through `build-desktop.yml`, then `gh release create vX.Y.Z` with the installers + Velopack feed files; an existing release is skipped). CI bakes the repo URL into the app (`UFlixUpdateRepo` → `AssemblyMetadata`), and `Services/AppUpdater.cs` checks that repo's releases with Velopack's `GithubSource`: once a day through a one-shot timer re-armed after each check (first check 30 s after start), or from Settings → Updates (Check now / Install and restart, daily toggle). Only a Velopack-installed copy (Setup.exe install, the AppImage) updates itself; local builds and unpacked folders show the releases link instead. The Linux tarball was dropped (2026-11-07): it had no Velopack update helper, so the AppImage is the only Linux package. Installs made before the updater shipped have to be updated by hand once. `EnableMsixTooling=true` stays on even unpackaged: it is what generates `resources.pri` without Visual Studio (without it the app dies at startup with `0xC000027B` in `Microsoft.UI.Xaml.dll`).

---

## 7. Status and open work

| Phase | State |
|---|---|
| 0 Scaffold | **Done.** `UFlix.Cross/`, two TFMs (WinAppSDK TFM only on Windows hosts), in `UFlix.sln`. The desktop head builds with 0 errors/0 warnings on Linux. CI: `.github/workflows/build-desktop.yml` (composite setup in `.github/actions/setup-uflix`) runs both package scripts on push/PR/`v*` tags and uploads `out/linux` + `out/windows` as artifacts; API keys come from the `UFLIX_*` repo secrets. |
| 1a Bundled libmpv | **Windows done:** `Native/mpv/fetch-windows.sh` → `libmpv-2.dll` LGPL (newest zhongfly build when the Natives workflow runs; v0.42.0 on 2026-11-07), mirrored in the natives release. **Linux done:** upstream ships no Linux libmpv binaries, so `Native/mpv/build-linux.sh` builds mpv v0.41.0 (GPL, X11/Vulkan/EGL/VA-API, Wayland off) + a playback-only shared FFmpeg n7.1.2 inside an Ubuntu 24.04 rootfs run rootless with `bwrap` (no root/podman; `chown` is stubbed because only uid 0 is mapped). Target is **Ubuntu 24.04+** (22.04 is EOL). It walks libmpv's `DT_NEEDED` and bundles every library whose package is not on both the stock 24.04 and 26.04 desktop manifests (FFmpeg, libplacebo, libass, dav1d, libva, libunibreak, ...), all with `RUNPATH=$ORIGIN`. |
| 1 MpvSharp | **Done.** `MpvSharp.Smoke` passes all 15 checks on Linux (libmpv 0.41, client API 2.5). **Open:** the Windows run; porting the full mpvstream S1–S9 scenario table. |
| 2 VideoSurface + player page | **Linux: plays** (1280×720 H.264, `hwdec=vulkan`, bar below the video). **Open:** the Windows head at runtime; HEVC10 and drop/CPU/RAM numbers against §1; clean-exit check with a person driving it. Linux mouse over the video (move, double-click) confirmed by hand. |
| 3 Port the UI | **Done (compile + shell renders on Linux).** **Open:** walk every page and dialog on both heads; Home rows load (fixed: `SourceProvider` type + empty `{x:Bind}`); the file picker on Linux (GTK/portal). |
| 4 Sources | **Implemented** for Local, Web, Torrent (stream_cb + runway), YouTube (video + external audio). **Open:** runtime check of each against real sources on both OSes. |
| 5 Platform services | **Done:** XDG paths, system ffmpeg, yt-dlp chmod, mpv settings in Settings. **Open:** fresh-user Linux test without distro mpv (needs 1a). |
| 6 Polish + performance | Not started (visual pass, ReadyToRun/NativeAOT, matrix re-run, iGPU check). |
| 7 Packaging | **Velopack.** **Windows:** `package-windows.ps1` makes `Setup.exe` (portable zip dropped 2026-11-07); silent install → app starts → uninstall cleans up, checked on the dev machine (not yet on a clean one). **Linux:** `package-linux.sh` makes the Velopack AppImage; the vpk step was checked by cross-packing from Windows, the script itself not yet run on Linux. Update feed: GitHub Releases via `release.yml` + `AppUpdater` (not yet tried end to end). **Open:** Flatpak with mpv and tsdl modules. |

Native torrent plan: `win-x64` tsdl is fixed — `build-windows.ps1` has been run (vcpkg at `E:\Project\vcpkg`), so `Engines/NativeRunner/runtimes/win-x64/native/tsdl.dll` is now the static-deps build exporting only `tsdl_*`, matching `NativeTS.cs`; Windows torrent playback works. CI now builds `libtsdl.so` in `ubuntu:24.04` and `tsdl.dll` on `windows-2025` (nothing native is committed); the Natives workflow's Windows job (libmpv fetch + tsdl build) passed on its first run, 2026-11-07, in 13 min. Still open: building the Flatpak module.

Risks: Linux only measured on NVIDIA/KDE/XWayland; `stream_cb` stability; Uno 7 churn (versions pinned).
