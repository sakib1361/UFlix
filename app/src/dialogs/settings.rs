//! Settings: player (subtitle style, subtitle provider), downloads, torrent search sources, Live TV playlists, updates.

use crate::runtime::{blocking, spawn_ui};
use crate::services::updater::{self, Outcome};
use crate::ui::{self, EngineUrl, SettingsState};
use engine::search::Engine;
use engine::settings as s;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static ENGINE_URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn st<R>(f: impl FnOnce(&SettingsState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<SettingsState>())
}

pub fn init() {
    st(|x| {
        x.set_version(format!("v{}", env!("UFLIX_VERSION")).into());
        let providers: Vec<SharedString> = engine::subtitles::Provider::ALL.iter().map(|p| p.label().into()).collect();
        x.set_subtitle_providers(ModelRc::from(Rc::new(VecModel::from(providers))));
        x.on_close(|| st(|x| x.set_open(false)));
        x.on_save(save);
        x.on_pick_folder(|which| pick_folder(which.to_string()));
        x.on_engine_url_edited(|i, url| {
            ENGINE_URLS.with(|e| {
                if let Some(slot) = e.borrow_mut().get_mut(i as usize) {
                    *slot = url.to_string();
                }
            })
        });
        x.on_check_updates(check);
        x.on_install_update(install);
        x.on_open_releases(|| {
            let _ = open::that_detached(updater::releases_url());
        });
    });
}

pub fn open() {
    let urls: Vec<String> = Engine::ALL.iter().map(|e| e.url()).collect();
    ENGINE_URLS.with(|e| *e.borrow_mut() = urls.clone());
    let engines: Vec<EngineUrl> =
        Engine::ALL.iter().zip(urls).map(|(e, url)| EngineUrl { name: e.label().into(), url: url.into() }).collect();
    let provider = engine::subtitles::Provider::current();
    st(|x| {
        x.set_sub_font_size(format!("{}", s::subtitle_font_size()).into());
        x.set_sub_color(s::subtitle_color().into());
        x.set_sub_font(s::subtitle_font().into());
        x.set_subtitle_provider(engine::subtitles::Provider::ALL.iter().position(|p| *p == provider).unwrap_or(0) as i32);
        x.set_torrent_folder(s::torrent_download_path().into());
        x.set_youtube_folder(s::youtube_download_path().into());
        x.set_max_folder_size(format!("{}", s::max_folder_size()).into());
        x.set_ytdl_first(s::use_ytdl_first());
        x.set_engines(ModelRc::from(Rc::new(VecModel::from(engines))));
        x.set_tv_links(s::tv_links().into());
        x.set_daily_check(s::check_updates_daily());
        x.set_can_update(updater::REPO.is_some());
        x.set_update_status(
            if updater::REPO.is_none() {
                "Development build: no update feed"
            } else if updater::available() {
                ""
            } else {
                "This copy can't update itself; use the releases page."
            }
            .into(),
        );
        x.set_open(true);
    });
}

fn save() {
    st(|x| {
        if let Ok(size) = x.get_sub_font_size().trim().parse::<f64>() {
            s::set_subtitle_font_size(size.clamp(10.0, 120.0));
        }
        let color = x.get_sub_color().trim().to_string();
        if color.starts_with('#') && (color.len() == 7 || color.len() == 9) {
            s::set_subtitle_color(color);
        }
        s::set_subtitle_font(x.get_sub_font().trim().to_string());
        if let Some(p) = engine::subtitles::Provider::ALL.get(x.get_subtitle_provider() as usize) {
            s::set_subtitle_provider(p.key().to_string());
        }
        s::set_torrent_download_path(x.get_torrent_folder().trim().to_string());
        s::set_youtube_download_path(x.get_youtube_folder().trim().to_string());
        s::set_max_folder_size(x.get_max_folder_size().trim().parse().unwrap_or(0).max(0));
        s::set_use_ytdl_first(x.get_ytdl_first());
        s::set_tv_links(x.get_tv_links().trim().to_string());
        s::set_check_updates_daily(x.get_daily_check());
        x.set_open(false);
    });
    for (e, url) in Engine::ALL.iter().zip(ENGINE_URLS.with(|u| u.borrow().clone())) {
        e.set_url(url.trim());
    }
    ui::toast("Settings saved. Subtitle style applies to the next video.");
}

fn pick_folder(which: String) {
    spawn_ui(async move {
        let folder = blocking(|| rfd::FileDialog::new().set_title("Choose a folder").pick_folder()).await;
        if let Some(f) = folder {
            let path: SharedString = f.to_string_lossy().into_owned().into();
            st(|x| if which == "torrent" { x.set_torrent_folder(path) } else { x.set_youtube_folder(path) });
        }
    });
}

fn check() {
    st(|x| {
        x.set_checking(true);
        x.set_update_status("Checking…".into());
    });
    spawn_ui(async {
        let outcome = updater::check().await;
        st(|x| {
            x.set_checking(false);
            let (text, available) = match outcome {
                Outcome::UpToDate => ("You have the latest version.".to_string(), false),
                Outcome::Available(v) => (format!("UFlix {v} is available."), true),
                Outcome::NotInstalled => ("A newer version may exist: this copy can't update itself, see the releases page.".into(), false),
                Outcome::Failed(e) => (format!("Couldn't check: {e}"), false),
            };
            x.set_update_status(text.into());
            x.set_update_available(available);
        });
    });
}

fn install() {
    st(|x| x.set_update_progress(0.0));
    spawn_ui(async {
        let result = updater::install(|p| crate::runtime::ui(move || st(|x| x.set_update_progress(p)))).await;
        if let Err(e) = result {
            st(|x| {
                x.set_update_progress(-1.0);
                x.set_update_status(format!("Update failed: {e}").into());
            });
        }
    });
}
