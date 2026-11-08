//! The movie / show dialog: details, cast or seasons and episodes, torrent search per provider grouped by quality.

use crate::cards::{CardList, CardSpec};
use crate::images;
use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, TitleState, TorrentRow};
use engine::models::{ContentType, MediaItem, MediaType, Quality, TorrentInfo};
use engine::search::Engine;
use engine::tmdb::{self, Episode, Movie, Show};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

enum Title {
    Movie(Movie),
    Show(Show),
}

struct State {
    title: Title,
    /// The watchlist entry this dialog was opened from (for "Continue").
    resume: Option<MediaItem>,
    engine: usize,
    groups: Vec<(Quality, Vec<TorrentInfo>)>,
    quality: usize,
    seq: u64,
    season: usize,
    episodes: Vec<Episode>,
    episode: Option<usize>,
    /// A whole-season search is shown instead of an episode's.
    full_season: bool,
    episode_cards: CardList,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Runs `f` with the dialog's Slint state.
fn ts<R>(f: impl FnOnce(&TitleState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<TitleState>())
}

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(items.into_iter().map(SharedString::from).collect::<Vec<_>>())))
}

pub fn init() {
    let w = ui::window();
    let t = w.global::<TitleState>();
    t.on_close(close);
    t.on_resume(|| {
        if let Some(Some(item)) = with(|s| s.resume.clone()) {
            close();
            crate::open::play(item);
        }
    });
    t.on_select_provider(|i| {
        with(|s| s.engine = i as usize);
        ts(|t| t.set_provider(i));
        search(false);
    });
    t.on_refresh(|| search(true));
    t.on_select_quality(|i| show_quality(i as usize));
    t.on_select_season(|i| load_season(i as usize));
    t.on_select_episode(|i| {
        with(|s| {
            s.episode = Some(i as usize);
            s.full_season = false;
        });
        ts(|t| t.set_episode(i));
        search(false);
    });
    t.on_full_season(|| {
        with(|s| {
            s.full_season = true;
            s.episode = None;
        });
        ts(|t| t.set_episode(-1));
        search(true);
    });
    t.on_play(|i| {
        if let Some(Some(info)) = with(|s| s.groups.get(s.quality).and_then(|(_, list)| list.get(i as usize).cloned())) {
            play(info.url.clone(), info.media_type, Some(info.title.clone()));
        }
    });
    t.on_download(|i| {
        if let Some(Some(info)) = with(|s| s.groups.get(s.quality).and_then(|(_, list)| list.get(i as usize).cloned())) {
            crate::services::torrents::download(&info.url, &info.title);
        }
    });
    t.on_open_custom(|| {
        let url = ts(|t| t.get_custom_url().trim().to_string());
        if url.is_empty() {
            return;
        }
        let kind = if url.starts_with("magnet:") || url.ends_with(".torrent") {
            MediaType::Torrent
        } else if url.starts_with("http") {
            MediaType::Web
        } else {
            MediaType::Local
        };
        play(url, kind, None);
    });
    let providers: Vec<String> = Engine::ALL.iter().map(|e| e.label().to_string()).collect();
    t.set_providers(strings(providers));
}

fn close() {
    ts(|t| t.set_open(false));
    STATE.with(|s| *s.borrow_mut() = None);
}

/// Opens the dialog for a library movie. Falls back to the player when TMDB doesn't know it.
pub fn open_movie(item: MediaItem) {
    ui::set_busy(true);
    spawn_ui(async move {
        let lookup = item.clone();
        let movie = bg(async move { tmdb::movie(&lookup).await }).await;
        ui::set_busy(false);
        match movie {
            Some(m) => show(Title::Movie(m), item),
            None if !item.url.is_empty() => crate::open::play(item),
            None => ui::toast(format!("No details found for {}", item.title)),
        }
    });
}

pub fn open_show(item: MediaItem) {
    ui::set_busy(true);
    spawn_ui(async move {
        let lookup = item.clone();
        let found = bg(async move { tmdb::show(&lookup, false).await }).await;
        ui::set_busy(false);
        match found {
            Some(s) => show(Title::Show(s), item),
            None => ui::toast(format!("No details found for {}", item.title)),
        }
    });
}

