# UFlix — Development reference

UFlix is a Rust desktop app: a [Slint](https://slint.dev/) UI over **mpv** (the only player), for **Windows and Linux**.
It has no web engine, uses zero-copy hardware decoding and keeps RAM and CPU low. Licence: GPL-3.0-or-later.

This file is the single reference for the app: architecture, code map, the rules learned the hard way, decisions and
open work. Keep it current instead of adding new documents.

---

## 1. Architecture

```
app/                 the UI process (Slint + controllers + player + services)
crates/engine/       UI-free core (async on tokio): models, SQLite store, settings, TMDB, catalogues, torrent search,
                     subtitles, YouTube (rustypipe + yt-dlp), IPTV, tools (yt-dlp/ffmpeg), thumbnails, image cache
crates/mpv/          libmpv binding, loaded at runtime from the bundled copy: Player, RenderContext (OpenGL), stream_cb
crates/torrent/      torrent engine on librqbit (pure Rust): Engine, Torrent, BlockingStream for the player
Native/mpv/          build/fetch scripts for the bundled libmpv;  packaging/  AppImage + Inno Setup
```

Layering: `app` → `engine`, `mpv`, `torrent`. The three crates know nothing about Slint or each other.

### The video path

mpv renders through its **OpenGL render API** into the window's own frame:

1. Slint (femtovg renderer, OpenGL) clears the frame with the window background.
2. In the `BeforeRendering` notifier, `mpv_render_context_render` draws the video into the bound framebuffer.
3. Slint draws the UI on top — the player overlay is translucent (gradients, panels, tooltips over the picture).

mpv's update callback asks Slint for a redraw, so frames are driven by mpv, not by a timer. With a side panel the video is
letterboxed into the left area with `video-margin-ratio-right`.

**Hardware decoding** (`hwdec=auto-safe`): Vulkan decoding can't be shared with OpenGL, so mpv skips it and takes
**nvdec** (CUDA–GL interop) on NVIDIA and **VA-API** (EGL dma-buf) on Intel/AMD — both zero-copy. This needs the
bundled libmpv built with CUDA interop and VA-API for Wayland/DRM (`Native/mpv/build-linux.sh`). Windows uses the LGPL
zhongfly build (d3d11va with GL interop / copy).

Measured on the dev PC (RTX 3060 Ti, 1080p HEVC, release build): 3.9–4.9 % of one core, GPU 7–9 %, ~185 MB own RAM
for the whole app while playing (the C# Uno app: 4.9–5.8 %, 23–31 %, 250–260 MB).

### Threading

- UI code runs on Slint's event loop; network and disk work runs on a 4-worker tokio runtime. UI code awaits it with
  `runtime::bg(fut)` inside `runtime::spawn_ui(async { … })`; other threads post to the UI with `runtime::ui(f)`.
- Each page/dialog controller keeps its state in a `thread_local` on the UI thread and talks to its Slint `global`.
- mpv events arrive on the "mpv-events" thread and are forwarded to the UI (position thinned to 4/s).
- glibc arenas are capped at 2 (`mallopt`), ~20 MB less RSS.

---

## 2. Code map (`app/src`)

| Path | Role |
|---|---|
| `main.rs` | start-up: runtime, OpenGL backend, controllers, navigation, command-line file, maximized window |
| `runtime.rs` | tokio runtime, `bg`, `spawn_ui`, `ui`, `sleep` |
| `ui.rs` | generated Slint code, the main window, `toast`, `set_busy` |
| `cards.rs`, `images.rs` | card lists with images loaded in the background (disk cache → decode at display width → 200-entry LRU) |
| `open.rs` | library titles open their dialog; everything else the player |
| `pages/` | `home` (rows, TMDB search), `discover`, `youtube`, `tv`, `torrents` |
| `dialogs/` | `title` (movie/show + sources), `settings`, `open_dialog`, `download` (yt-dlp), `subtitles` |
| `player/video.rs` | the single mpv instance, render notifier, `ready()` (load only once the render context exists) |
| `player/session.rs` | the playing item and its queue (playlist / Up next / torrent files), sources, events, menus, info, advanced controls, watchlist |
| `services/` | `torrents` (session, Play/Download tracking), `keep_awake` (D-Bus / SetThreadExecutionState), `updater` |
| `tour.rs` | `UFLIX_TOUR` screenshots |
| `ui/` | Slint: `theme`, `icons` (Material SVGs), `widgets`, `pages/`, `dialogs/`, `player.slint`, `main.slint` |

### Sources in the player

| MediaType | How it plays |
|---|---|
| Local | the file + subtitles found by `engine::subtitles::finder` (beside it, `Subs/`, RARBG `Subs/<video>/`) |
| Web | the URL + side-by-side `.srt` / `.en.srt` downloaded to the temp folder |
| Unknown (IPTV) | the stream, shown as LIVE, never saved to the watchlist |
| Torrent | librqbit: the file (episode match for shows, else the largest) + its subtitle files, played through `stream_cb` |
| Youtube | rustypipe or yt-dlp first (Settings), the other as fallback; video URL + `audio-add`; quality menu; captions in the subtitles menu |
| PlayList | the playlist's videos as the queue, autoplay |

**Torrent buffering:** librqbit's stream waits for and prioritises the pieces at the read position; mpv's cache
(`cache-pause-wait=4`) pauses when it runs dry and resumes with 4 s buffered. `BlockingStream::cancel` (mpv's
`cancel_fn`) wakes a blocked read on stop. Only the playing file (and its subtitles) is selected, so it gets all the
bandwidth (librqbit has no settable file priority); once it is complete the player's stats tick selects the whole
torrent so the other episodes download, and playing another file narrows the selection to it again.

**Torrent lifetime:** a torrent opened to play is tracked as Play and removed from the session when the player closes
unless it finished (files stay for the size cleaner); Download torrents stay (and seed) until complete and come back at
the next start. The session listens for incoming peers (TCP, UPnP) and starts only when something needs it.

---

## 3. Rules learned the hard way

**libmpv**
- `LC_NUMERIC` must be `"C"` before `mpv_create` (`ffi::set_c_numeric_locale`).
- Load a file only after the render context exists — with `vo=libmpv` and no context mpv disables video for that file
  (audio plays, picture stays black). `player::video::ready()` waits for it.
- Free the render context on the GL thread before destroying mpv; destroy is quit → join the event thread →
  `mpv_terminate_destroy`. One mpv instance lives for the whole run (stopped, not destroyed, between videos).
  At exit `main` runs the event loop itself (`window.run()` hides the window first, and on Wayland hiding frees GL) and
  `player::shutdown(window)` does stop → drop the window (Slint's `RenderingTeardown` frees the render context) → quit
  → join → destroy. A core that doesn't quit in 5 s is left to process exit instead of hanging there.
- The UI thread is mpv's render thread, and libmpv says the render thread must not make blocking client calls
  (`mpv_command`, `mpv_get/set_property`). UFlix does (seek, pause, the 4/s cache read, the info panel), so a stuck
  core freezes the whole UI: on 2026-11-10 a VA-API decode hung inside nvidia-vaapi-driver (`mp_image_hw_download` →
  `nvidia_drv_video.so`), the UI blocked in `mp_dispatch_lock`, and KWin killed the window (SIGABRT via
  `kwin_killer_helper`; not a crash).
- With the render API mpv loads every GL interop up front: on NVIDIA that includes VA-API, so nvidia-vaapi-driver is in
  the process even when nvdec decodes, and `auto-safe` falls back to it if nvdec fails.
- Never call libmpv from a `stream_cb` callback; always give it a cancel function.
**Live TV** (`engine::tv`, `pages/tv.rs`)
- Playlists are a list in Settings → Live TV, stored comma-separated in `TvLinks` (none ⇒ iptv-org's `bd.m3u`).
- Entries whose names differ only by quality/tags (`(720p)`, `(HD)`, `[Geo-blocked]`) merge into one channel; other
  parentheses stay ("Channel S (Bangladesh)"). A channel with several streams opens a picker. iptv-org's own lists are
  already de-duplicated, so merging matters for third-party playlists.
- After loading, every stream is probed 10 at a time (`probe_all`: 2xx + first bytes, `#EXTM3U` for HLS, 10 s);
  cards show Live/Offline and "Hide offline" drops dead channels. A reload aborts the running probes.
- "Recently watched" on the Live TV page (not Home): the last 5 channels played (`TvRecent`, one per channel name, the
  exact stream replayed), with the live status once the probes reach that stream.
- `#EXTVLCOPT` header lines are skipped, not passed to mpv: streams that need a referrer/user agent show as offline.

- Per-file `loadfile` options use `%len%value` quoting; external audio/subtitles are added after `FILE_LOADED`.
- `keep-open=yes` ⇒ EOF comes from `eof-reached`.

**Slint**
- `row` can't be a property name (it clashes with grid-layout properties); Flickable/ScrollView use `content-x/-y`,
  `content-width/-height` (the `viewport-*` names are deprecated).
- `global::<T>()` borrows the window: keep the window in a local (`let w = ui::window(); w.global::<T>()`).
- `take_snapshot` includes the mpv video (it re-renders through the notifier), so tours show the real picture.

**No idle polling**
- Nothing ticks while the user is idle. Timers that run only during an activity are fine: the 1 s torrent/info refresh
  while a torrent plays or the info panel is open, the Torrents page refresh while it is visible, auto-hide one-shots.

**Diagnostics** (off unless set)
- `UFLIX_DEBUG=1` prints mpv's log lines; `UFLIX_MPV_LOG=<file>` writes mpv's full log.
- `UFLIX_HWDEC=<value>` replaces `hwdec` (`nvdec`, `nvdec-copy`, `vaapi`, `vaapi-copy`, `no`); the decoder and interop
  in use are printed once per file.
- `UFLIX_MEMLOG=<secs>` prints RSS (anon/file), threads, MemAvailable, mpv's demuxer cache and torrent counters to
  stderr from its own thread.

**Testing on the dev machine**
- Test windows open on the LG monitor (`.claude/scripts/test-monitor2.sh`, a KWin script); media plays at 10 %
  (`UFLIX_TEST_VOLUME=10`, never saved); tours use a scratch `XDG_DATA_HOME`.

---

## 4. Decisions

- **Rust + Slint** replaced the C# Uno app (2026-11): lower CPU/GPU/RAM with zero-copy decoding and translucent overlays
  (Uno's Linux video was an opaque native child window).
- **librqbit** replaced the C++ libtorrent wrapper (tsdl): no native build, Apache-2.0.
- **SQLite** (`<data>/LocalData.sqlite`) replaced LiteDB; the old data is not imported.
- **Self-updater** over GitHub releases (+ `SHA256SUMS`) replaced Velopack: AppImage swapped in place, Windows installer
  run silently.
- **Data folders** are unchanged from the C# app (`~/.local/share/UFlix`, `Documents\UFlix`, `~/Downloads/UFlix`).
- **Licence:** GPL-3.0-or-later; every dependency must stay GPLv3-compatible (Slint is used under GPLv3).

---

## 5. Open work

- Windows: build and run the app and installer on a Windows machine (only CI-compiled so far); check d3d11 interop.
- Natives: run the Natives workflow once to publish a set with the nvdec/VA-API libmpv in the new `runtimes/` layout
  (the pinned `natives-2` set still has the C# folder layout).
- uTP for torrents once librqbit marks it stable.
- A memory probe like the C# `UFLIX_MEMPROBE` (page cycling + RSS log); `UFLIX_MEMLOG` covers the logging half.
- Stop making blocking libmpv calls on the UI (render) thread: async commands/properties and observed values instead.
- Decide the NVIDIA hwdec default (`nvdec` + CUDA interop only, no VA-API) once the `UFLIX_HWDEC` tests are in.
