//! What's playing: the current item, its queue (playlist, YouTube "up next" or a torrent's files), opening each source,
//! mpv events, menus, the info panel and saving the position to the watchlist.

use super::video::{TorrentSource, player};
use crate::cards::CardList;
use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, MenuItem, PlayerState, Shell, Stat};
use engine::models::{ContentType, MediaItem, MediaType};
use engine::youtube::{self, StreamSet};
use mpv::{EndReason, Event, LoadOptions, Track, TrackType};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Queue {
    None,
    Playlist,
    Related,
    Files,
}

struct TorrentPlay {
    url: String,
    handle: torrent::Torrent,
    files: Vec<torrent::TorrentFile>,
    index: usize,
}

#[derive(Clone)]
enum MenuAction {
    SubOff,
    SubTrack(i64),
    Caption(String),
    SearchSubs,
    Audio(i64),
    Quality(usize),
}

#[derive(Default)]
struct Session {
    item: MediaItem,
    queue_kind: Option<Queue>,
    queue: Vec<MediaItem>,
    index: usize,
    list_cards: Option<CardList>,
    torrent: Option<TorrentPlay>,
    streams: Option<StreamSet>,
    quality: usize,
    tracks: Vec<Track>,
    menu: Vec<MenuAction>,
    seq: u64,
    loaded: bool,
    position: f64,
    duration: f64,
    last_saved: f64,
    sub_delay: f64,
    volume: f64,
    muted: bool,
}

thread_local! {
    static S: RefCell<Option<Session>> = const { RefCell::new(None) };
    static HIDE: slint::Timer = slint::Timer::default();
    static TICK: slint::Timer = slint::Timer::default();
}

fn with<R>(f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    S.with(|s| s.borrow_mut().as_mut().map(f))
}

fn ps<R>(f: impl FnOnce(&PlayerState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<PlayerState>())
}

fn strings(items: &[MenuItem]) -> ModelRc<MenuItem> {
    ModelRc::from(Rc::new(VecModel::from(items.to_vec())))
}

fn mpv_do(f: impl FnOnce(&mpv::Player)) {
    if let Ok(p) = player() {
        f(&p);
    }
}

// ---------------------------------------------------------------- opening

/// Opens the player page on `item` (a file, link, torrent, YouTube video or playlist, or channel).
pub fn open(mut item: MediaItem) {
    if let Err(e) = player() {
        ui::toast(format!("Player unavailable: {e}"));
        return;
    }
    if item.position <= 0.0
        && let Some(p) = engine::watchlist::position_of(&item)
    {
        item.position = p;
    }
    let old = S.with(|s| s.borrow_mut().take());
    if let Some(old) = old {
        leave(old);
    }
    let volume = super::video::initial_volume();
    S.with(|s| *s.borrow_mut() = Some(Session { volume, ..Default::default() }));
    let picture = engine::settings::picture();
    ps(|p| {
        p.set_open(true);
        p.set_menu("".into());
        p.set_info_open(false);
        p.set_theater(false);
        p.set_side_available(false);
        p.set_details_title("".into());
        p.set_details_channel("".into());
        p.set_details_stats("".into());
        p.set_details_description("".into());
        p.set_description_expanded(false);
        p.set_list(ModelRc::default());
        p.set_has_prev(false);
        p.set_has_next(false);
        p.set_autoplay(engine::settings::autoplay());
        p.set_volume(volume as f32);
        p.set_muted(false);
        p.set_speed(1.0);
        p.set_brightness(picture.brightness as f32);
        p.set_contrast(picture.contrast as f32);
        p.set_saturation(picture.saturation as f32);
        p.set_gamma(picture.gamma as f32);
    });
    apply_picture(&picture);
    mpv_do(|p| {
        p.set_speed(1.0);
        p.set_property("video-margin-ratio-right", "0");
    });
    super::video::set_active(true);
    crate::services::keep_awake::set(false);
    show_controls();
    set_loading(true, "Opening…");
    ps(|p| p.set_title(item.title.clone().into()));
    spawn_ui(async move {
        super::video::ready().await;
        match item.media_type {
            MediaType::PlayList => open_playlist(item),
            MediaType::Youtube => {
                with(|s| s.queue_kind = Some(Queue::Related));
                load_related(item.clone());
                play_item(item);
            }
            _ => play_item(item),
        }
    });
}

fn open_playlist(item: MediaItem) {
    with(|s| {
        s.item = item.clone();
        s.queue_kind = Some(Queue::Playlist);
    });
    set_loading(true, "Loading playlist…");
    ps(|p| p.set_title(item.title.clone().into()));
    spawn_ui(async move {
        let id = item.url.clone();
        let items = bg(async move { youtube::playlist_items(&id, true).await }).await.unwrap_or_default();
        if items.is_empty() {
            ui::toast("Couldn't load the playlist");
            close();
            return;
        }
        let index = item.playlist_index.min(items.len() - 1);
        let mut first = items[index].clone();
        first.position = item.position;
        set_queue(Queue::Playlist, items, index, &item.title);
        play_item(first);
    });
}