fn show(title: Title, from: MediaItem) {
    let w = ui::window();
    let t = w.global::<TitleState>();
    let resume = (!from.url.is_empty() && from.position > 0.0).then_some(from.clone());
    let (name, meta, rating, overview, poster, backdrop, cast) = match &title {
        Title::Movie(m) => {
            let mut meta = vec![m.year()];
            if let Some(r) = m.runtime.filter(|r| *r > 0) {
                meta.push(format!("{}h {:02}m", r / 60, r % 60));
            }
            meta.push(m.genres.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", "));
            (m.title.clone(), meta, m.vote_average, m.overview.clone(), m.poster_path.clone(), m.backdrop_path.clone(), m.credits.clone())
        }
        Title::Show(s) => {
            let mut meta = vec![s.year(), format!("{} seasons", s.seasons.len())];
            if !s.status.is_empty() {
                meta.push(s.status.clone());
            }
            meta.push(s.genres.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", "));
            (s.name.clone(), meta, s.vote_average, s.overview.clone(), s.poster_path.clone(), s.backdrop_path.clone(), s.credits.clone())
        }
    };
    let is_show = matches!(title, Title::Show(_));
    t.set_is_show(is_show);
    t.set_title(name.into());
    t.set_meta(meta.into_iter().filter(|m| !m.is_empty()).collect::<Vec<_>>().join("  ·  ").into());
    t.set_rating(if rating > 0.0 { format!("{rating:.1}").into() } else { "".into() });
    t.set_overview(overview.into());
    t.set_poster(Default::default());
    t.set_backdrop(Default::default());
    t.set_resume_text(match &resume {
        Some(r) if is_show => format!("Continue {}", r.title.rsplit_once(" S").map(|(_, e)| format!("S{e}")).unwrap_or_default()).into(),
        Some(_) => "Continue watching".into(),
        None => "".into(),
    });
    t.set_custom_url(resume.as_ref().map(|r| r.url.clone()).unwrap_or_default().into());
    t.set_left_tab(0);
    t.set_provider(0);
    t.set_episode(-1);
    let scale = ui::scale_factor();
    let poster_url = tmdb::image(tmdb::IMAGE_300, &poster);
    let backdrop_url = tmdb::image(tmdb::IMAGE_780, &backdrop);
    spawn_ui(async move {
        if let Some(img) = images::load(poster_url, (168.0 * scale) as u32).await {
            ts(|t| t.set_poster(img));
        }
    });
    spawn_ui(async move {
        if let Some(img) = images::load(backdrop_url, (1180.0 * scale) as u32).await {
            ts(|t| t.set_backdrop(img));
        }
    });
    let cast_list = CardList::new(44);
    cast_list.push_all(
        cast.map(|c| c.cast)
            .unwrap_or_default()
            .into_iter()
            .take(40)
            .map(|c| CardSpec {
                title: c.name,
                subtitle: c.character,
                image_url: tmdb::image(tmdb::IMAGE_185, &c.profile_path),
                ..Default::default()
            })
            .collect(),
    );
    t.set_cast(cast_list.model());
    let episode_cards = CardList::new(128);
    t.set_episodes(episode_cards.model());
    let seasons = match &title {
        Title::Show(s) => s.seasons.iter().map(|x| format!("Season {}", x.season_number)).collect(),
        Title::Movie(_) => Vec::new(),
    };
    t.set_seasons(strings(seasons));
    // a resumed episode opens its season
    let resume_season = resume.as_ref().and_then(|r| engine::parse::parse_name(&r.title).season);
    let season_index = match (&title, resume_season) {
        (Title::Show(s), Some(n)) => s.seasons.iter().position(|x| x.season_number == n).unwrap_or(0),
        _ => 0,
    };
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            title,
            resume,
            engine: 0,
            groups: Vec::new(),
            quality: 0,
            seq: 0,
            season: 0,
            episodes: Vec::new(),
            episode: None,
            full_season: false,
            episode_cards,
        })
    });
    set_torrents(Vec::new(), "");
    t.set_open(true);
    if is_show {
        load_season(season_index);
        set_torrents(Vec::new(), "Pick an episode, or search the full season.");
    } else {
        search(false);
    }
}

fn load_season(index: usize) {
    let Some(Some((show_id, number, cards))) = with(|s| match &s.title {
        Title::Show(show) => {
            s.season = index;
            s.episode = None;
            s.episodes.clear();
            s.episode_cards.clear();
            show.seasons.get(index).map(|x| (show.id, x.season_number, s.episode_cards.clone()))
        }
        Title::Movie(_) => None,
    }) else {
        return;
    };
    let w = ui::window();
    let t = w.global::<TitleState>();
    t.set_season(index as i32);
    t.set_episode(-1);
    t.set_episodes_loading(true);
    spawn_ui(async move {
        let season = bg(async move { tmdb::season(show_id, number).await }).await;
        let episodes = season.map(|s| s.episodes).unwrap_or_default();
        let current = with(|s| s.season == index).unwrap_or(false);
        if !current {
            return;
        }
        ts(|t| t.set_episodes_loading(false));
        cards.push_all(
            episodes
                .iter()
                .map(|e| CardSpec {
                    title: if e.name.is_empty() { format!("Episode {}", e.episode_number) } else { e.name.clone() },
                    subtitle: [
                        e.air_date.clone().unwrap_or_default(),
                        if e.vote_average > 0.0 { format!("★ {:.1}", e.vote_average) } else { String::new() },
                    ]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join("  ·  "),
                    image_url: tmdb::image(tmdb::IMAGE_300, &e.still_path),
                    badge: format!("E{:02}", e.episode_number),
                    detail: e.overview.clone(),
                    ..Default::default()
                })
                .collect(),
        );
        with(|s| s.episodes = episodes);
    });
}

