//! Keeps the torrent folder under the Settings size cap by deleting the oldest downloads first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

struct Entry {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    is_dir: bool,
}

fn dir_stats(path: &Path) -> (u64, SystemTime) {
    let mut size = 0;
    let mut newest = SystemTime::UNIX_EPOCH;
    let Ok(rd) = std::fs::read_dir(path) else { return (0, newest) };
    for e in rd.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            let (s, m) = dir_stats(&e.path());
            size += s;
            newest = newest.max(m);
        } else {
            size += meta.len();
            newest = newest.max(meta.modified().unwrap_or(newest));
        }
    }
    (size, newest)
}

fn list(path: &Path, out: &mut Vec<Entry>) {
    let Ok(rd) = std::fs::read_dir(path) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(meta) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if name == "_cache" {
            continue;
        }
        if meta.is_dir() {
            if name.ends_with("_incomplete") {
                list(&p, out);
                continue;
            }
            let (size, modified) = dir_stats(&p);
            out.push(Entry { path: p, size, modified, is_dir: true });
        } else {
            out.push(Entry { path: p, size: meta.len(), modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), is_dir: false });
        }
    }
}

/// Deletes the oldest entries of the torrent folder until it fits the cap (no-op without a cap).
pub fn enforce_size_cap() {
    let folder = crate::paths::torrent_folder();
    let _ = std::fs::create_dir_all(&folder);
    let cap_gb = crate::settings::max_folder_size();
    if cap_gb <= 0 {
        return;
    }
    let cap = cap_gb as u64 * 1024 * 1024 * 1024;
    let mut entries = Vec::new();
    list(&folder, &mut entries);
    let mut total: u64 = entries.iter().map(|e| e.size).sum();
    entries.sort_by_key(|e| e.modified);
    for e in entries {
        if total <= cap {
            break;
        }
        let removed = if e.is_dir { std::fs::remove_dir_all(&e.path) } else { std::fs::remove_file(&e.path) };
        if removed.is_ok() {
            total = total.saturating_sub(e.size);
        }
    }
}