/// "Up next" for a YouTube video: YouTube's recommendations, with the video itself first.
fn load_related(item: MediaItem) {
    let id = item.url.clone();
    spawn_ui(async move {
        let details = bg(async move { youtube::details(&id).await }).await;
        let Ok(d) = details else { return };
        let current = with(|s| s.item.url.clone()).unwrap_or_default();
        if current != item.url && !with(|s| s.queue.is_empty()).unwrap_or(true) {
            return;
        }
        let mut list = vec![item.clone()];
        list.extend(d.related.iter().map(|r| r.to_media()));
        set_queue(Queue::Related, list, 0, "Up next");
    });
}

fn set_queue(kind: Queue, items: Vec<MediaItem>, index: usize, title: &str) {
    let cards = CardList::new(124);
    let specs = items
        .iter()
        .enumerate()
        .map(|(i, m)| crate::cards::CardSpec {
            title: m.title.clone(),
            subtitle: m.author.clone(),
            image_url: if m.image.is_empty() && kind != Queue::Files { youtube::thumb(&m.url, "mqdefault") } else { m.image.clone() },
            badge: match kind {
                Queue::Files => format!("{}", i + 1),
                _ => m.duration.map(|d| youtube::format_duration(Some(d as u32))).unwrap_or_default(),
            },
            ..Default::default()
        })
        .collect();
    cards.push_all(specs);
    with(|s| {
        s.queue_kind = Some(kind);
        s.queue = items;
        s.index = index;
        s.list_cards = Some(cards.clone());
    });
    refresh_list(title);
}

/// Shows the queue in the side panel and marks the current entry.
fn refresh_list(title: &str) {
    let Some(Some((cards, index, kind, len))) =
        with(|s| s.list_cards.clone().map(|c| (c, s.index, s.queue_kind.unwrap_or(Queue::None), s.queue.len())))
    else {
        return;
    };
    let title = title.to_string();
    ps(|p| {
        p.set_list(cards.model());
        p.set_list_index(index as i32);
        p.set_list_title(title.into());
        p.set_side_available(kind != Queue::None && len > 0 && (kind != Queue::Files || len > 1));
        p.set_show_autoplay(matches!(kind, Queue::Related | Queue::Playlist));
        p.set_has_next(index + 1 < len);
        p.set_has_prev(index > 0 && kind != Queue::Related);
    });
    update_video_margin();
}

fn set_loading(loading: bool, text: &str) {
    let text = text.to_string();
    ps(|p| {
        p.set_loading(loading);
        p.set_loading_text(text.into());
        if !loading {
            p.set_loading_progress(-1.0);
            p.set_torrent_stats("".into());
        }
    });
}

/// Loads `item` into mpv according to its source.
fn play_item(item: MediaItem) {
    let seq = with(|s| {
        s.seq += 1;
        s.item = item.clone();
        s.loaded = false;
        s.position = item.position;
        s.duration = item.duration.unwrap_or(0.0);
        s.last_saved = 0.0;
        s.streams = None;
        s.tracks.clear();
        s.sub_delay = 0.0;
        s.seq
    })
    .unwrap_or(0);
    let live = item.media_type == MediaType::Unknown || item.url.contains(".m3u8");
    ps(|p| {
        p.set_title(item.title.clone().into());
        p.set_subtitle(subtitle_for(&item).into());
        p.set_position(0.0);
        p.set_duration(item.duration.unwrap_or(0.0) as f32);
        p.set_buffered(0.0);
        p.set_live(live);
        p.set_has_quality_menu(false);
        p.set_has_audio_menu(false);
        p.set_sub_delay_text("0.0 s".into());
    });
    mpv_do(|p| p.set_sub_delay(0.0));
    set_loading(true, "Opening…");
    if item.media_type == MediaType::Youtube {
        show_details(&item);
    }
    let start = (!live && item.position > 5.0).then_some(item.position);
    let title = Some(item.title.clone());
    match item.media_type {
        MediaType::Youtube => spawn_ui(async move {
            let id = item.url.clone();
            let ytdlp_first = engine::settings::use_ytdl_first();
            // one source first (Settings → "yt-dlp first"), the other as the fallback
            let set = bg(async move {
                let first = if ytdlp_first { youtube::streams_ytdlp(&id).await } else { youtube::streams(&id).await };
                match first {
                    Ok(s) => Ok(s),
                    Err(e1) => {
                        let other = if ytdlp_first { youtube::streams(&id).await } else { youtube::streams_ytdlp(&id).await };
                        other.map_err(|e2| format!("{e1}; {e2}"))
                    }
                }
            })
            .await;
            if with(|s| s.seq) != Some(seq) {
                return;
            }
            match set {
                Ok(set) => {
                    let height = engine::settings::youtube_height();
                    let index = set.options.iter().position(|o| height == 0 || o.height <= height).unwrap_or(0);
                    let chosen = set.options[index].clone();
                    with(|s| {
                        s.quality = index;
                        s.streams = Some(set);
                    });
                    ps(|p| p.set_has_quality_menu(true));
                    load(seq, chosen.url, LoadOptions { audio_url: chosen.audio_url, start, title, ..Default::default() });
                }
                Err(e) => fail(seq, &format!("YouTube: {e}")),
            }
        }),
        MediaType::Torrent => open_torrent(seq, item, start),
        MediaType::Local => {
            let subs = engine::subtitles::finder::find(std::path::Path::new(&item.url), Some(&item.title));
            let subtitles = subs.into_iter().map(|p| p.to_string_lossy().into_owned()).collect();
            load(seq, item.url.clone(), LoadOptions { start, title, subtitles, ..Default::default() });
        }
        _ => spawn_ui(async move {
            let url = item.url.clone();
            let subtitles = if live { Vec::new() } else { bg(async move { web_subtitles(&url).await }).await };
            if with(|s| s.seq) == Some(seq) {
                load(seq, item.url.clone(), LoadOptions { start, title, subtitles, ..Default::default() });
            }
        }),
    }
}