fn set_torrents(rows: Vec<TorrentRow>, empty: &str) {
    let w = ui::window();
    let t = w.global::<TitleState>();
    t.set_torrents(ModelRc::from(Rc::new(VecModel::from(rows))));
    t.set_empty_text(empty.into());
}

fn row(t: &TorrentInfo) -> TorrentRow {
    let mut meta = Vec::new();
    if !t.size.is_empty() {
        meta.push(t.size.clone());
    }
    if !t.seeders.is_empty() && t.media_type != MediaType::Local {
        meta.push(format!("{} seeds", t.seeders));
    }
    meta.push(t.provider.clone());
    TorrentRow {
        title: t.title.clone().into(),
        meta: meta.into_iter().filter(|m| !m.is_empty()).collect::<Vec<_>>().join("  ·  ").into(),
        is_local: t.media_type == MediaType::Local,
    }
}

fn search(forced: bool) {
    let Some((engine, title, seq, episode, full)) = with(|s| {
        s.seq += 1;
        let engine = Engine::ALL[s.engine.min(Engine::ALL.len() - 1)];
        let title = match &s.title {
            Title::Movie(m) => Title::Movie(m.clone()),
            Title::Show(sh) => Title::Show(sh.clone()),
        };
        let episode = s.episode.and_then(|i| s.episodes.get(i).cloned());
        (engine, title, s.seq, episode, s.full_season)
    }) else {
        return;
    };
    let season_no = with(|s| match &s.title {
        Title::Show(sh) => sh.seasons.get(s.season).map(|x| x.season_number).unwrap_or(1),
        _ => 1,
    })
    .unwrap_or(1);
    if matches!(title, Title::Show(_)) && episode.is_none() && !full {
        return;
    }
    let w = ui::window();
    let t = w.global::<TitleState>();
    t.set_searching(true);
    t.set_qualities(strings(Vec::new()));
    set_torrents(Vec::new(), "");
    spawn_ui(async move {
        let results = bg(async move {
            match title {
                Title::Movie(m) => engine.search_movie(&m, forced).await,
                Title::Show(sh) if full => engine.search_season(&sh, season_no, forced).await,
                Title::Show(sh) => {
                    let e = episode.unwrap_or_default();
                    engine.search_episode(&sh, e.season_number, e.episode_number, forced).await
                }
            }
        })
        .await;
        if with(|s| s.seq) != Some(seq) {
            return;
        }
        let mut groups: Vec<(Quality, Vec<TorrentInfo>)> = Vec::new();
        for q in Quality::ALL {
            let list: Vec<TorrentInfo> = results.iter().filter(|r| r.quality == q).cloned().collect();
            if !list.is_empty() {
                groups.push((q, list));
            }
        }
        let labels: Vec<String> = groups.iter().map(|(q, l)| format!("{} ({})", q.label(), l.len())).collect();
        let empty = format!("Nothing found on {}. Try another source.", engine.label());
        let w = ui::window();
        let t = w.global::<TitleState>();
        t.set_searching(false);
        t.set_qualities(strings(labels));
        with(|s| {
            s.groups = groups;
            s.quality = 0;
        });
        show_quality(0);
        if results.is_empty() {
            set_torrents(Vec::new(), &empty);
        }
    });
}

fn show_quality(index: usize) {
    let rows = with(|s| {
        s.quality = index;
        s.groups.get(index).map(|(_, l)| l.iter().map(row).collect::<Vec<_>>()).unwrap_or_default()
    })
    .unwrap_or_default();
    ts(|t| t.set_quality(index as i32));
    set_torrents(rows, "");
}

/// Builds the item to play from the dialog's title and the chosen source, closes the dialog and plays it.
fn play(url: String, kind: MediaType, release: Option<String>) {
    let Some(item) = with(|s| {
        let mut item = MediaItem { url, media_type: kind, ..Default::default() };
        match &s.title {
            Title::Movie(m) => {
                item.id = m.imdb().unwrap_or_default();
                item.tmdb_id = m.id;
                item.title = m.title.clone();
                item.description = m.overview.clone();
                item.image = tmdb::image(tmdb::IMAGE_300, &m.poster_path);
                item.backdrop = tmdb::image(tmdb::IMAGE_780, &m.backdrop_path);
                item.year = m.year();
                item.rating = m.vote_average;
                item.content_type = ContentType::Movie;
            }
            Title::Show(sh) => {
                let ep = s.episode.and_then(|i| s.episodes.get(i)).cloned().or_else(|| s.episodes.first().cloned()).unwrap_or_default();
                item.id = sh.imdb().unwrap_or_default();
                item.tmdb_id = sh.id;
                item.title = format!("{} S{:02} E{:02}", sh.name, ep.season_number, ep.episode_number);
                item.description = sh.overview.clone();
                item.image = tmdb::image(tmdb::IMAGE_300, &sh.poster_path);
                item.backdrop = tmdb::image(tmdb::IMAGE_780, &sh.backdrop_path);
                item.year = sh.year();
                item.rating = sh.vote_average;
                item.content_type = ContentType::Shows;
            }
        }
        let _ = release;
        item
    }) else {
        return;
    };
    close();
    crate::open::play(item);
}
