// no console window in Windows release builds
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! UFlix: movies, shows, torrents, YouTube and live TV, played by mpv inside a Slint UI.

mod cards;
mod diag;
mod dialogs;
mod images;
mod open;
mod pages;
mod player;
mod runtime;
mod services;
mod tour;
mod ui;

use slint::ComponentHandle;
use ui::{AppWindow, Page, Shell};

/// Shows a page; pages that refresh while visible are told when they show or hide.
pub fn navigate(page: Page) {
    let w = ui::window();
    let shell = w.global::<Shell>();
    if shell.get_page() == Page::Torrents && page != Page::Torrents {
        pages::torrents::set_visible(false);
    }
    shell.set_page(page);
    match page {
        Page::Discover => pages::discover::shown(),
        Page::Youtube => pages::youtube::shown(),
        Page::Tv => pages::tv::shown(),
        Page::Torrents => pages::torrents::set_visible(true),
        _ => {}
    }
}

fn main() -> anyhow::Result<()> {
    runtime::init();
    diag::start();
    // The femtovg renderer runs on OpenGL, which mpv's render API draws the video into.
    slint::BackendSelector::new().require_opengl().select()?;

    let window = AppWindow::new()?;
    ui::set_window(&window);
    window.global::<Shell>().set_version(format!("v{}", env!("UFLIX_VERSION")).into());
    window.global::<Shell>().on_navigate(navigate);
    window.global::<Shell>().on_open_dialog(|name| match name.as_str() {
        "open" => dialogs::open_dialog::open(),
        "download" => dialogs::download::open(),
        "settings" => dialogs::settings::open(),
        _ => {}
    });

    std::thread::spawn(|| {
        engine::image_cache::clear_old();
        engine::cleaner::enforce_size_cap();
    });
    pages::home::init();
    pages::discover::init();
    pages::youtube::init();
    pages::tv::init();
    pages::torrents::init();
    dialogs::settings::init();
    dialogs::open_dialog::init();
    dialogs::download::init();
    dialogs::subtitles::init();
    dialogs::title::init();
    player::init(&window);
    services::torrents::start();
    services::updater::schedule(std::time::Duration::from_secs(30));
    // `uflix <file or link>` opens it at start-up
    if let Some(arg) = std::env::args().nth(1).filter(|a| !a.starts_with('-')) {
        slint::Timer::single_shot(std::time::Duration::from_millis(300), move || dialogs::open_dialog::open_text(&arg));
    }

    // UFLIX_TOUR=<folder>: the app walks its screens and saves screenshots (see tour.rs)
    if let Some(steps) = tour::steps() {
        tour::run(steps);
    }
    // start maximized, like the C# app (a fixed size can be taller than a small or scaled screen)
    window.window().set_maximized(true);
    // not window.run(): it hides the window (on Wayland that tears down GL) before playback could be stopped
    window.show()?;
    slint::run_event_loop()?;
    player::close();
    // mpv goes before the window's GL context and before the torrent session its stream reads from
    player::shutdown(window);
    services::torrents::shutdown();
    Ok(())
}