fn subtitle_for(item: &MediaItem) -> String {
    match item.content_type {
        ContentType::Youtube => item.author.clone(),
        _ => [item.year.clone(), item.author.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
    }
}

/// Side-by-side subtitles of a web video (`movie.srt`, `movie.en.srt`), downloaded to the temp folder.
async fn web_subtitles(url: &str) -> Vec<String> {
    let Some(dot) = url.rfind('.').filter(|d| url[*d..].len() <= 5 && !url[*d..].contains('/')) else { return Vec::new() };
    let base = &url[..dot];
    let mut out = Vec::new();
    for candidate in [format!("{base}.srt"), format!("{base}.en.srt")] {
        if let Ok(text) = engine::http::get_text(&candidate).await
            && text.contains("-->")
        {
            let file = engine::paths::temp_folder().join(format!("web-{:x}.srt", candidate.len() ^ text.len()));
            let _ = std::fs::create_dir_all(engine::paths::temp_folder());
            if std::fs::write(&file, text).is_ok() {
                out.push(file.to_string_lossy().into_owned());
            }
        }
    }
    out
}

fn load(seq: u64, url: String, options: LoadOptions) {
    let Ok(p) = player() else { return };
    p.load(&url, options, move |r| crate::runtime::ui(move || on_loaded(seq, r)));
}

fn fail(seq: u64, message: &str) {
    if with(|s| s.seq) != Some(seq) {
        return;
    }
    ui::toast(message.to_string());
    let has_queue = with(|s| s.index + 1 < s.queue.len()).unwrap_or(false);
    if has_queue { next() } else { close() }
}

fn on_loaded(seq: u64, result: Result<(), String>) {
    if with(|s| s.seq) != Some(seq) {
        return;
    }
    if let Err(e) = result {
        if e != "replaced" && e != "stopped" && e != "shutdown" {
            fail(seq, &format!("Couldn't play: {e}"));
        }
        return;
    }
    set_loading(false, "");
    let (volume, muted, start, item) = with(|s| {
        s.loaded = true;
        (s.volume, s.muted, s.position, s.item.clone())
    })
    .unwrap_or((100.0, false, 0.0, MediaItem::default()));
    mpv_do(|p| {
        p.set_volume(if muted { 0.0 } else { volume });
        let duration = p.get_double("duration").unwrap_or(0.0);
        if start > 0.0 && duration > 0.0 && duration - start < 10.0 {
            p.seek(0.0, false);
        }
        p.play();
    });
    engine::watchlist::add_or_update(&item);
    crate::services::keep_awake::set(true);
    show_controls();
}

// ---------------------------------------------------------------- torrents

fn open_torrent(seq: u64, item: MediaItem, start: Option<f64>) {
    set_loading(true, "Connecting to peers…");
    spawn_ui(async move {
        let handle = match crate::services::torrents::open(item.url.clone(), item.title.clone()).await {
            Ok(h) => h,
            Err(e) => return fail(seq, &format!("Torrent: {e}")),
        };
        if with(|s| s.seq) != Some(seq) {
            return;
        }
        let files = torrent::movies_sorted(&handle.files());
        if files.is_empty() {
            return fail(seq, "The torrent has no video files");
        }
        let index = pick_file(&item, &files);
        with(|s| {
            s.torrent = Some(TorrentPlay { url: item.url.clone(), handle: handle.clone(), files: files.clone(), index });
        });
        if files.len() > 1 {
            let items: Vec<MediaItem> = files
                .iter()
                .map(|f| MediaItem { title: f.name().to_string(), author: torrent::readable_size(f.size), ..Default::default() })
                .collect();
            set_queue(Queue::Files, items, index, "Files in this torrent");
        }
        play_torrent_file(seq, index, start);
    });
}

/// The file to play: the one played before, the matching episode of a show, else the largest video.
fn pick_file(item: &MediaItem, files: &[torrent::TorrentFile]) -> usize {
    if let Some(path) = &item.torrent_file
        && let Some(i) = files.iter().position(|f| &f.path == path)
    {
        return i;
    }
    if item.content_type == ContentType::Shows {
        let want = engine::parse::parse_name(&item.title);
        if let Some(ep) = want.episode
            && let Some(i) = files.iter().position(|f| {
                let p = engine::parse::parse_name(f.name());
                p.episode == Some(ep) && (p.season.is_none() || want.season.is_none() || p.season == want.season)
            })
        {
            return i;
        }
        return 0;
    }
    files.iter().enumerate().max_by_key(|(_, f)| f.size).map(|(i, _)| i).unwrap_or(0)
}

fn play_torrent_file(seq: u64, index: usize, start: Option<f64>) {
    let Some(Some((handle, file, all, title))) = with(|s| {
        let t = s.torrent.as_mut()?;
        t.index = index;
        s.index = index;
        s.item.torrent_file = Some(t.files[index].path.clone());
        Some((t.handle.clone(), t.files[index].clone(), t.handle.files(), s.item.title.clone()))
    }) else {
        return;
    };
    if with(|s| s.queue_kind == Some(Queue::Files)).unwrap_or(false) {
        refresh_list("Files in this torrent");
    }
    // the video and its subtitles (same rules as for files on disk)
    let paths: Vec<String> = all.iter().map(|f| f.path.clone()).collect();
    let subs = engine::subtitles::finder::select(&file.path, &paths, Some(&title));
    let sub_files: Vec<torrent::TorrentFile> = all.iter().filter(|f| subs.contains(&f.path)).take(20).cloned().collect();
    set_loading(true, "Buffering…");
    start_torrent_stats(seq);
    spawn_ui(async move {
        let h = handle.clone();
        let mut wanted: Vec<usize> = vec![file.index];
        wanted.extend(sub_files.iter().map(|f| f.index));
        let sub_indexes: Vec<usize> = sub_files.iter().map(|f| f.index).collect();
        let f = file.clone();
        let result = bg(async move {
            h.select_files(&wanted).await?;
            h.resume().await.ok();
            // subtitles are tiny: wait (briefly) for them first
            for _ in 0..40 {
                let st = h.status();
                let done = sub_indexes.iter().all(|i| {
                    st.file_progress.get(*i).copied().unwrap_or(0) >= h.files().iter().find(|x| x.index == *i).map_or(0, |x| x.size)
                });
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            h.stream(&f).await
        })
        .await;
        if with(|s| s.seq) != Some(seq) {
            return;
        }
        let stream = match result {
            Ok(s) => s,
            Err(e) => return fail(seq, &format!("Torrent: {e}")),
        };
        let folder = handle.output_folder();
        let subtitles = sub_files.iter().map(|f| folder.join(&f.path).to_string_lossy().into_owned()).collect();
        let Ok(p) = player() else { return };
        let source: Arc<dyn mpv::StreamSource> = Arc::new(TorrentSource(stream));
        p.load_stream(source, LoadOptions { start, title: Some(file.name().to_string()), subtitles, ..Default::default() }, move |r| {
            crate::runtime::ui(move || on_loaded(seq, r))
        });
    });
}

/// Torrent stats in the loading card and the info panel, once a second while a torrent plays.
fn start_torrent_stats(seq: u64) {
    TICK.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            if with(|s| s.seq) != Some(seq) {
                return;
            }
            refresh_stats();
        })
    });
}

