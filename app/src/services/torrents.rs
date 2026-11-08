//! The app-wide torrent session. Each torrent is tracked as Play (streamed for the player; removed when the player
//! closes before it completes, files kept for the size cleaner) or Download (kept until complete). Tracked records are
//! saved so unfinished downloads come back at the next start.

use crate::runtime::{bg, handle};
use engine::models::{TorrentKind, TrackedTorrent};
use std::sync::Arc;
use tokio::sync::OnceCell;
use torrent::{Engine, Torrent};

static ENGINE: OnceCell<Result<Arc<Engine>, String>> = OnceCell::const_new();

async fn engine() -> Result<Arc<Engine>, String> {
    ENGINE
        .get_or_init(|| async {
            let folder = engine::paths::torrent_folder();
            Engine::start(&folder, &engine::paths::torrent_state_folder()).await.map(Arc::new)
        })
        .await
        .clone()
}

/// Re-adds unfinished tracked downloads in the background (completed records are dropped). The session itself starts
/// only when there is something to download or play.
pub fn start() {
    let mut tracked = engine::settings::tracked_torrents();
    tracked.retain(|t| t.progress < 100.0 && t.kind == TorrentKind::Download);
    engine::settings::set_tracked_torrents(tracked.clone());
    if tracked.is_empty() {
        return;
    }
    handle().spawn(async move {
        let Ok(e) = engine().await else { return };
        for t in tracked {
            let _ = e.add(&t.url, std::path::Path::new(&t.save_path), None).await;
        }
    });
}

fn track(url: &str, title: &str, kind: TorrentKind) {
    let mut list = engine::settings::tracked_torrents();
    match list.iter_mut().find(|t| t.url == url) {
        Some(t) => {
            // a Download is never demoted to Play
            if kind == TorrentKind::Download {
                t.kind = kind;
            }
        }
        None => list.push(TrackedTorrent {
            url: url.to_string(),
            title: title.to_string(),
            kind,
            progress: 0.0,
            save_path: engine::paths::torrent_folder().to_string_lossy().into_owned(),
            added: Some(chrono::Utc::now()),
        }),
    }
    engine::settings::set_tracked_torrents(list);
}

fn kind_of(url: &str) -> Option<TorrentKind> {
    engine::settings::tracked_torrents().into_iter().find(|t| t.url == url).map(|t| t.kind)
}

/// Adds (or finds) a torrent for the player; resolves once its file list is known.
pub async fn open(url: String, title: String) -> Result<Torrent, String> {
    bg(async move {
        let e = engine().await?;
        let t = e.add(&url, &engine::paths::torrent_folder(), None).await?;
        track(&url, &title, TorrentKind::Play);
        Ok(t)
    })
    .await
}

/// Called when the player leaves a torrent: an unfinished Play torrent is removed (its files stay on disk).
pub fn release(url: &str, torrent: &Torrent) {
    if kind_of(url) == Some(TorrentKind::Download) || torrent.is_finished() {
        return;
    }
    let (url, t) = (url.to_string(), torrent.clone());
    handle().spawn(async move {
        if let Ok(e) = engine().await {
            let _ = e.remove(&t, false).await;
        }
        let mut list = engine::settings::tracked_torrents();
        list.retain(|x| x.url != url);
        engine::settings::set_tracked_torrents(list);
    });
}

/// Downloads a whole torrent (or promotes the one playing to a download).
pub fn download(url: &str, title: &str) {
    track(url, title, TorrentKind::Download);
    let url = url.to_string();
    let title = title.to_string();
    crate::ui::toast(format!("Downloading {title}"));
    handle().spawn(async move {
        let result = async {
            let e = engine().await?;
            let t = e.add(&url, &engine::paths::torrent_folder(), None).await?;
            let all: Vec<usize> = t.files().iter().map(|f| f.index).collect();
            t.select_files(&all).await?;
            t.resume().await.ok();
            Ok::<_, String>(())
        }
        .await;
        if let Err(e) = result {
            crate::ui::toast_from_any(format!("Download failed: {e}"));
        }
    });
}

/// Every torrent in the session with its tracked record (Torrents page).
pub async fn list() -> Vec<(Torrent, Option<TrackedTorrent>)> {
    bg(async {
        let Ok(e) = engine().await else { return Vec::new() };
        let tracked = engine::settings::tracked_torrents();
        e.torrents()
            .into_iter()
            .map(|t| {
                let hash = t.info_hash();
                let rec =
                    tracked.iter().find(|r| torrent::magnet_info_hash(&r.url).is_some_and(|h| h.eq_ignore_ascii_case(&hash))).cloned();
                (t, rec)
            })
            .collect()
    })
    .await
}

pub async fn remove(t: Torrent, delete_files: bool) {
    bg(async move {
        if let Ok(e) = engine().await {
            let hash = t.info_hash();
            let _ = e.remove(&t, delete_files).await;
            let mut list = engine::settings::tracked_torrents();
            list.retain(|r| !torrent::magnet_info_hash(&r.url).is_some_and(|h| h.eq_ignore_ascii_case(&hash)));
            engine::settings::set_tracked_torrents(list);
        }
    })
    .await
}

/// Saves the progress of tracked downloads (whole percents).
pub fn record_progress(url: &str, progress: f64) {
    let mut list = engine::settings::tracked_torrents();
    if let Some(t) = list.iter_mut().find(|t| t.url == url)
        && (t.progress.floor() - progress.floor()).abs() >= 1.0
    {
        t.progress = progress;
        engine::settings::set_tracked_torrents(list);
    }
}

/// Stops the session (saves fast-resume state). Call at exit.
pub fn shutdown() {
    if let Some(Ok(e)) = ENGINE.get() {
        let e = e.clone();
        let _ = handle().block_on(async move { tokio::time::timeout(std::time::Duration::from_secs(3), e.stop()).await });
    }
}
