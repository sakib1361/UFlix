//! UFLIX_TOUR=<folder>: the app walks through its screens by itself and saves a PNG of each (development checks and the
//! README screenshots). Nothing is driven from outside the app. Run it with a scratch XDG_DATA_HOME so no personal
//! watchlist or settings show (and none are changed).
//!
//! - UFLIX_TOUR_PLAY=<file, link or magnet>: only the player screens, on that source (UFLIX_TOUR_WAIT ms to load it).
//! - otherwise: every page, dialog and player panel; the player plays Big Buck Bunny (CC BY 3.0, Blender Foundation).

use crate::runtime::{sleep, spawn_ui};
use crate::ui::{self, *};
use crate::{Page, dialogs, navigate, pages};
use engine::models::{ContentType, MediaItem};
use slint::ComponentHandle;
use std::path::PathBuf;

/// One stop: what to do, how long to wait for it to load, and the file name.
pub struct Step {
    pub name: &'static str,
    pub wait_ms: u64,
    pub action: Box<dyn Fn()>,
}

fn step(name: &'static str, wait_ms: u64, action: impl Fn() + 'static) -> Step {
    Step { name, wait_ms, action: Box::new(action) }
}

pub fn folder() -> Option<PathBuf> {
    std::env::var_os("UFLIX_TOUR").map(PathBuf::from)
}

/// The tour asked for by the environment, if any.
pub fn steps() -> Option<Vec<Step>> {
    folder()?;
    Some(match std::env::var("UFLIX_TOUR_PLAY") {
        Ok(source) => player_steps(source),
        Err(_) => full_steps(),
    })
}

fn w() -> AppWindow {
    ui::window()
}

const BIG_BUCK_BUNNY: &str = "https://archive.org/download/BigBuckBunny_124/Content/big_buck_bunny_720p_surround.mp4";
const BIG_BUCK_BUNNY_YT: &str = "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
const BLENDER_CHANNEL: &str = "UCSMOQeBJ2RAnuFungnQOxLg";

