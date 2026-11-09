//! Raw libmpv API, resolved at runtime from the bundled library (never the system copy in a release).
//! Lookup: $MPV_LIB → <exe>/runtimes/<rid>/native → <exe> → the repo's runtimes folder (development).

use libloading::Library;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::OnceLock;

pub type Cb = unsafe extern "C" fn(*mut c_void);

#[repr(C)]
pub struct Event {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct EventProperty {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct EventLogMessage {
    pub prefix: *const c_char,
    pub level: *const c_char,
    pub text: *const c_char,
    pub log_level: c_int,
}

#[repr(C)]
pub struct EventEndFile {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: c_int,
}

#[repr(C)]
pub struct RenderParam {
    pub ty: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct OpenGlInitParams {
    pub get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub ctx: *mut c_void,
}

#[repr(C)]
pub struct OpenGlFbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

#[repr(C)]
pub struct StreamCbInfo {
    pub cookie: *mut c_void,
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, u64) -> i64>,
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64) -> i64>,
    pub size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub close: Option<unsafe extern "C" fn(*mut c_void)>,
    pub cancel: Option<unsafe extern "C" fn(*mut c_void)>,
}

pub type StreamOpen = unsafe extern "C" fn(*mut c_void, *mut c_char, *mut StreamCbInfo) -> c_int;

pub const EVENT_NONE: c_int = 0;
pub const EVENT_SHUTDOWN: c_int = 1;
pub const EVENT_LOG_MESSAGE: c_int = 2;
pub const EVENT_START_FILE: c_int = 6;
pub const EVENT_END_FILE: c_int = 7;
pub const EVENT_FILE_LOADED: c_int = 8;
pub const EVENT_VIDEO_RECONFIG: c_int = 17;
pub const EVENT_PROPERTY_CHANGE: c_int = 22;

pub const FORMAT_NONE: c_int = 0;
pub const FORMAT_STRING: c_int = 1;
pub const FORMAT_FLAG: c_int = 3;
pub const FORMAT_INT64: c_int = 4;
pub const FORMAT_DOUBLE: c_int = 5;

pub const ERROR_LOADING_FAILED: c_int = -13;
pub const ERROR_UNSUPPORTED: c_int = -18;
pub const ERROR_GENERIC: c_int = -20;

pub const RENDER_PARAM_INVALID: c_int = 0;
pub const RENDER_PARAM_API_TYPE: c_int = 1;
pub const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const RENDER_PARAM_FLIP_Y: c_int = 4;
pub const RENDER_PARAM_X11_DISPLAY: c_int = 8;
pub const RENDER_PARAM_WL_DISPLAY: c_int = 9;

macro_rules! api {
    ($($field:ident : $sym:literal => $ty:ty,)*) => {
        pub struct Api {
            _lib: Library,
            pub path: PathBuf,
            $(pub $field: $ty,)*
        }
        impl Api {
            unsafe fn load(lib: Library, path: PathBuf) -> Result<Self, String> {
                unsafe {
                    $(let $field: $ty = *lib.get::<$ty>(concat!($sym, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $sym))?;)*
                    Ok(Self { _lib: lib, path, $($field,)* })
                }
            }
        }
    };
}

api! {
    client_api_version: "mpv_client_api_version" => unsafe extern "C" fn() -> u64,
    error_string: "mpv_error_string" => unsafe extern "C" fn(c_int) -> *const c_char,
    free: "mpv_free" => unsafe extern "C" fn(*mut c_void),
    create: "mpv_create" => unsafe extern "C" fn() -> *mut c_void,
    initialize: "mpv_initialize" => unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: "mpv_terminate_destroy" => unsafe extern "C" fn(*mut c_void),
    wakeup: "mpv_wakeup" => unsafe extern "C" fn(*mut c_void),
    set_option_string: "mpv_set_option_string" => unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property_string: "mpv_set_property_string" => unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property: "mpv_set_property" => unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    get_property: "mpv_get_property" => unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    get_property_string: "mpv_get_property_string" => unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    command: "mpv_command" => unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
    command_async: "mpv_command_async" => unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    observe_property: "mpv_observe_property" => unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    request_log_messages: "mpv_request_log_messages" => unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    wait_event: "mpv_wait_event" => unsafe extern "C" fn(*mut c_void, f64) -> *mut Event,
    stream_cb_add_ro: "mpv_stream_cb_add_ro" => unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, StreamOpen) -> c_int,
    render_context_create: "mpv_render_context_create" => unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    render_context_set_update_callback: "mpv_render_context_set_update_callback" => unsafe extern "C" fn(*mut c_void, Option<Cb>, *mut c_void),
    render_context_update: "mpv_render_context_update" => unsafe extern "C" fn(*mut c_void) -> u64,
    render_context_render: "mpv_render_context_render" => unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_context_report_swap: "mpv_render_context_report_swap" => unsafe extern "C" fn(*mut c_void),
    render_context_free: "mpv_render_context_free" => unsafe extern "C" fn(*mut c_void),
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn lib_name() -> &'static str {
    if cfg!(windows) {
        "libmpv-2.dll"
    } else if cfg!(target_os = "macos") {
        "libmpv.2.dylib"
    } else {
        "libmpv.so.2"
    }
}

pub fn rid() -> &'static str {
    if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "macos") {
        "osx-x64"
    } else {
        "linux-x64"
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(p) = std::env::var_os("MPV_LIB") {
        let p = PathBuf::from(p);
        list.push(if p.is_dir() { p.join(lib_name()) } else { p });
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        list.push(dir.join("runtimes").join(rid()).join("native").join(lib_name()));
        list.push(dir.join(lib_name()));
    }
    list.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtimes").join(rid()).join("native").join(lib_name()));
    list
}

/// The loaded libmpv, or why it could not be loaded.
pub fn api() -> Result<&'static Api, String> {
    API.get_or_init(|| {
        let mut errors = Vec::new();
        for path in candidates() {
            if !path.exists() {
                continue;
            }
            match unsafe { Library::new(&path) } {
                Ok(lib) => return unsafe { Api::load(lib, path) },
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        Err(if errors.is_empty() { format!("{} not found (run Native/natives.sh fetch)", lib_name()) } else { errors.join("; ") })
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub fn error_string(code: c_int) -> String {
    match api() {
        Ok(a) => unsafe { CStr::from_ptr((a.error_string)(code)).to_string_lossy().into_owned() },
        Err(_) => format!("error {code}"),
    }
}

/// mpv_create returns NULL unless numbers are formatted the C way.
pub fn set_c_numeric_locale() {
    unsafe extern "C" {
        fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
    }
    #[cfg(windows)]
    const LC_NUMERIC: c_int = 4;
    #[cfg(not(windows))]
    const LC_NUMERIC: c_int = 1;
    unsafe { setlocale(LC_NUMERIC, c"C".as_ptr()) };
}
