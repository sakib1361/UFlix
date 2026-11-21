//! The few libmpv calls the spike needs, linked directly (statically on iOS). Video comes out of mpv's software
//! render API as RGB frames that the UI shows as an image: simple and renderer-independent (Slint is Skia/Metal on
//! iOS, so the desktop app's OpenGL path does not apply). Decoding still uses VideoToolbox (copy-back).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Condvar, Mutex};

#[allow(non_camel_case_types)]
type mpv_handle = c_void;
#[allow(non_camel_case_types)]
type mpv_render_context = c_void;

#[repr(C)]
struct Event {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct EventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct EventEndFile {
    reason: c_int,
    error: c_int,
}

#[repr(C)]
struct RenderParam {
    ty: c_int,
    data: *mut c_void,
}

unsafe extern "C" {
    fn mpv_create() -> *mut mpv_handle;
    fn mpv_initialize(ctx: *mut mpv_handle) -> c_int;
    fn mpv_terminate_destroy(ctx: *mut mpv_handle);
    fn mpv_set_option_string(ctx: *mut mpv_handle, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_command(ctx: *mut mpv_handle, args: *const *const c_char) -> c_int;
    fn mpv_observe_property(ctx: *mut mpv_handle, id: u64, name: *const c_char, format: c_int) -> c_int;
    fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut Event;
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_render_context_create(res: *mut *mut mpv_render_context, mpv: *mut mpv_handle, params: *mut RenderParam) -> c_int;
    fn mpv_render_context_set_update_callback(
        ctx: *mut mpv_render_context,
        cb: Option<unsafe extern "C" fn(*mut c_void)>,
        cb_ctx: *mut c_void,
    );
    fn mpv_render_context_update(ctx: *mut mpv_render_context) -> u64;
    fn mpv_render_context_render(ctx: *mut mpv_render_context, params: *mut RenderParam) -> c_int;
}

const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_SW_SIZE: c_int = 17;
const RENDER_PARAM_SW_FORMAT: c_int = 18;
const RENDER_PARAM_SW_STRIDE: c_int = 19;
const RENDER_PARAM_SW_POINTER: c_int = 20;
const RENDER_UPDATE_FRAME: u64 = 1;

/// What the event thread reports to the UI.
pub enum Update {
    Position(f64),
    Duration(f64),
    Paused(bool),
    Buffering(bool),
    VideoSize(i64, i64),
    Loaded,
    Ended(String),
}

/// A frame of `w * h` RGBA pixels (alpha already set to opaque).
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Handle(*mut mpv_handle);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

pub struct Player {
    h: Handle,
}

/// Wakes the render thread when mpv has a new frame (set from mpv's update callback, which must not call mpv).
struct Wake {
    pending: Mutex<bool>,
    cv: Condvar,
}

unsafe extern "C" fn on_render_update(ctx: *mut c_void) {
    let wake = unsafe { &*(ctx as *const Wake) };
    *wake.pending.lock().unwrap() = true;
    wake.cv.notify_one();
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn err(code: c_int) -> String {
    unsafe { CStr::from_ptr(mpv_error_string(code)) }.to_string_lossy().into_owned()
}

impl Player {
    /// Creates mpv, the event thread (→ `on_update`) and the render thread (→ `on_frame`). `target_size` gives the
    /// size frames are rendered at (width, height), read before every frame.
    pub fn new(
        on_update: impl Fn(Update) + Send + 'static,
        on_frame: impl Fn(Frame) + Send + 'static,
        target_size: impl Fn() -> (u32, u32) + Send + 'static,
    ) -> Result<Player, String> {
        let h = unsafe { mpv_create() };
        if h.is_null() {
            return Err("mpv_create failed".into());
        }
        let set = |k: &str, v: &str| unsafe { mpv_set_option_string(h, cstr(k).as_ptr(), cstr(v).as_ptr()) };
        set("vo", "libmpv");
        set("hwdec", "auto-copy"); // VideoToolbox, frames copied back for the software renderer
        set("sw-fast", "yes");
        set("keep-open", "yes");
        set("cache", "yes");
        set("demuxer-max-bytes", "64MiB");
        set("terminal", "yes");
        set("msg-level", "all=info");
        let r = unsafe { mpv_initialize(h) };
        if r < 0 {
            return Err(format!("mpv_initialize: {}", err(r)));
        }
        for (id, name, fmt) in [
            (1, "time-pos", FORMAT_DOUBLE),
            (2, "duration", FORMAT_DOUBLE),
            (3, "pause", FORMAT_FLAG),
            (4, "paused-for-cache", FORMAT_FLAG),
            (5, "dwidth", FORMAT_INT64),
            (6, "dheight", FORMAT_INT64),
        ] {
            unsafe { mpv_observe_property(h, id, cstr(name).as_ptr(), fmt) };
        }
        let handle = Handle(h);

        // Render thread: owns the render context, all render calls happen here.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        std::thread::Builder::new().name("mpv-render".into()).spawn(move || render_loop(handle, on_frame, target_size, ready_tx)).unwrap();
        ready_rx.recv().map_err(|e| e.to_string())??;

        std::thread::Builder::new().name("mpv-events".into()).spawn(move || event_loop(handle, on_update)).unwrap();
        Ok(Player { h: handle })
    }

    pub fn command(&self, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let r = unsafe { mpv_command(self.h.0, ptrs.as_ptr()) };
        if r < 0 {
            eprintln!("mpv command {args:?}: {}", err(r));
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // The render context lives until the process exits (the spike never closes the player).
        unsafe { mpv_terminate_destroy(self.h.0) };
    }
}

fn event_loop(h: Handle, on_update: impl Fn(Update)) {
    loop {
        let ev = unsafe { &*mpv_wait_event(h.0, -1.0) };
        match ev.event_id {
            EVENT_SHUTDOWN => return,
            EVENT_FILE_LOADED => on_update(Update::Loaded),
            EVENT_END_FILE => {
                let end = unsafe { &*(ev.data as *const EventEndFile) };
                let msg = if end.error < 0 { format!("Playback failed: {}", err(end.error)) } else { String::new() };
                on_update(Update::Ended(msg));
            }
            EVENT_PROPERTY_CHANGE => {
                let p = unsafe { &*(ev.data as *const EventProperty) };
                if p.data.is_null() {
                    continue;
                }
                let u = unsafe {
                    match (ev.reply_userdata, p.format) {
                        (1, FORMAT_DOUBLE) => Update::Position(*(p.data as *const f64)),
                        (2, FORMAT_DOUBLE) => Update::Duration(*(p.data as *const f64)),
                        (3, FORMAT_FLAG) => Update::Paused(*(p.data as *const c_int) != 0),
                        (4, FORMAT_FLAG) => Update::Buffering(*(p.data as *const c_int) != 0),
                        (5, FORMAT_INT64) => Update::VideoSize(*(p.data as *const i64), -1),
                        (6, FORMAT_INT64) => Update::VideoSize(-1, *(p.data as *const i64)),
                        _ => continue,
                    }
                };
                on_update(u);
            }
            _ => {}
        }
    }
}

fn render_loop(
    h: Handle,
    on_frame: impl Fn(Frame),
    target_size: impl Fn() -> (u32, u32),
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let api = cstr("sw");
    let mut params =
        [RenderParam { ty: RENDER_PARAM_API_TYPE, data: api.as_ptr() as *mut c_void }, RenderParam { ty: 0, data: std::ptr::null_mut() }];
    let mut rc: *mut mpv_render_context = std::ptr::null_mut();
    let r = unsafe { mpv_render_context_create(&mut rc, h.0, params.as_mut_ptr()) };
    if r < 0 {
        let _ = ready.send(Err(format!("mpv_render_context_create: {}", err(r))));
        return;
    }
    let wake: &'static Wake = Box::leak(Box::new(Wake { pending: Mutex::new(false), cv: Condvar::new() }));
    unsafe { mpv_render_context_set_update_callback(rc, Some(on_render_update), wake as *const Wake as *mut c_void) };
    let _ = ready.send(Ok(()));

    let format = cstr("rgb0");
    // 64-byte aligned buffer (mpv's fast path); reused while the size stays the same.
    let mut buf: Vec<u64> = Vec::new();
    loop {
        {
            let mut p = wake.pending.lock().unwrap();
            while !*p {
                p = wake.cv.wait(p).unwrap();
            }
            *p = false;
        }
        if unsafe { mpv_render_context_update(rc) } & RENDER_UPDATE_FRAME == 0 {
            continue;
        }
        let (w, h) = target_size();
        let (w, h) = ((w.max(16) + 15) & !15, h.max(16)); // width a multiple of 16 → 64-byte stride
        let stride = w as usize * 4;
        let len = stride * h as usize;
        buf.resize(len / 8 + 8, 0);
        let off = (64 - (buf.as_ptr() as usize % 64)) % 64;
        let ptr = unsafe { (buf.as_mut_ptr() as *mut u8).add(off) };
        let mut size = [w as c_int, h as c_int];
        let mut stride_v = stride;
        let mut rp = [
            RenderParam { ty: RENDER_PARAM_SW_SIZE, data: size.as_mut_ptr() as *mut c_void },
            RenderParam { ty: RENDER_PARAM_SW_FORMAT, data: format.as_ptr() as *mut c_void },
            RenderParam { ty: RENDER_PARAM_SW_STRIDE, data: &mut stride_v as *mut usize as *mut c_void },
            RenderParam { ty: RENDER_PARAM_SW_POINTER, data: ptr as *mut c_void },
            RenderParam { ty: 0, data: std::ptr::null_mut() },
        ];
        if unsafe { mpv_render_context_render(rc, rp.as_mut_ptr()) } < 0 {
            continue;
        }
        let mut rgba = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        on_frame(Frame { w, h, rgba });
    }
}