fn torrent_line() -> Option<String> {
    with(|s| {
        let t = s.torrent.as_ref()?;
        let st = t.handle.status();
        let file = &t.files[t.index];
        let done = st.file_progress.get(file.index).copied().unwrap_or(0);
        Some(format!(
            "{} peers · ↓ {}/s · ↑ {}/s · {} of {}",
            st.peers,
            torrent::readable_size(st.download_rate as u64),
            torrent::readable_size(st.upload_rate as u64),
            torrent::readable_size(done),
            torrent::readable_size(file.size)
        ))
    })
    .flatten()
}

// ---------------------------------------------------------------- events

pub fn on_event(e: Event) {
    if S.with(|s| s.borrow().is_none()) {
        return;
    }
    match e {
        Event::Position(p) => {
            let save = with(|s| {
                s.position = p;
                s.item.position = p;
                (p - s.last_saved).abs() > 15.0
            })
            .unwrap_or(false);
            ps(|st| st.set_position(p as f32));
            mpv_do(|m| {
                if let Some(c) = m.get_double("demuxer-cache-time") {
                    ps(|st| st.set_buffered(c as f32));
                }
            });
            if save {
                save_position();
            }
        }
        Event::Duration(d) => {
            with(|s| {
                s.duration = d;
                s.item.duration = Some(d);
            });
            ps(|st| st.set_duration(d as f32));
        }
        Event::Pause(paused) => {
            ps(|st| st.set_paused(paused));
            crate::services::keep_awake::set(!paused);
            if paused {
                show_controls();
                save_position();
            }
        }
        Event::Buffering(b) => {
            let loaded = with(|s| s.loaded).unwrap_or(false);
            if loaded {
                ps(|st| {
                    st.set_loading(b);
                    st.set_loading_text(if b { "Buffering…".into() } else { "".into() });
                });
            }
        }
        Event::Volume(v) => {
            if !with(|s| s.muted).unwrap_or(false) {
                ps(|st| st.set_volume(v as f32));
            }
        }
        Event::Tracks(tracks) => {
            let audio = tracks.iter().filter(|t| t.kind == TrackType::Audio).count();
            with(|s| s.tracks = tracks);
            ps(|st| st.set_has_audio_menu(audio > 1));
            if let Some(menu) = Some(ps(|st| st.get_menu())).filter(|m| m == "subs" || m == "audio") {
                open_menu(&menu);
            }
        }
        Event::Ended(EndReason::Eof, _) => on_finished(),
        Event::Ended(EndReason::Error, e) => {
            let loaded = with(|s| s.loaded).unwrap_or(false);
            if loaded {
                ui::toast(format!("Playback error: {e}"));
            }
        }
        _ => {}
    }
}

