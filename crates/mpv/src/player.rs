//! One mpv instance. Events are raised on the "mpv-events" thread; the caller marshals them to its UI.

use crate::ffi::{self, api};
use crate::stream;
use serde::Deserialize;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackType {
    Video,
    Audio,
    Subtitle,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub kind: TrackType,
    pub title: String,
    pub lang: String,
    pub codec: String,
    pub external: bool,
    pub selected: bool,
}

impl Track {
    /// "English · ass", "Track 2 · aac".
    pub fn description(&self) -> String {
        let name = if !self.title.is_empty() {
            self.title.clone()
        } else if !self.lang.is_empty() {
            self.lang.clone()
        } else {
            format!("Track {}", self.id)
        };
        if self.codec.is_empty() { name } else { format!("{name} · {}", self.codec) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
}

#[derive(Clone, Debug)]
pub enum Event {
    Position(f64),
    Duration(f64),
    Pause(bool),
    Buffering(bool),
    Volume(f64),
    SubDelay(f64),
    Tracks(Vec<Track>),
    /// `hwdec-current` ("" when no video decoder, "no" for software) and the GPU interop mpv has loaded for it.
    Hwdec {
        current: String,
        interop: String,
    },
    FileLoaded,
    Ended(EndReason, String),
    Log(String),
}

/// mpv options set before mpv_initialize. The defaults match UFlix's MpvSharp.
pub struct Options {
    pub hwdec: String,
    pub vo: String,
    pub volume_max: f64,
    pub log_level: String,
    /// Applied last (the video surface's options, subtitle styling, ...).
    pub extra: Vec<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        Self { hwdec: "auto-safe".into(), vo: "libmpv".into(), volume_max: 200.0, log_level: "warn".into(), extra: Vec::new() }
    }
}

#[derive(Clone, Default)]
pub struct LoadOptions {
    pub audio_url: Option<String>,
    pub subtitles: Vec<String>,
    pub start: Option<f64>,
    pub title: Option<String>,
}

type Done = Box<dyn FnOnce(Result<(), String>) + Send>;

#[derive(Default)]
struct LoadState {
    done: Option<Done>,
    awaiting_start: bool,
    stream_uri: Option<String>,
    after: Option<LoadOptions>,
}

pub(crate) struct Handle(pub(crate) *mut c_void);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

struct Shared {
    h: Handle,
    load: Mutex<LoadState>,
    on_event: Box<dyn Fn(Event) + Send + Sync>,
}

pub struct Player {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

const P_TIME: u64 = 1;
const P_DURATION: u64 = 2;
const P_PAUSE: u64 = 3;
const P_CACHE: u64 = 4;
const P_EOF: u64 = 5;
const P_TRACKS: u64 = 6;
const P_VOLUME: u64 = 7;
const P_SUBDELAY: u64 = 8;
const P_HWDEC: u64 = 9;

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

impl Player {
    pub fn new(options: Options, on_event: impl Fn(Event) + Send + Sync + 'static) -> Result<Self, String> {
        let a = api()?;
        ffi::set_c_numeric_locale();
        let h = unsafe { (a.create)() };
        if h.is_null() {
            return Err("mpv_create failed".into());
        }
        let shared = Arc::new(Shared { h: Handle(h), load: Mutex::default(), on_event: Box::new(on_event) });
        let opt = |k: &str, v: &str| unsafe {
            let r = (a.set_option_string)(h, cstr(k).as_ptr(), cstr(v).as_ptr());
            if r < 0 {
                (shared.on_event)(Event::Log(format!("[mpv] option {k}={v}: {}", ffi::error_string(r))));
            }
        };
        for (k, v) in [
            ("terminal", "no"),
            ("config", "no"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("osc", "no"),
            ("ytdl", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("input-cursor", "no"),
            ("cursor-autohide", "no"),
            ("stop-screensaver", "yes"),
            ("demuxer-max-bytes", "32MiB"),
            ("demuxer-max-back-bytes", "8MiB"),
        ] {
            opt(k, v);
        }
        opt("hwdec", &options.hwdec);
        opt("vo", &options.vo);
        opt("volume-max", &options.volume_max.to_string());
        for (k, v) in &options.extra {
            opt(k, v);
        }
        let r = unsafe { (a.initialize)(h) };
        if r < 0 {
            unsafe { (a.terminate_destroy)(h) };
            return Err(format!("mpv_initialize: {}", ffi::error_string(r)));
        }
        unsafe {
            (a.request_log_messages)(h, cstr(&options.log_level).as_ptr());
            for (id, name, fmt) in [
                (P_TIME, "time-pos", ffi::FORMAT_DOUBLE),
                (P_DURATION, "duration", ffi::FORMAT_DOUBLE),
                (P_PAUSE, "pause", ffi::FORMAT_FLAG),
                (P_CACHE, "paused-for-cache", ffi::FORMAT_FLAG),
                (P_EOF, "eof-reached", ffi::FORMAT_FLAG),
                (P_TRACKS, "track-list", ffi::FORMAT_STRING),
                (P_VOLUME, "volume", ffi::FORMAT_DOUBLE),
                (P_SUBDELAY, "sub-delay", ffi::FORMAT_DOUBLE),
                (P_HWDEC, "hwdec-current", ffi::FORMAT_STRING),
            ] {
                (a.observe_property)(h, id, cstr(name).as_ptr(), fmt);
            }
        }
        stream::register(h)?;
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("mpv-events".into()).spawn(move || event_loop(&s)).map_err(|e| e.to_string())?;
        Ok(Self { shared, thread: Some(thread) })
    }

    pub(crate) fn raw(&self) -> *mut c_void {
        self.shared.h.0
    }

    /// Loads a file or URL; `done` runs on the event thread when it is loaded (external audio and subtitles already added) or fails.
    pub fn load(&self, url: &str, options: LoadOptions, done: impl FnOnce(Result<(), String>) + Send + 'static) {
        let mut file_opts = Vec::new();
        if let Some(start) = options.start.filter(|s| *s > 0.0) {
            file_opts.push(file_opt("start", &format!("{start:.3}")));
        }
        if let Some(title) = options.title.as_deref().filter(|t| !t.is_empty()) {
            file_opts.push(file_opt("force-media-title", title));
        }
        {
            let mut st = self.shared.load.lock().unwrap();
            if let Some(old) = st.done.take() {
                old(Err("replaced".into()));
            }
            if let Some(uri) = st.stream_uri.take()
                && uri != url
            {
                stream::remove(&uri);
            }
            st.done = Some(Box::new(done));
            st.awaiting_start = true;
            st.stream_uri = url.starts_with(stream::PREFIX).then(|| url.to_string());
            st.after = Some(options);
        }
        self.set_property("pause", "yes");
        let r = self.command(&["loadfile", url, "replace", "-1", &file_opts.join(",")]);
        if r < 0
            && let Some(done) = self.shared.load.lock().unwrap().done.take()
        {
            done(Err(format!("loadfile: {}", ffi::error_string(r))));
        }
    }

    /// Plays a stream through mpv's stream_cb ("uflix://<id>"). mpv calls the source's `cancel` on stop.
    pub fn load_stream(
        &self,
        source: Arc<dyn stream::StreamSource>,
        options: LoadOptions,
        done: impl FnOnce(Result<(), String>) + Send + 'static,
    ) {
        let uri = stream::add(source);
        self.load(&uri, options, done);
    }

    pub fn play(&self) {
        self.set_property("pause", "no");
    }
    pub fn pause(&self) {
        self.set_property("pause", "yes");
    }
    pub fn toggle_pause(&self) {
        self.command(&["cycle", "pause"]);
    }
    pub fn stop(&self) {
        self.command(&["stop"]);
    }
    /// Like [`Player::stop`] but doesn't wait for the core (for shutdown, where a stuck core must not block exit).
    pub fn stop_async(&self) {
        command_async(&self.shared.h, &["stop"]);
    }
    pub fn seek(&self, seconds: f64, exact: bool) {
        self.command(&["seek", &format!("{:.3}", seconds.max(0.0)), if exact { "absolute+exact" } else { "absolute+keyframes" }]);
    }
    pub fn seek_relative(&self, seconds: f64) {
        self.command(&["seek", &format!("{seconds:.3}"), "relative"]);
    }
    pub fn set_volume(&self, volume: f64) {
        self.set_double("volume", volume);
    }
    pub fn set_sub_delay(&self, seconds: f64) {
        self.set_double("sub-delay", seconds);
    }
    pub fn set_speed(&self, speed: f64) {
        self.set_double("speed", speed);
    }
    pub fn add_subtitle(&self, path: &str, select: bool) {
        self.command(&["sub-add", path, if select { "select" } else { "auto" }]);
    }
    /// id < 0 turns that track type off.
    pub fn select_track(&self, kind: TrackType, id: i64) {
        let name = match kind {
            TrackType::Video => "vid",
            TrackType::Audio => "aid",
            TrackType::Subtitle => "sid",
        };
        self.set_property(name, &if id < 0 { "no".to_string() } else { id.to_string() });
    }

    pub fn get_property(&self, name: &str) -> Option<String> {
        get_string(&self.shared.h, name)
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let a = api().ok()?;
        let mut v = 0f64;
        let r = unsafe { (a.get_property)(self.raw(), cstr(name).as_ptr(), ffi::FORMAT_DOUBLE, &mut v as *mut f64 as *mut c_void) };
        (r >= 0).then_some(v)
    }

    pub fn set_property(&self, name: &str, value: &str) {
        if let Ok(a) = api() {
            let r = unsafe { (a.set_property_string)(self.raw(), cstr(name).as_ptr(), cstr(value).as_ptr()) };
            if r < 0 {
                (self.shared.on_event)(Event::Log(format!("[mpv] set {name}={value}: {}", ffi::error_string(r))));
            }
        }
    }

    pub fn set_double(&self, name: &str, mut value: f64) {
        if let Ok(a) = api() {
            unsafe { (a.set_property)(self.raw(), cstr(name).as_ptr(), ffi::FORMAT_DOUBLE, &mut value as *mut f64 as *mut c_void) };
        }
    }

    pub fn command(&self, args: &[&str]) -> c_int {
        command(&self.shared.h, args, &*self.shared.on_event)
    }
}

fn command(h: &Handle, args: &[&str], log: &(dyn Fn(Event) + Send + Sync)) -> c_int {
    let Ok(a) = api() else { return ffi::ERROR_GENERIC };
    let owned: Vec<CString> = args.iter().map(|s| cstr(s)).collect();
    let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let r = unsafe { (a.command)(h.0, ptrs.as_ptr()) };
    if r < 0 {
        log(Event::Log(format!("[mpv] command {}: {}", args.join(" "), ffi::error_string(r))));
    }
    r
}

/// Queues a command without waiting for the core (mpv copies the arguments; the reply event is ignored).
fn command_async(h: &Handle, args: &[&str]) -> c_int {
    let Ok(a) = api() else { return ffi::ERROR_GENERIC };
    let owned: Vec<CString> = args.iter().map(|s| cstr(s)).collect();
    let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe { (a.command_async)(h.0, 0, ptrs.as_ptr()) }
}

fn get_string(h: &Handle, name: &str) -> Option<String> {
    let a = api().ok()?;
    unsafe {
        let p = (a.get_property_string)(h.0, cstr(name).as_ptr());
        if p.is_null() {
            return None;
        }
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        (a.free)(p as *mut c_void);
        Some(s)
    }
}

/// mpv's %length%value quoting: values may contain commas, colons or '=' (URLs, titles).
fn file_opt(name: &str, value: &str) -> String {
    format!("{name}=%{}%{value}", value.len())
}

impl Drop for Player {
    /// quit → join the event thread → destroy. Destroying first can hang or crash with a live VO or stream callback.
    /// Free any render context (on the GL thread) before dropping the player.
    /// If the core doesn't quit (a decoder stuck in a GPU driver), mpv is left to process exit instead of hanging here.
    fn drop(&mut self) {
        let Ok(a) = api() else { return };
        // async: a synchronous command would wait for the core forever if it is stuck
        command_async(&self.shared.h, &["quit"]);
        if let Some(t) = self.thread.take() {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !t.is_finished() && std::time::Instant::now() < deadline {
                unsafe { (a.wakeup)(self.shared.h.0) };
                std::thread::sleep(Duration::from_millis(20));
            }
            if !t.is_finished() {
                // terminate_destroy would block on the same stuck core; the OS reclaims everything at exit
                eprintln!("[mpv] the core did not quit within 5 s; not destroying it");
                return;
            }
            let _ = t.join();
        }
        unsafe { (a.terminate_destroy)(self.shared.h.0) };
        if let Some(uri) = self.shared.load.lock().unwrap_or_else(|e| e.into_inner()).stream_uri.take() {
            stream::remove(&uri);
        }
    }
}

#[derive(Deserialize)]
struct RawTrack {
    id: Option<i64>,
    #[serde(rename = "type")]
    kind: Option<String>,
    title: Option<String>,
    lang: Option<String>,
    codec: Option<String>,
    #[serde(default)]
    external: bool,
    #[serde(default)]
    selected: bool,
}

fn parse_tracks(json: &str) -> Vec<Track> {
    let raw: Vec<RawTrack> = serde_json::from_str(json).unwrap_or_default();
    raw.into_iter()
        .filter_map(|t| {
            let kind = match t.kind.as_deref()? {
                "video" => TrackType::Video,
                "audio" => TrackType::Audio,
                "sub" => TrackType::Subtitle,
                _ => return None,
            };
            Some(Track {
                id: t.id?,
                kind,
                title: t.title.unwrap_or_default(),
                lang: t.lang.unwrap_or_default(),
                codec: t.codec.unwrap_or_default(),
                external: t.external,
                selected: t.selected,
            })
        })
        .collect()
}

fn event_loop(s: &Shared) {
    let Ok(a) = api() else { return };
    let emit = |e: Event| (s.on_event)(e);
    loop {
        let ev = unsafe { &*(a.wait_event)(s.h.0, -1.0) };
        match ev.event_id {
            ffi::EVENT_SHUTDOWN => {
                if let Some(done) = s.load.lock().unwrap().done.take() {
                    done(Err("shutdown".into()));
                }
                return;
            }
            ffi::EVENT_LOG_MESSAGE => unsafe {
                let m = &*(ev.data as *const ffi::EventLogMessage);
                let prefix = CStr::from_ptr(m.prefix).to_string_lossy();
                let text = CStr::from_ptr(m.text).to_string_lossy();
                emit(Event::Log(format!("[{prefix}] {}", text.trim_end())));
            },
            ffi::EVENT_START_FILE => s.load.lock().unwrap().awaiting_start = false,
            ffi::EVENT_FILE_LOADED => {
                let (done, after) = {
                    let mut st = s.load.lock().unwrap();
                    (st.done.take(), st.after.take())
                };
                // external audio and subtitles are added after FILE_LOADED (path-list options split on ':' on Linux)
                if let Some(after) = after {
                    if let Some(audio) = after.audio_url.as_deref().filter(|u| !u.is_empty()) {
                        command(&s.h, &["audio-add", audio, "select"], &emit);
                    }
                    for sub in &after.subtitles {
                        command(&s.h, &["sub-add", sub, "auto"], &emit);
                    }
                }
                emit(Event::FileLoaded);
                if let Some(done) = done {
                    done(Ok(()));
                }
            }
            ffi::EVENT_END_FILE => {
                let e = unsafe { &*(ev.data as *const ffi::EventEndFile) };
                let reason = match e.reason {
                    0 => EndReason::Eof,
                    2 => EndReason::Stop,
                    3 => EndReason::Quit,
                    4 => EndReason::Error,
                    _ => EndReason::Redirect,
                };
                let error = if e.error < 0 { ffi::error_string(e.error) } else { String::new() };
                let mut st = s.load.lock().unwrap();
                // the END_FILE of the file being replaced arrives before our START_FILE: it isn't ours
                if st.awaiting_start {
                    continue;
                }
                let done = st.done.take();
                if let Some(uri) = st.stream_uri.take() {
                    stream::remove(&uri);
                }
                drop(st);
                if let Some(done) = done {
                    done(Err(if reason == EndReason::Error { format!("load failed: {error}") } else { "stopped".into() }));
                }
                emit(Event::Ended(reason, error));
            }
            ffi::EVENT_PROPERTY_CHANGE => unsafe {
                let p = &*(ev.data as *const ffi::EventProperty);
                let none = p.format == ffi::FORMAT_NONE || p.data.is_null();
                if none {
                    match ev.reply_userdata {
                        P_TRACKS => emit(Event::Tracks(Vec::new())),
                        P_HWDEC => emit(Event::Hwdec { current: String::new(), interop: String::new() }),
                        _ => {}
                    }
                    continue;
                }
                let dbl = || *(p.data as *const f64);
                let flag = || *(p.data as *const c_int) != 0;
                let string = || CStr::from_ptr(*(p.data as *const *const c_char)).to_string_lossy().into_owned();
                match ev.reply_userdata {
                    P_TIME => emit(Event::Position(dbl())),
                    P_DURATION => emit(Event::Duration(dbl())),
                    P_PAUSE => emit(Event::Pause(flag())),
                    P_CACHE => emit(Event::Buffering(flag())),
                    // keep-open=yes: a file that plays to its end stays open and paused, so EOF shows up here
                    P_EOF if flag() => emit(Event::Ended(EndReason::Eof, String::new())),
                    P_TRACKS => emit(Event::Tracks(parse_tracks(&string()))),
                    P_VOLUME => emit(Event::Volume(dbl())),
                    P_SUBDELAY => emit(Event::SubDelay(dbl())),
                    P_HWDEC => {
                        let current = string();
                        let hw = !current.is_empty() && current != "no";
                        let interop = if hw { get_string(&s.h, "hwdec-interop").unwrap_or_default() } else { String::new() };
                        emit(Event::Hwdec { current, interop })
                    }
                    _ => {}
                }
            },
            _ => {}
        }
    }
}

/// libmpv's client API version as (major, minor).
pub fn client_api_version() -> Option<(u32, u32)> {
    let v = unsafe { (api().ok()?.client_api_version)() };
    Some(((v >> 16) as u32, (v & 0xffff) as u32))
}
