//! The video: one mpv instance for the app's lifetime, drawn by mpv's OpenGL render API into the window's frame before
//! Slint draws the UI on top (so overlays can be translucent). mpv's events are forwarded to the UI thread.

use crate::runtime::ui;
use crate::ui::AppWindow;
use mpv::{Event, NativeDisplay, Options, Player, RenderContext};
use slint::{ComponentHandle, GraphicsAPI, RenderingState};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

static PLAYER: OnceLock<Result<Arc<Player>, String>> = OnceLock::new();

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
    PLAYER
        .get_or_init(|| {
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
            Player::new(Options { extra, ..Default::default() }, forward).map(Arc::new)
        })
        .clone()
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
    if let Event::Log(line) = &e {
        if std::env::var_os("UFLIX_DEBUG").is_some() {
            eprintln!("{line}");
        }
        return;
    }
    ui(move || super::session::on_event(e));
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
