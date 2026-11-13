//! The video: one mpv instance for the app's lifetime, drawn by mpv's OpenGL render API into the window's frame before
//! Slint draws the UI on top (so overlays can be translucent). mpv's events are forwarded to the UI thread.

use crate::runtime::ui;
use crate::ui::AppWindow;
use mpv::{Event, NativeDisplay, Options, Player, RenderContext};
use slint::{ComponentHandle, GraphicsAPI, RenderingState};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// None until first use; taken by [`shutdown`] so mpv is destroyed before the process exits.
static PLAYER: Mutex<Option<Result<Arc<Player>, String>>> = Mutex::new(None);
static SHUT_DOWN: AtomicBool = AtomicBool::new(false);

fn slot() -> MutexGuard<'static, Option<Result<Arc<Player>, String>>> {
    PLAYER.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    /// Draw the video only while the player page is open.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static RENDER: RefCell<Option<RenderContext>> = const { RefCell::new(None) };
    /// Waiting for the render context (mpv disables video for a file loaded before it exists).
    static WAITERS: RefCell<Vec<tokio::sync::oneshot::Sender<()>>> = const { RefCell::new(Vec::new()) };
}

/// Resolves once mpv's render context exists (created on the next frame while the player is active).
pub async fn ready() {
    if RENDER.with(|r| r.borrow().is_some()) {
        return;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    WAITERS.with(|w| w.borrow_mut().push(tx));
    crate::ui::window().window().request_redraw();
    let _ = rx.await;
}

/// The mpv player, created on first use.
pub fn player() -> Result<Arc<Player>, String> {
    if SHUT_DOWN.load(Ordering::Acquire) {
        return Err("the player has shut down".into());
    }
    slot()
        .get_or_insert_with(|| {
            let s = engine::settings::subtitle_font_size();
            let mut extra: Vec<(String, String)> = vec![
                ("sub-font-size".into(), format!("{s}")),
                ("sub-color".into(), engine::settings::subtitle_color()),
                ("sub-border-size".into(), "2.5".into()),
                ("slang".into(), "en,eng".into()),
                ("cache".into(), "yes".into()),
                // torrents: pause when the cache runs dry, resume with 4 s buffered
                ("cache-pause-wait".into(), "4".into()),
                ("volume".into(), format!("{}", initial_volume())),
            ];
            let font = engine::settings::subtitle_font();
            if !font.trim().is_empty() {
                extra.push(("sub-font".into(), font));
            }
            if let Ok(log) = std::env::var("UFLIX_MPV_LOG") {
                extra.push(("log-file".into(), log));
            }
            let mut options = Options { extra, hwdec: hwdec_value(&engine::settings::video_decoder()), ..Default::default() };
            // diagnostics: UFLIX_HWDEC=nvdec|nvdec-copy|vaapi|vaapi-copy|no|... replaces the setting
            if let Some(hwdec) = std::env::var("UFLIX_HWDEC").ok().filter(|v| !v.trim().is_empty()) {
                eprintln!("[uflix] hwdec={hwdec} (UFLIX_HWDEC)");
                options.hwdec = hwdec;
            }
            Player::new(options, forward).map(Arc::new)
        })
        .clone()
}

// ---------------------------------------------------------------- video decoder

/// When a decoder mode is offered: always, never, if mpv loaded a GPU interop, or if it did not.
#[derive(Clone, Copy)]
enum Need {
    Always,
    Never,
    Interop(&'static str),
    NotInterop(&'static str),
}

/// A decoder API in Settings: its label, the mpv hwdec value (`-copy` appended for copy mode) and when its zero-copy
/// and copy modes can work.
struct Api {
    label: &'static str,
    hwdec: &'static str,
    zero_copy: Need,
    copy: Need,
}

const fn api(label: &'static str, hwdec: &'static str, zero_copy: Need, copy: Need) -> Api {
    Api { label, hwdec, zero_copy, copy }
}

#[cfg(windows)]
const APIS: &[Api] = &[
    api("Auto", "auto", Need::Always, Need::Never),
    // the Windows interop names are not checked yet (no Windows test run), so nothing is filtered there
    api("D3D11", "d3d11va", Need::Always, Need::Always),
    api("NVDEC", "nvdec", Need::Never, Need::Always),
    api("Software", "no", Need::Always, Need::Never),
];
#[cfg(not(windows))]
const APIS: &[Api] = &[
    api("Auto", "auto", Need::Always, Need::Never),
    api("NVDEC", "nvdec", Need::Interop("cuda"), Need::Interop("cuda")),
    api("VA-API", "vaapi", Need::Interop("vaapi"), Need::Interop("vaapi")),
    // mpv has no Vulkan→GL interop, so Vulkan is copy-only; offered where there is no NVIDIA GPU
    api("Vulkan", "vulkan", Need::Never, Need::NotInterop("cuda")),
    api("Software", "no", Need::Always, Need::Never),
];

/// A decoder API this machine can use, with the modes it offers.
pub struct Decoder {
    pub label: &'static str,
    pub hwdec: &'static str,
    pub zero_copy: bool,
    pub copy: bool,
}

impl Decoder {
    /// The stored setting for this API in the given mode (copy-only APIs are always copy).
    pub fn value(&self, copy: bool) -> String {
        if (copy && self.copy) || !self.zero_copy { format!("{}-copy", self.hwdec) } else { self.hwdec.to_string() }
    }
}

/// The decoder APIs this machine can use. mpv has no "what works" query: zero-copy needs the GPU interop mpv loaded
/// with the render context (`hwdec-interop`, remembered across runs), and until a video has been shown once every
/// choice is offered. A choice that still fails falls back to software decoding.
pub fn decoders() -> Vec<Decoder> {
    let seen = engine::settings::decoder_interops();
    let has = |i: &str| seen.split(',').any(|s| s.trim() == i);
    let ok = |n: Need| match n {
        Need::Always => true,
        Need::Never => false,
        Need::Interop(i) => seen.is_empty() || has(i),
        Need::NotInterop(i) => seen.is_empty() || !has(i),
    };
    APIS.iter()
        .map(|a| Decoder { label: a.label, hwdec: a.hwdec, zero_copy: ok(a.zero_copy), copy: ok(a.copy) })
        .filter(|d| d.zero_copy || d.copy)
        .collect()
}

/// A stored setting split into the API (its hwdec value) and copy mode.
pub fn parse_decoder(value: &str) -> (&str, bool) {
    match value.strip_suffix("-copy") {
        Some(api) => (api, true),
        None => (value, false),
    }
}

/// The mpv `hwdec` value for a stored choice.
fn hwdec_value(choice: &str) -> String {
    if choice.is_empty() || choice == "auto" { "auto-safe".into() } else { choice.into() }
}

/// Switches the decoder of the running player; mpv re-creates the decoder in place (no reload, no seek).
pub fn apply_decoder(choice: &str) {
    if let Some(p) = existing() {
        p.set_property("hwdec", &hwdec_value(choice));
    }
}

/// The decoder of the last video ("nvdec", "no" = software), for Settings.
pub fn decoder_in_use() -> String {
    IN_USE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

static IN_USE: Mutex<String> = Mutex::new(String::new());

/// Remembers mpv's loaded GPU interops for Settings. mpv reports them only once a hardware decoder has started
/// (`hwdec-interop` is still empty right after the render context is created).
fn remember_interops(interops: &str) {
    if !interops.is_empty() && interops != engine::settings::decoder_interops() {
        engine::settings::set_decoder_interops(interops.to_string());
    }
}

/// The player if it has been created (never creates it). For diagnostics on other threads.
pub fn existing() -> Option<Arc<Player>> {
    slot().as_ref()?.as_ref().ok().cloned()
}

/// Ends mpv before the process exits, in the order libmpv needs: stop → free the render context (on this thread, which
/// owns the GL context, while it still exists) → quit → join the event thread → destroy. Call on the UI thread once the
/// event loop has returned; it takes the window because dropping the last handle is what makes Slint tear down GL
/// (`RenderingTeardown` below frees the render context). Without this mpv's threads kept running through exit handlers.
pub fn shutdown(window: AppWindow) {
    SHUT_DOWN.store(true, Ordering::Release);
    ACTIVE.with(|a| a.set(false));
    let player = slot().take().and_then(Result::ok);
    if let Some(p) = &player {
        p.stop_async();
    }
    let _ = window.hide();
    drop(window);
    WAITERS.with(|w| w.borrow_mut().clear());
    let Some(mut p) = player else { return };
    if RENDER.with(|r| r.borrow().is_some()) {
        // libmpv: freeing the core before its render context is undefined behaviour; leave both to process exit
        eprintln!("mpv: the render context outlived the window; not destroying mpv");
        std::mem::forget(p);
        return;
    }
    // another thread (UFLIX_MEMLOG) may hold a clone for a moment
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match Arc::try_unwrap(p) {
            Ok(player) => return drop(player),
            Err(still) if Instant::now() < deadline => {
                p = still;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(still) => {
                eprintln!("mpv: still in use at exit; not destroying it");
                std::mem::forget(still);
                return;
            }
        }
    }
}

/// The saved volume, or $UFLIX_TEST_VOLUME for test runs (never saved).
pub fn initial_volume() -> f64 {
    std::env::var("UFLIX_TEST_VOLUME").ok().and_then(|v| v.parse().ok()).unwrap_or_else(engine::settings::volume)
}

static LAST_POS: AtomicU64 = AtomicU64::new(0);

/// mpv thread → UI thread. Position updates are thinned to 4 per second.
fn forward(e: Event) {
    if let Event::Position(p) = e {
        let last = f64::from_bits(LAST_POS.load(Ordering::Relaxed));
        if (p - last).abs() < 0.25 {
            return;
        }
        LAST_POS.store(p.to_bits(), Ordering::Relaxed);
    }
    if let Event::Hwdec { current, interop } = &e {
        log_hwdec(current, interop);
        let interop = interop.clone();
        ui(move || remember_interops(&interop));
    }
    if let Event::Log(line) = &e {
        if std::env::var_os("UFLIX_DEBUG").is_some() {
            eprintln!("{line}");
        }
        return;
    }
    ui(move || super::session::on_event(e));
}

/// One line per file: which decoder and GPU interop mpv picked (hwdec-current is unset between files).
fn log_hwdec(current: &str, interop: &str) {
    static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if current == *last {
        return;
    }
    if !current.is_empty() {
        *IN_USE.lock().unwrap_or_else(|e| e.into_inner()) = current.to_string();
    }
    *last = current.to_string();
    match current {
        "" => {}
        "no" => eprintln!("[uflix] video decoder: software"),
        _ => eprintln!("[uflix] video decoder: {current} (interop: {})", if interop.is_empty() { "none" } else { interop }),
    }
}

pub fn set_active(active: bool) {
    ACTIVE.with(|a| a.set(active));
    crate::ui::window().window().request_redraw();
}

fn native_display(window: &slint::Window) -> Option<NativeDisplay> {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    match window.window_handle().display_handle().ok()?.as_raw() {
        RawDisplayHandle::Wayland(h) => Some(NativeDisplay::Wayland(h.display.as_ptr())),
        RawDisplayHandle::Xlib(h) => Some(NativeDisplay::X11(h.display?.as_ptr())),
        _ => None,
    }
}

/// Hooks mpv's rendering into the window. Call once at start-up.
pub fn install(window: &AppWindow) -> Result<(), slint::SetRenderingNotifierError> {
    let weak = window.as_weak();
    window.window().set_rendering_notifier(move |state, api| match state {
        RenderingState::BeforeRendering => {
            if !ACTIVE.with(|a| a.get()) {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            RENDER.with(|r| {
                let mut r = r.borrow_mut();
                if r.is_none() {
                    let (Ok(p), GraphicsAPI::NativeOpenGL { get_proc_address }) = (player(), api) else { return };
                    match RenderContext::new(&p, *get_proc_address, native_display(w.window())) {
                        Ok(mut ctx) => {
                            ctx.set_update_callback(|| ui(|| crate::ui::window().window().request_redraw()));
                            *r = Some(ctx);
                            for tx in WAITERS.with(|w| std::mem::take(&mut *w.borrow_mut())) {
                                let _ = tx.send(());
                            }
                        }
                        Err(e) => eprintln!("mpv render context: {e}"),
                    }
                }
                if let Some(ctx) = r.as_ref() {
                    let size = w.window().size();
                    ctx.render(size.width as i32, size.height as i32);
                }
            });
        }
        RenderingState::AfterRendering => {
            if ACTIVE.with(|a| a.get()) {
                RENDER.with(|r| {
                    if let Some(ctx) = r.borrow().as_ref() {
                        ctx.report_swap();
                    }
                });
            }
        }
        // free mpv's GL objects while the context still exists
        RenderingState::RenderingTeardown => RENDER.with(|r| *r.borrow_mut() = None),
        _ => {}
    })
}

/// Adapts a torrent stream to mpv's stream callbacks.
pub struct TorrentSource(pub torrent::BlockingStream);

impl mpv::StreamSource for TorrentSource {
    fn read(&self, buf: &mut [u8]) -> i64 {
        self.0.read(buf)
    }
    fn seek(&self, position: u64) -> i64 {
        self.0.seek(position)
    }
    fn size(&self) -> i64 {
        self.0.len() as i64
    }
    fn cancel(&self) {
        self.0.cancel();
    }
}
