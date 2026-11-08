//! Serves Rust readers to mpv through mpv_stream_cb_add_ro as "uflix://<id>". The callbacks run on mpv's demuxer
//! thread: they never call back into libmpv and never panic across the FFI boundary.

use crate::ffi::{self, api};
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const PREFIX: &str = "uflix://";

/// A seekable byte source. `read` may block until data is available and returns 0 at the end or after `cancel`.
pub trait StreamSource: Send + Sync {
    /// Bytes read, 0 at the end, < 0 on error.
    fn read(&self, buf: &mut [u8]) -> i64;
    /// New absolute position, < 0 on error.
    fn seek(&self, position: u64) -> i64;
    fn size(&self) -> i64;
    /// Wakes a blocked read; later reads return 0. Called by mpv on stop.
    fn cancel(&self);
}

fn registry() -> &'static Mutex<HashMap<u64, Arc<dyn StreamSource>>> {
    static R: OnceLock<Mutex<HashMap<u64, Arc<dyn StreamSource>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn add(source: Arc<dyn StreamSource>) -> String {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    registry().lock().unwrap().insert(id, source);
    format!("{PREFIX}{id}")
}

pub(crate) fn remove(uri: &str) {
    if let Some(id) = parse(uri) {
        registry().lock().unwrap().remove(&id);
    }
}

fn parse(uri: &str) -> Option<u64> {
    uri.strip_prefix(PREFIX)?.parse().ok()
}

pub(crate) fn register(h: *mut c_void) -> Result<(), String> {
    let a = api()?;
    let r = unsafe { (a.stream_cb_add_ro)(h, c"uflix".as_ptr(), std::ptr::null_mut(), open) };
    if r < 0 { Err(format!("stream_cb_add_ro: {}", ffi::error_string(r))) } else { Ok(()) }
}

unsafe fn source<'a>(cookie: *mut c_void) -> &'a Arc<dyn StreamSource> {
    unsafe { &*(cookie as *const Arc<dyn StreamSource>) }
}

unsafe extern "C" fn open(_user: *mut c_void, uri: *mut c_char, info: *mut ffi::StreamCbInfo) -> c_int {
    std::panic::catch_unwind(|| unsafe {
        let uri = CStr::from_ptr(uri).to_string_lossy();
        let Some(src) = parse(&uri).and_then(|id| registry().lock().unwrap().get(&id).cloned()) else {
            return ffi::ERROR_LOADING_FAILED;
        };
        let info = &mut *info;
        info.cookie = Box::into_raw(Box::new(src)) as *mut c_void;
        info.read = Some(read);
        info.seek = Some(seek);
        info.size = Some(size);
        info.close = Some(close);
        info.cancel = Some(cancel);
        0
    })
    .unwrap_or(ffi::ERROR_LOADING_FAILED)
}

unsafe extern "C" fn read(cookie: *mut c_void, buf: *mut c_char, n: u64) -> i64 {
    std::panic::catch_unwind(|| unsafe {
        let slice = std::slice::from_raw_parts_mut(buf as *mut u8, n.min(i32::MAX as u64) as usize);
        source(cookie).read(slice)
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn seek(cookie: *mut c_void, offset: i64) -> i64 {
    std::panic::catch_unwind(|| unsafe { if offset < 0 { -1 } else { source(cookie).seek(offset as u64) } })
        .unwrap_or(ffi::ERROR_GENERIC as i64)
}

unsafe extern "C" fn size(cookie: *mut c_void) -> i64 {
    std::panic::catch_unwind(|| unsafe { source(cookie).size() }).unwrap_or(ffi::ERROR_UNSUPPORTED as i64)
}

unsafe extern "C" fn close(cookie: *mut c_void) {
    let _ = std::panic::catch_unwind(|| unsafe { drop(Box::from_raw(cookie as *mut Arc<dyn StreamSource>)) });
}

unsafe extern "C" fn cancel(cookie: *mut c_void) {
    let _ = std::panic::catch_unwind(|| unsafe { source(cookie).cancel() });
}
