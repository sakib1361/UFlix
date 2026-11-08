//! Disk cache for remote images (posters, thumbnails): `<data>/Cache/<md5 of url>`. Concurrent requests for the same
//! URL share one download. Files older than 7 days are cleared at start-up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;

fn in_flight() -> &'static Mutex<HashMap<String, Arc<Notify>>> {
    static M: OnceLock<Mutex<HashMap<String, Arc<Notify>>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

fn file_for(url: &str) -> PathBuf {
    crate::paths::cache_folder().join(format!("{:x}", md5::compute(url.as_bytes())))
}

/// The local file for `url`, downloading it first if needed. None for empty URLs or failed downloads.
pub async fn get(url: &str) -> Option<PathBuf> {
    if url.is_empty() {
        return None;
    }
    if !url.starts_with("http") {
        let p = PathBuf::from(url.strip_prefix("file://").unwrap_or(url));
        return p.is_file().then_some(p);
    }
    let path = file_for(url);
    loop {
        if path.is_file() {
            return Some(path);
        }
        let (notify, leader) = {
            let mut map = in_flight().lock().unwrap();
            match map.get(url) {
                Some(n) => (n.clone(), false),
                None => {
                    let n = Arc::new(Notify::new());
                    map.insert(url.to_string(), n.clone());
                    (n, true)
                }
            }
        };
        if !leader {
            let wait = notify.notified();
            if !in_flight().lock().unwrap().contains_key(url) {
                continue;
            }
            wait.await;
            if path.is_file() {
                return Some(path);
            }
            return None;
        }
        let result = download(url, &path).await;
        in_flight().lock().unwrap().remove(url);
        notify.notify_waiters();
        return result.then_some(path);
    }
}

async fn download(url: &str, path: &PathBuf) -> bool {
    let Ok(bytes) = crate::http::get_bytes(url).await else { return false };
    if bytes.is_empty() {
        return false;
    }
    let _ = tokio::fs::create_dir_all(crate::paths::cache_folder()).await;
    let tmp = path.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await.is_ok() && tokio::fs::rename(&tmp, path).await.is_ok()
}

/// Removes cached images older than a week.
pub fn clear_old() {
    let Ok(dir) = std::fs::read_dir(crate::paths::cache_folder()) else { return };
    let week = std::time::Duration::from_secs(7 * 86_400);
    for entry in dir.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > week);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
