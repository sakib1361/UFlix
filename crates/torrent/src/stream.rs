//! A synchronous, cancellable view of librqbit's async file stream, for readers that run on their own thread (mpv's
//! demuxer calls read/seek through stream_cb and must be able to abort a blocked read on stop).

use std::io::SeekFrom;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio::sync::Notify;

/// librqbit's file stream (its type isn't exported by name).
pub(crate) trait Source: AsyncRead + AsyncSeek + Unpin + Send {}
impl<T: AsyncRead + AsyncSeek + Unpin + Send> Source for T {}

pub struct BlockingStream {
    inner: Mutex<Box<dyn Source>>,
    len: u64,
    rt: tokio::runtime::Handle,
    cancelled: AtomicBool,
    wake: Notify,
}

impl BlockingStream {
    pub(crate) fn new(inner: Box<dyn Source>, len: u64, rt: tokio::runtime::Handle) -> Self {
        Self { inner: Mutex::new(inner), len, rt, cancelled: AtomicBool::new(false), wake: Notify::new() }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Bytes read (blocking until data arrives); 0 at the end or once cancelled; < 0 on error.
    /// Must not be called from inside the tokio runtime.
    pub fn read(&self, buf: &mut [u8]) -> i64 {
        if buf.is_empty() || self.cancelled.load(Ordering::Acquire) {
            return 0;
        }
        let mut inner = self.inner.lock().unwrap();
        self.rt.block_on(async {
            let cancelled = self.wake.notified();
            tokio::pin!(cancelled);
            cancelled.as_mut().enable();
            if self.cancelled.load(Ordering::Acquire) {
                return 0;
            }
            tokio::select! {
                r = inner.read(buf) => match r { Ok(n) => n as i64, Err(_) => -1 },
                _ = cancelled => 0,
            }
        })
    }

    pub fn seek(&self, position: u64) -> i64 {
        if self.cancelled.load(Ordering::Acquire) {
            return -1;
        }
        let mut inner = self.inner.lock().unwrap();
        self.rt.block_on(inner.seek(SeekFrom::Start(position.min(self.len)))).map_or(-1, |p| p as i64)
    }

    /// Wakes a blocked read; later reads return 0. Safe from any thread.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.wake.notify_waiters();
    }
}
