//! Opt-in diagnostics, off unless their environment variable is set.
//!
//! `UFLIX_MEMLOG=<seconds>` prints a line to stderr every that many seconds: the process's resident memory (split into
//! heap/anonymous and file-backed), the system's available memory, mpv's demuxer cache and the torrent session's
//! counters. It runs on its own thread so it keeps logging while the UI thread is busy or stuck.

use std::time::{Duration, Instant};

pub fn start() {
    let Some(secs) = std::env::var("UFLIX_MEMLOG").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|s| *s > 0) else {
        return;
    };
    let started = Instant::now();
    let _ = std::thread::Builder::new().name("uflix-memlog".into()).spawn(move || {
        loop {
            // memory first: if mpv's core is stuck, the mpv query below blocks but this part is already out
            eprint!("{}", line(started.elapsed()));
            match crate::player::demuxer_cache_bytes() {
                Some(b) => eprintln!(" mpv_cache={}MB", b / (1024 * 1024)),
                None => eprintln!(" mpv_cache=-"),
            }
            std::thread::sleep(Duration::from_secs(secs));
        }
    });
}

fn line(elapsed: Duration) -> String {
    let mb = |kb: Option<u64>| kb.map_or("?".to_string(), |k| format!("{}", k / 1024));
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut out = format!(
        "[memlog] t={}s rss={}MB (anon={} file={}) threads={} sys_avail={}MB",
        elapsed.as_secs(),
        mb(kb(&status, "VmRSS:")),
        mb(kb(&status, "RssAnon:")),
        mb(kb(&status, "RssFile:")),
        kb(&status, "Threads:").map_or("?".into(), |n| n.to_string()),
        mb(kb(&meminfo, "MemAvailable:")),
    );
    if let Some(t) = crate::services::torrents::stats() {
        out.push_str(&format!(
            " torrents={} peers={} fetched={}MB uploaded={}MB",
            t.torrents,
            t.live_peers,
            t.fetched_bytes / (1024 * 1024),
            t.uploaded_bytes / (1024 * 1024)
        ));
    }
    out
}

/// A "Key:   123 kB" (or plain number) field from a /proc file.
fn kb(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| l.strip_prefix(key)).and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())
}