fn on_finished() {
    let Some((kind, has_next, item)) = with(|s| (s.queue_kind, s.index + 1 < s.queue.len(), s.item.clone())) else { return };
    save_position();
    let autoplay = ps(|p| p.get_autoplay());
    match kind {
        Some(Queue::Related) | Some(Queue::Playlist) if !autoplay => {}
        _ if has_next => next(),
        _ if item.content_type == ContentType::Shows => {
            // the torrent has no more episodes: pick the next one in the show dialog
            close();
            crate::open::open_media(item);
        }
        _ => close(),
    }
}

fn save_position() {
    let item = with(|s| {
        s.last_saved = s.position;
        s.loaded.then(|| s.item.clone())
    })
    .flatten();
    if let Some(item) = item {
        let mut entry = item;
        if entry.duration.is_some_and(|d| d > 0.0 && entry.position > d - 30.0) {
            entry.position = 0.0; // finished: start over next time
        }
        engine::watchlist::add_or_update(&entry);
    }
}

// ---------------------------------------------------------------- transport

pub fn next() {
    let Some((kind, index, len)) = with(|s| (s.queue_kind, s.index, s.queue.len())) else { return };
    if index + 1 >= len {
        return;
    }
    select(kind, index + 1);
}

pub fn prev() {
    let Some((kind, index)) = with(|s| (s.queue_kind, s.index)) else { return };
    if index > 0 {
        select(kind, index - 1);
    }
}

fn select(kind: Option<Queue>, index: usize) {
    save_position();
    match kind {
        Some(Queue::Files) => {
            let seq = with(|s| {
                s.seq += 1;
                s.loaded = false;
                s.position = 0.0;
                s.item.position = 0.0;
                if s.item.content_type == ContentType::Shows
                    && let Some(t) = &s.torrent
                {
                    let p = engine::parse::parse_name(t.files[index].name());
                    if let (Some(se), Some(ep)) = (p.season, p.episode) {
                        let show = engine::parse::parse_name(&s.item.title).title;
                        s.item.title = format!("{show} S{se:02} E{ep:02}");
                    }
                }
                s.seq
            });
            if let Some(seq) = seq {
                ps(|p| p.set_title(with(|s| s.item.title.clone()).unwrap_or_default().into()));
                play_torrent_file(seq, index, None);
            }
        }
        Some(Queue::Playlist) | Some(Queue::Related) => {
            let Some(Some((mut item, playlist))) = with(|s| {
                s.index = index;
                let item = s.queue.get(index).cloned()?;
                Some((item, s.queue_kind == Some(Queue::Playlist)))
            }) else {
                return;
            };
            item.position = engine::watchlist::position_of(&item).unwrap_or(0.0);
            let title = if playlist { ps(|p| p.get_list_title()).to_string() } else { "Up next".to_string() };
            refresh_list(&title);
            play_item(item);
        }
        _ => {}
    }
}