fn player_steps(source: String) -> Vec<Step> {
    let wait = std::env::var("UFLIX_TOUR_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(7000);
    vec![
        step("player", wait, move || dialogs::open_dialog::open_text(&source)),
        step("player-advanced", 1500, || w().global::<PlayerState>().invoke_open_menu("advanced".into())),
        step("player-info", 2500, || {
            w().global::<PlayerState>().set_menu("".into());
            w().global::<PlayerState>().invoke_toggle_info();
        }),
    ]
}

fn close_dialogs() {
    let win = w();
    win.global::<TitleState>().set_open(false);
    win.global::<SettingsState>().set_open(false);
    win.global::<OpenState>().set_open(false);
    win.global::<DownloadState>().set_open(false);
    win.global::<SubtitlesState>().set_open(false);
    win.global::<TvState>().set_picker_open(false);
    win.global::<HomeState>().set_results_open(false);
}

fn full_steps() -> Vec<Step> {
    vec![
        // home and search
        step("home", 7000, || navigate(Page::Home)),
        step("search-movies", 4000, || {
            let win = w();
            let h = win.global::<HomeState>();
            h.set_query("dune".into());
            h.set_results_open(true);
            h.set_tab(0);
            h.invoke_search_edited("dune".into());
        }),
        step("search-shows", 1500, || w().global::<HomeState>().set_tab(1)),
        // discover
        step("discover-movies", 5000, || {
            close_dialogs();
            navigate(Page::Discover);
        }),
        step("discover-shows", 5000, || w().global::<DiscoverState>().invoke_select_content(1)),
        step("discover-genre", 5000, || w().global::<DiscoverState>().invoke_select_category(4)),
        // the title dialogs
        step("movie", 7000, || {
            let item =
                MediaItem { title: "Dune: Part Two".into(), tmdb_id: 693134, content_type: ContentType::Movie, ..Default::default() };
            crate::open::open_media(item);
        }),
        step("movie-1080p", 1500, || w().global::<TitleState>().invoke_select_quality(1)),
        step("show", 7000, || {
            close_dialogs();
            let item = MediaItem { title: "Severance".into(), tmdb_id: 95396, content_type: ContentType::Shows, ..Default::default() };
            crate::open::open_media(item);
        }),
        step("show-episode", 7000, || w().global::<TitleState>().invoke_select_episode(0)),
        step("show-cast", 2500, || w().global::<TitleState>().set_left_tab(1)),
        // YouTube
        step("youtube", 7000, || {
            close_dialogs();
            navigate(Page::Youtube);
        }),
        step("youtube-search", 6000, || {
            let win = w();
            let y = win.global::<YoutubeState>();
            y.set_query("blender open movie".into());
            y.invoke_submit("blender open movie".into());
        }),
        step("youtube-playlists", 5000, || w().global::<YoutubeState>().invoke_select_filter(2)),
        step("youtube-channel", 7000, || pages::youtube::show_channel(BLENDER_CHANNEL.into(), "Blender".into())),
        // live TV
        step("tv", 20000, || navigate(Page::Tv)),
        step("tv-filter", 2000, || {
            let win = w();
            let t = win.global::<TvState>();
            t.set_filter("news".into());
            t.invoke_filter_edited("news".into());
        }),
        step("tv-picker", 2000, pages::tv::open_first_picker),
        // torrents
        step("torrents", 2500, || navigate(Page::Torrents)),
        step("torrents-search", 9000, || {
            let win = w();
            let t = win.global::<TorrentsState>();
            t.invoke_select_tab(1);
            t.set_query("Big Buck Bunny".into());
            t.invoke_search("Big Buck Bunny".into());
        }),
        // dialogs
        step("settings-player", 1500, dialogs::settings::open),
        step("settings-downloads", 1000, || w().global::<SettingsState>().set_tab(1)),
        step("settings-torrents", 1000, || w().global::<SettingsState>().set_tab(2)),
        step("settings-livetv", 1000, || {
            let win = w();
            let x = win.global::<SettingsState>();
            x.set_tab(3);
            x.set_new_tv_link("https://iptv-org.github.io/iptv/categories/news.m3u".into());
            x.invoke_add_tv_link();
        }),
        step("settings-updates", 1000, || w().global::<SettingsState>().set_tab(4)),
        step("open", 1200, || {
            close_dialogs();
            dialogs::open_dialog::open();
        }),
        step("download", 8000, || {
            close_dialogs();
            dialogs::download::open();
            let win = w();
            let d = win.global::<DownloadState>();
            d.set_url(BIG_BUCK_BUNNY_YT.into());
            d.invoke_parse();
        }),
        // the player
        step("player", 12000, || {
            close_dialogs();
            crate::open::play(MediaItem {
                title: "Big Buck Bunny".into(),
                url: BIG_BUCK_BUNNY.into(),
                image: "https://archive.org/services/img/BigBuckBunny_124".into(),
                year: "2008".into(),
                author: "Blender Foundation".into(),
                media_type: engine::models::MediaType::Web,
                ..Default::default()
            });
        }),
        step("player-controls", 3000, || {
            let win = w();
            let p = win.global::<PlayerState>();
            p.invoke_seek(300.0);
            p.invoke_activity();
        }),
        step("player-advanced", 1500, || w().global::<PlayerState>().invoke_open_menu("advanced".into())),
        step("player-subtitles", 1500, || w().global::<PlayerState>().invoke_open_menu("subs".into())),
        step("player-info", 2500, || {
            let win = w();
            let p = win.global::<PlayerState>();
            p.set_menu("".into());
            p.invoke_toggle_info();
        }),
        step("player-youtube", 20000, || {
            let win = w();
            let p = win.global::<PlayerState>();
            p.set_info_open(false);
            dialogs::open_dialog::open_text(BIG_BUCK_BUNNY_YT);
        }),
        step("home-continue", 6000, || {
            crate::player::close();
            w().global::<HomeState>().set_query("".into());
            navigate(Page::Home);
        }),
    ]
}

/// UFLIX_TOUR_FROM=<shot> starts at that shot (re-shooting one section).
pub fn run(mut steps: Vec<Step>) {
    let Some(dir) = folder() else { return };
    if let Ok(from) = std::env::var("UFLIX_TOUR_FROM")
        && let Some(i) = steps.iter().position(|s| s.name == from)
    {
        steps.drain(..i);
    }
    let _ = std::fs::create_dir_all(&dir);
    spawn_ui(async move {
        for step in steps {
            (step.action)();
            sleep(step.wait_ms).await;
            snapshot(&dir.join(format!("{}.png", step.name)));
        }
        let _ = slint::quit_event_loop();
    });
}

pub fn snapshot(path: &std::path::Path) {
    match w().window().take_snapshot() {
        Ok(buf) => {
            if let Some(img) = image::RgbaImage::from_raw(buf.width(), buf.height(), buf.as_bytes().to_vec()) {
                let _ = img.save(path);
                println!("tour: saved {}", path.display());
            }
        }
        Err(e) => eprintln!("tour: snapshot failed: {e}"),
    }
}
