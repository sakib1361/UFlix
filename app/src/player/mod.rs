//! The player page: mpv video (see `video`) under a Slint overlay, driven by `session`.

mod session;
mod video;

use crate::ui::{self, PlayerState};
use engine::models::MediaItem;
use slint::ComponentHandle;

pub use session::close;

pub fn open(item: MediaItem) {
    session::open(item);
}

pub fn init(window: &ui::AppWindow) {
    if let Err(e) = video::install(window) {
        eprintln!("video: {e:?}");
    }
    let p = window.global::<PlayerState>();
    p.on_close(session::close);
    p.on_toggle_pause(session::toggle_pause);
    p.on_seek(|t| session::seek(t as f64));
    p.on_seek_relative(|d| session::seek_relative(d as f64));
    p.on_set_volume(|v| session::set_volume(v as f64));
    p.on_toggle_mute(session::toggle_mute);
    p.on_toggle_fullscreen(session::toggle_fullscreen);
    p.on_prev(session::prev);
    p.on_next(session::next);
    p.on_open_menu(|m| session::open_menu(&m));
    p.on_menu_select(|i| session::menu_select(i as usize));
    p.on_sub_delay(|d| session::sub_delay(d as f64));
    p.on_activity(session::show_controls);
    p.on_set_speed(|s| session::set_speed(s as f64));
    p.on_set_picture(|name, v| session::set_picture(&name, v as f64));
    p.on_reset_picture(session::reset_picture);
    p.on_toggle_info(session::toggle_info);
    p.on_toggle_theater(session::toggle_theater);
    p.on_list_select(|i| session::list_select(i as usize));
    p.on_set_autoplay(engine::settings::set_autoplay);
    p.on_open_channel(session::open_channel);
    p.on_width_changed(|_| session::update_video_margin());
}