pub fn list_select(index: usize) {
    let kind = with(|s| s.queue_kind).flatten();
    if kind == Some(Queue::Related) {
        // a related video starts its own "up next"
        if let Some(Some(item)) = with(|s| s.queue.get(index).cloned()) {
            if index == with(|s| s.index).unwrap_or(0) {
                return;
            }
            save_position();
            with(|s| s.index = index);
            load_related(item.clone());
            play_item(item);
        }
        return;
    }
    select(kind, index);
}

pub fn toggle_pause() {
    mpv_do(|p| p.toggle_pause());
    show_controls();
}

pub fn seek(seconds: f64) {
    mpv_do(|p| p.seek(seconds, true));
}

pub fn seek_relative(seconds: f64) {
    mpv_do(|p| p.seek_relative(seconds));
    show_controls();
}

pub fn set_volume(v: f64) {
    let v = v.clamp(0.0, 200.0);
    with(|s| {
        s.volume = v;
        s.muted = false;
    });
    mpv_do(|p| p.set_volume(v));
    ps(|p| {
        p.set_volume(v as f32);
        p.set_muted(false);
    });
    if std::env::var_os("UFLIX_TEST_VOLUME").is_none() {
        engine::settings::set_volume(v);
    }
}

pub fn toggle_mute() {
    let Some((muted, volume)) = with(|s| {
        s.muted = !s.muted;
        (s.muted, s.volume)
    }) else {
        return;
    };
    mpv_do(|p| p.set_volume(if muted { 0.0 } else { volume }));
    ps(|p| p.set_muted(muted));
}

pub fn sub_delay(delta: f64) {
    let d = with(|s| {
        s.sub_delay += delta;
        s.sub_delay
    })
    .unwrap_or(0.0);
    mpv_do(|p| p.set_sub_delay(d));
    ps(|p| p.set_sub_delay_text(format!("{d:+.1} s").into()));
    ui::toast(format!("Subtitle delay {d:+.1} s"));
}

pub fn toggle_fullscreen() {
    let w = ui::window();
    let fs = !w.window().is_fullscreen();
    w.window().set_fullscreen(fs);
    ps(|p| p.set_fullscreen(fs));
    update_video_margin();
}

pub fn toggle_theater() {
    ps(|p| p.set_theater(!p.get_theater()));
    update_video_margin();
}

/// Keeps the video left of the side panel (mpv letterboxes into the remaining area).
pub fn update_video_margin() {
    let (side, width) = ps(|p| (p.get_side_available() && !p.get_theater() && !p.get_fullscreen(), ui::window().window().size().width));
    let logical = width as f32 / ui::scale_factor();
    let ratio = if side && logical > 0.0 { (400.0 / logical).min(0.6) } else { 0.0 };
    mpv_do(|p| p.set_property("video-margin-ratio-right", &format!("{ratio:.4}")));
}

// ---------------------------------------------------------------- controls visibility

/// Shows the bars and hides them again after 3 s of no activity while playing.
pub fn show_controls() {
    ps(|p| p.set_controls_visible(true));
    mpv_do(|p| p.set_property("sub-margin-y", "110"));
    HIDE.with(|t| {
        t.start(slint::TimerMode::SingleShot, Duration::from_secs(3), || {
            let busy = ps(|p| p.get_paused() || p.get_loading() || p.get_menu() != "" || !p.get_open());
            if !busy {
                ps(|p| p.set_controls_visible(false));
                mpv_do(|p| p.set_property("sub-margin-y", "34"));
            }
        })
    });
}

// ---------------------------------------------------------------- menus

