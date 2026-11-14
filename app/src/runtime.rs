//! Threading: network and disk work runs on a tokio runtime; UI code runs on Slint's event loop and awaits that work
//! through [`bg`]. [`ui`] posts a closure from any thread to the UI thread.

use std::future::Future;
use std::sync::OnceLock;

static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn init() {
    limit_malloc_arenas();
    raise_open_file_limit();
    // network and disk work: a few workers are plenty (one per core only adds per-thread memory)
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("uflix-worker")
            .build()
            .expect("tokio runtime")
    });
}

/// glibc gives each thread that allocates its own arena (up to 8 × cores); two keep RSS ~20 MB lower with no
/// measurable cost here.
fn limit_malloc_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn mallopt(param: i32, value: i32) -> i32;
        }
        const M_ARENA_MAX: i32 = -8;
        unsafe { mallopt(M_ARENA_MAX, 2) };
    }
}

/// Desktop launchers start apps with a soft limit of 1024 open files. Torrent peers use one socket each, and with
/// several torrents they reach it; the NVIDIA EGL driver then can't create its per-frame sync fd and the window stops
/// drawing (or aborts). Raise the soft limit to the hard one, as torrent clients do.
fn raise_open_file_limit() {
    #[cfg(target_os = "linux")]
    {
        #[repr(C)]
        struct RLimit {
            cur: u64,
            max: u64,
        }
        unsafe extern "C" {
            fn getrlimit(resource: i32, rlim: *mut RLimit) -> i32;
            fn setrlimit(resource: i32, rlim: *const RLimit) -> i32;
        }
        const RLIMIT_NOFILE: i32 = 7;
        let mut lim = RLimit { cur: 0, max: 0 };
        unsafe {
            if getrlimit(RLIMIT_NOFILE, &mut lim) == 0 && lim.cur < lim.max {
                lim.cur = lim.max;
                setrlimit(RLIMIT_NOFILE, &lim);
            }
        }
    }
}

pub fn handle() -> tokio::runtime::Handle {
    RT.get().expect("runtime::init").handle().clone()
}

/// Runs `fut` on the tokio runtime and awaits its result from the UI thread.
pub async fn bg<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) -> T {
    handle().spawn(fut).await.expect("background task panicked")
}

/// Runs a blocking function on tokio's blocking pool.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    handle().spawn_blocking(f).await.expect("blocking task panicked")
}

/// Starts a future on the UI event loop (it may await `bg` work).
pub fn spawn_ui(fut: impl Future<Output = ()> + 'static) {
    let _ = slint::spawn_local(fut);
}

/// Runs `f` on the UI thread (from any thread).
pub fn ui(f: impl FnOnce() + Send + 'static) {
    let _ = slint::invoke_from_event_loop(f);
}

/// Waits on the UI thread without blocking it.
pub async fn sleep(ms: u64) {
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    slint::Timer::single_shot(std::time::Duration::from_millis(ms), move || {
        let _ = tx.send(());
    });
    let _ = rx.await;
}
