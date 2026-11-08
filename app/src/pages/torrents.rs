//! Torrents page: the session's torrents (refreshed once a second while the page is open) and a search by movie name
//! that adds results as downloads (so closing the player never stops them).

use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, DownloadRow, TorrentRow, TorrentsState};
use engine::models::{ContentType, MediaItem, MediaType, TorrentInfo};
use engine::search::Engine;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Default)]
struct State {
    rows: Vec<(torrent::Torrent, Option<engine::models::TrackedTorrent>)>,
    results: Vec<TorrentInfo>,
    engine: usize,
    seq: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static TICK: slint::Timer = slint::Timer::default();
}

fn ts<R>(f: impl FnOnce(&TorrentsState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<TorrentsState>())
}

pub fn init() {
    ts(|t| {
        let names: Vec<SharedString> = Engine::ALL.iter().map(|e| e.label().into()).collect();
        t.set_providers(ModelRc::from(Rc::new(VecModel::from(names))));
        t.set_empty_text("Search a movie by name".into());
        t.on_select_tab(|i| ts(|t| t.set_tab(i)));
        t.on_play(|i| {
            let Some((handle, rec)) = STATE.with(|s| s.borrow().rows.get(i as usize).cloned()) else { return };
            let url = rec.as_ref().map(|r| r.url.clone()).unwrap_or_else(|| format!("magnet:?xt=urn:btih:{}", handle.info_hash()));
            let title = rec.map(|r| r.title).unwrap_or_else(|| handle.name());
            crate::open::play(MediaItem { url, title, media_type: MediaType::Torrent, ..Default::default() });
        });
        t.on_toggle_pause(|i| {
            let Some((handle, _)) = STATE.with(|s| s.borrow().rows.get(i as usize).cloned()) else { return };
            spawn_ui(async move {
                let paused = handle.is_paused();
                let h = handle.clone();
                bg(async move {
                    if paused {
                        h.resume().await.ok()
                    } else {
                        h.pause().await.ok()
                    };
                })
                .await;
                refresh();
            });
        });
        t.on_remove(|i| {
            let Some((handle, _)) = STATE.with(|s| s.borrow().rows.get(i as usize).cloned()) else { return };
            spawn_ui(async move {
                crate::services::torrents::remove(handle, false).await;
                refresh();
            });
        });
        t.on_select_provider(|i| {
            STATE.with(|s| s.borrow_mut().engine = i as usize);
            ts(|t| t.set_provider(i));
            let q = ts(|t| t.get_query()).to_string();
            if !q.trim().is_empty() {
                search(q);
            }
        });
        t.on_search(|q| search(q.to_string()));
        t.on_result_play(|i| {
            if let Some(info) = STATE.with(|s| s.borrow().results.get(i as usize).cloned()) {
                crate::services::torrents::download(&info.url, &info.title);
                crate::open::play(MediaItem { url: info.url, title: info.title, media_type: MediaType::Torrent, ..Default::default() });
            }
        });
        t.on_result_download(|i| {
            if let Some(info) = STATE.with(|s| s.borrow().results.get(i as usize).cloned()) {
                crate::services::torrents::download(&info.url, &info.title);
            }
        });
    });
}

/// The page became visible / hidden: refresh once a second only while it shows.
pub fn set_visible(visible: bool) {
    if visible {
        refresh();
        TICK.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_secs(1), refresh));
    } else {
        TICK.with(|t| t.stop());
    }
}

fn refresh() {
    spawn_ui(async {
        let list = crate::services::torrents::list().await;
        let rows: Vec<DownloadRow> = list
            .iter()
            .map(|(t, rec)| {
                let s = t.status();
                let title = rec.as_ref().map(|r| r.title.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| t.name());
                let finished = s.state == torrent::State::Finished;
                let paused = s.state == torrent::State::Paused;
                let mut meta = vec![
                    format!("{:.0}%", s.progress * 100.0),
                    format!("{} of {}", torrent::readable_size(s.downloaded_bytes), torrent::readable_size(s.total_bytes)),
                ];
                if !finished && !paused {
                    meta.push(format!("{}/s", torrent::readable_size(s.download_rate as u64)));
                    meta.push(format!("{} peers", s.peers));
                    if let Some(eta) = s.eta {
                        let m = eta.as_secs() / 60;
                        meta.push(if m >= 60 { format!("{}h {}m left", m / 60, m % 60) } else { format!("{m}m left") });
                    }
                }
                if paused {
                    meta.push("paused".into());
                }
                if let Some(e) = &s.error {
                    meta.push(e.clone());
                }
                if let Some(r) = rec {
                    crate::services::torrents::record_progress(&r.url, s.progress * 100.0);
                }
                DownloadRow { title: title.into(), meta: meta.join("  ·  ").into(), progress: s.progress as f32, paused, finished }
            })
            .collect();
        STATE.with(|s| s.borrow_mut().rows = list);
        ts(|t| t.set_downloads(ModelRc::from(Rc::new(VecModel::from(rows)))));
    });
}

/// Searches the chosen provider for a movie name; the Stremio addons need an IMDb id, so TMDB finds it first.
fn search(query: String) {
    let query = query.trim().to_string();
    if query.is_empty() {
        return;
    }
    let (engine, seq) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.seq += 1;
        (Engine::ALL[s.engine.min(Engine::ALL.len() - 1)], s.seq)
    });
    ts(|t| {
        t.set_searching(true);
        t.set_results(ModelRc::default());
    });
    spawn_ui(async move {
        let q = query.clone();
        let results = bg(async move {
            let bare = engine::tmdb::Movie { title: q.clone(), ..Default::default() };
            let found = engine.search_movie(&bare, false).await;
            if !found.is_empty() {
                return found;
            }
            let Some(first) = engine::tmdb::search(ContentType::Movie, &q, None).await.into_iter().next() else { return found };
            match engine::tmdb::movie(&first).await {
                Some(m) => engine.search_movie(&m, false).await,
                None => found,
            }
        })
        .await;
        if STATE.with(|s| s.borrow().seq) != seq {
            return;
        }
        let rows: Vec<TorrentRow> = results
            .iter()
            .map(|r| TorrentRow {
                title: r.title.clone().into(),
                meta: [
                    r.quality.label().to_string(),
                    r.size.clone(),
                    if r.seeders.is_empty() { String::new() } else { format!("{} seeds", r.seeders) },
                ]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join("  ·  ")
                .into(),
                is_local: r.media_type == MediaType::Local,
            })
            .collect();
        STATE.with(|s| s.borrow_mut().results = results);
        ts(|t| {
            t.set_searching(false);
            t.set_empty_text(format!("Nothing found for “{query}” on {}", engine.label()).into());
            t.set_results(ModelRc::from(Rc::new(VecModel::from(rows))));
        });
    });
}