pub fn open_menu(name: &str) {
    let current = ps(|p| p.get_menu());
    if current == name && S.with(|s| s.borrow().is_some()) && !matches!(name, "subs" | "audio") {
        ps(|p| p.set_menu("".into()));
        return;
    }
    let mut items: Vec<MenuItem> = Vec::new();
    let mut actions: Vec<Option<MenuAction>> = Vec::new();
    let mut add = |text: String, checked: bool, action: Option<MenuAction>, header: bool| {
        items.push(MenuItem { text: text.into(), checked, enabled: true, header });
        actions.push(action);
    };
    let title;
    match name {
        "subs" => {
            title = "Subtitles";
            let tracks = with(|s| s.tracks.clone()).unwrap_or_default();
            let subs: Vec<&Track> = tracks.iter().filter(|t| t.kind == TrackType::Subtitle).collect();
            add("Off".into(), !subs.iter().any(|t| t.selected), Some(MenuAction::SubOff), false);
            for t in subs {
                add(t.description(), t.selected, Some(MenuAction::SubTrack(t.id)), false);
            }
            let captions = with(|s| s.streams.as_ref().map(|x| x.captions.clone())).flatten().unwrap_or_default();
            if !captions.is_empty() {
                add("YouTube captions".into(), false, None, true);
                for c in captions.into_iter().take(30) {
                    add(c.label.clone(), false, Some(MenuAction::Caption(c.url)), false);
                }
            }
            add("Online".into(), false, None, true);
            add(
                format!("Search subtitles ({})…", engine::subtitles::Provider::current().label()),
                false,
                Some(MenuAction::SearchSubs),
                false,
            );
        }
        "audio" => {
            title = "Audio";
            let tracks = with(|s| s.tracks.clone()).unwrap_or_default();
            for t in tracks.iter().filter(|t| t.kind == TrackType::Audio) {
                add(t.description(), t.selected, Some(MenuAction::Audio(t.id)), false);
            }
        }
        "quality" => {
            title = "Quality";
            let (opts, current) =
                with(|s| (s.streams.as_ref().map(|x| x.options.clone()).unwrap_or_default(), s.quality)).unwrap_or_default();
            for (i, o) in opts.iter().enumerate() {
                add(o.label.clone(), i == current, Some(MenuAction::Quality(i)), false);
            }
        }
        _ => {
            ps(|p| p.set_menu(name.into()));
            show_controls();
            return;
        }
    }
    with(|s| s.menu = actions.into_iter().map(|a| a.unwrap_or(MenuAction::SubOff)).collect());
    // headers carry no action: mark them so a click does nothing (rows with header=true aren't clickable)
    ps(|p| {
        p.set_menu_title(title.into());
        p.set_menu_items(strings(&items));
        p.set_menu(name.into());
    });
    show_controls();
}

pub fn menu_select(index: usize) {
    let Some(Some(action)) = with(|s| s.menu.get(index).cloned()) else { return };
    let close_menu = || ps(|p| p.set_menu("".into()));
    match action {
        MenuAction::SubOff => {
            mpv_do(|p| p.select_track(TrackType::Subtitle, -1));
            close_menu();
        }
        MenuAction::SubTrack(id) => {
            mpv_do(|p| p.select_track(TrackType::Subtitle, id));
            close_menu();
        }
        MenuAction::Caption(url) => {
            mpv_do(|p| p.add_subtitle(&url, true));
            close_menu();
        }
        MenuAction::SearchSubs => {
            close_menu();
            if let Some(item) = with(|s| s.item.clone()) {
                crate::dialogs::subtitles::open(item, |path| mpv_do(|p| p.add_subtitle(&path, true)));
            }
        }
        MenuAction::Audio(id) => {
            mpv_do(|p| p.select_track(TrackType::Audio, id));
            close_menu();
        }
        MenuAction::Quality(i) => {
            close_menu();
            change_quality(i);
        }
    }
}

/// Reloads the YouTube video at the current position in another quality.
fn change_quality(index: usize) {
    let Some(Some((opt, seq, pos, title))) = with(|s| {
        let opt = s.streams.as_ref()?.options.get(index)?.clone();
        s.quality = index;
        s.seq += 1;
        s.loaded = false;
        Some((opt, s.seq, s.position, s.item.title.clone()))
    }) else {
        return;
    };
    engine::settings::set_youtube_height(opt.height);
    set_loading(true, "Switching quality…");
    load(seq, opt.url, LoadOptions { audio_url: opt.audio_url, start: Some(pos), title: Some(title), ..Default::default() });
}

// ---------------------------------------------------------------- advanced

pub fn set_speed(speed: f64) {
    mpv_do(|p| p.set_speed(speed));
    ps(|p| p.set_speed(speed as f32));
}

fn apply_picture(p: &engine::settings::Picture) {
    mpv_do(|m| {
        m.set_double("brightness", p.brightness);
        m.set_double("contrast", p.contrast);
        m.set_double("saturation", p.saturation);
        m.set_double("gamma", p.gamma);
    });
}

pub fn set_picture(name: &str, value: f64) {
    let mut pic = engine::settings::picture();
    match name {
        "brightness" => pic.brightness = value,
        "contrast" => pic.contrast = value,
        "saturation" => pic.saturation = value,
        "gamma" => pic.gamma = value,
        _ => return,
    }
    engine::settings::set_picture(pic);
    mpv_do(|m| m.set_double(name, value));
    ps(|p| match name {
        "brightness" => p.set_brightness(value as f32),
        "contrast" => p.set_contrast(value as f32),
        "saturation" => p.set_saturation(value as f32),
        _ => p.set_gamma(value as f32),
    });
}

