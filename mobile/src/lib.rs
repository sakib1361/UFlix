//! UFlix mobile: the app shared by iOS (now) and Android (later). A basic player that streams Big Buck Bunny:
//! libmpv decodes, its software renderer hands RGB frames to the Slint UI (ui/player.slint).

mod mpv;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

slint::include_modules!();

/// Big Buck Bunny as an HLS stream (adaptive, https); `UFLIX_MEDIA` overrides it for tests.
const SAMPLE: &str = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8";
/// Software rendering is done on the CPU, so frames are never larger than this.
const MAX_WIDTH: u32 = 1920;

#[derive(Default)]
struct Sizes {
    /// video area on screen, physical pixels
    area: (u32, u32),
    /// video's display size (dwidth, dheight)
    video: (i64, i64),
}

impl Sizes {
    /// The size to render at: the video's aspect fitted into the area, never above the video's own size.
    fn target(&self) -> (u32, u32) {
        let (vw, vh) = self.video;
        if vw <= 0 || vh <= 0 {
            return (640, 360);
        }
        let aspect = vw as f64 / vh as f64;
        let (aw, ah) = (self.area.0.max(1) as f64, self.area.1.max(1) as f64);
        let (mut w, mut h) = if aw / ah > aspect { (ah * aspect, ah) } else { (aw, aw / aspect) };
        let cap = (vw as f64).min(MAX_WIDTH as f64);
        if w > cap {
            h *= cap / w;
            w = cap;
        }
        (w.round() as u32, h.round() as u32)
    }
}

fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

pub fn run() {
    let ui = AppWindow::new().expect("create window");
    let sizes = Arc::new(Mutex::new(Sizes::default()));
    // frames handed to the UI thread but not shown yet; more than one queued means the UI is behind, so drop
    let in_flight = Arc::new(AtomicUsize::new(0));

    let on_update = {
        let weak = ui.as_weak();
        let sizes = sizes.clone();
        move |u: mpv::Update| {
            if let mpv::Update::VideoSize(w, h) = u {
                let mut s = sizes.lock().unwrap();
                if w >= 0 {
                    s.video.0 = w;
                }
                if h >= 0 {
                    s.video.1 = h;
                }
                return;
            }
            let _ = weak.upgrade_in_event_loop(move |ui| match u {
                mpv::Update::Position(p) => {
                    ui.set_position(p as f32);
                    ui.set_position_text(clock(p).into());
                }
                mpv::Update::Duration(d) => {
                    ui.set_duration(d as f32);
                    ui.set_duration_text(clock(d).into());
                }
                mpv::Update::Paused(p) => ui.set_paused(p),
                mpv::Update::Buffering(b) => ui.set_buffering(b),
                mpv::Update::Loaded => {
                    ui.set_buffering(false);
                    ui.set_message("".into());
                }
                mpv::Update::Ended(msg) => {
                    ui.set_buffering(false);
                    if !msg.is_empty() {
                        ui.set_message(msg.into());
                    }
                }
                mpv::Update::VideoSize(..) => {}
            });
        }
    };

    let on_frame = {
        let weak = ui.as_weak();
        let sizes = sizes.clone();
        let in_flight = in_flight.clone();
        move |f: mpv::Frame| {
            if in_flight.load(Ordering::Acquire) > 1 {
                return;
            }
            in_flight.fetch_add(1, Ordering::AcqRel);
            let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&f.rgba, f.w, f.h);
            let sizes = sizes.clone();
            let in_flight = in_flight.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                in_flight.fetch_sub(1, Ordering::AcqRel);
                ui.set_frame(Image::from_rgba8(pixels));
                ui.set_buffering(false);
                // keep the render size in step with the video area (rotation, resizes)
                let scale = ui.window().scale_factor();
                let area = ((ui.get_video_width() * scale) as u32, (ui.get_video_height() * scale) as u32);
                sizes.lock().unwrap().area = area;
            });
        }
    };

    let target = {
        let sizes = sizes.clone();
        move || sizes.lock().unwrap().target()
    };
    let size = ui.window().size(); // physical; refined from the video area on every frame
    sizes.lock().unwrap().area = (size.width, size.height);

    let player = match mpv::Player::new(on_update, on_frame, target) {
        Ok(p) => Rc::new(p),
        Err(e) => {
            ui.set_buffering(false);
            ui.set_message(format!("The player could not start: {e}").into());
            ui.run().expect("run");
            return;
        }
    };
    if let Ok(v) = std::env::var("UFLIX_TEST_VOLUME") {
        player.command(&["set", "volume", &v]);
    }

    let p = player.clone();
    ui.on_toggle_pause(move || p.command(&["cycle", "pause"]));
    let p = player.clone();
    ui.on_seek(move |secs| p.command(&["seek", &format!("{secs:.2}"), "absolute+keyframes"]));
    let p = player.clone();
    ui.on_skip(move |secs| p.command(&["seek", &format!("{secs:.2}"), "relative+keyframes"]));

    let media = std::env::var("UFLIX_MEDIA").unwrap_or_else(|_| SAMPLE.to_owned());
    // load once the window is up, so the first frame has a surface to go to
    let p = player.clone();
    let pending = RefCell::new(Some(media));
    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
        if let Some(m) = pending.borrow_mut().take() {
            p.command(&["loadfile", &m]);
        }
    });

    ui.run().expect("run");
}