pub fn reset_picture() {
    let pic = engine::settings::Picture::default();
    engine::settings::set_picture(pic);
    apply_picture(&pic);
    ps(|p| {
        p.set_brightness(0.0);
        p.set_contrast(0.0);
        p.set_saturation(0.0);
        p.set_gamma(0.0);
    });
    set_speed(1.0);
}

// ---------------------------------------------------------------- info panel

pub fn toggle_info() {
    let open = ps(|p| {
        let open = !p.get_info_open();
        p.set_info_open(open);
        open
    });
    if open {
        refresh_stats();
        TICK.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_secs(1), refresh_stats));
    } else if with(|s| s.torrent.is_none()).unwrap_or(true) {
        TICK.with(|t| t.stop());
    }
}

fn refresh_stats() {
    let torrent = torrent_line();
    if ps(|p| p.get_loading())
        && let Some(line) = &torrent
    {
        ps(|p| p.set_torrent_stats(line.clone().into()));
    }
    if !ps(|p| p.get_info_open()) {
        return;
    }
    let Ok(m) = player() else { return };
    let g = |name: &str| m.get_property(name).unwrap_or_default();
    let mut stats = vec![
        ("Video", format!("{} · {}×{}", g("video-codec").split(' ').next().unwrap_or(""), g("video-params/w"), g("video-params/h"))),
        ("Frame rate", format!("{:.2} fps", m.get_double("estimated-vf-fps").or(m.get_double("container-fps")).unwrap_or(0.0))),
        ("Decoder", {
            let hw = g("hwdec-current");
            if hw.is_empty() || hw == "no" { "software".into() } else { format!("hardware ({hw})") }
        }),
        ("Dropped", format!("{} frames", g("frame-drop-count"))),
        ("Video bitrate", format!("{:.1} Mb/s", m.get_double("video-bitrate").unwrap_or(0.0) / 1e6)),
        ("Audio", format!("{} · {} ch · {} Hz", g("audio-codec-name"), g("audio-params/channel-count"), g("audio-params/samplerate"))),
        ("Cache", format!("{:.0} s ahead", m.get_double("demuxer-cache-duration").unwrap_or(0.0))),
    ];
    if let Some(line) = torrent {
        stats.push(("Torrent", line));
    }
    let model: Vec<Stat> = stats.into_iter().map(|(l, v)| Stat { label: SharedString::from(l), value: v.into() }).collect();
    ps(|p| p.set_stats(ModelRc::from(Rc::new(VecModel::from(model)))));
}

// ---------------------------------------------------------------- YouTube details

fn show_details(item: &MediaItem) {
    let id = item.url.clone();
    ps(|p| {
        p.set_details_title(item.title.clone().into());
        p.set_details_channel(item.author.clone().into());
        p.set_details_stats("".into());
        p.set_details_description("".into());
        p.set_description_expanded(false);
    });
    spawn_ui(async move {
        let key = id.clone();
        let d = bg(async move { youtube::details(&key).await }).await;
        if with(|s| s.item.url == id) != Some(true) {
            return;
        }
        if let Ok(d) = d {
            ps(|p| {
                p.set_details_title(d.title.into());
                p.set_details_channel(d.author.into());
                p.set_details_stats(d.stats.into());
                p.set_details_description(d.description.into());
            });
            with(|s| {
                if s.item.author.is_empty() {
                    s.item.author = ps(|p| p.get_details_channel()).to_string();
                }
            });
        }
    });
}

pub fn open_channel() {
    let Some(id) = with(|s| s.item.url.clone()) else { return };
    spawn_ui(async move {
        let key = id.clone();
        if let Ok(d) = bg(async move { youtube::details(&key).await }).await {
            close();
            crate::pages::youtube::show_channel(d.channel_id, d.author);
        }
    });
}

// ---------------------------------------------------------------- closing

fn leave(s: Session) {
    if s.loaded {
        let mut item = s.item.clone();
        if item.duration.is_some_and(|d| d > 0.0 && item.position > d - 30.0) {
            item.position = 0.0;
        }
        engine::watchlist::add_or_update(&item);
    }
    mpv_do(|p| p.stop());
    if let Some(t) = s.torrent {
        crate::services::torrents::release(&t.url, &t.handle);
    }
}

pub fn close() {
    TICK.with(|t| t.stop());
    HIDE.with(|t| t.stop());
    if let Some(s) = S.with(|s| s.borrow_mut().take()) {
        leave(s);
    }
    let w = ui::window();
    if w.window().is_fullscreen() {
        w.window().set_fullscreen(false);
    }
    ps(|p| {
        p.set_open(false);
        p.set_fullscreen(false);
        p.set_loading(false);
        p.set_menu("".into());
    });
    super::video::set_active(false);
    crate::services::keep_awake::set(false);
    w.global::<Shell>().set_toast("".into());
    crate::pages::home::refresh_watchlist();
    crate::pages::youtube::refresh_continue_watching();
}
