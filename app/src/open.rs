//! Opening an item: library titles (TMDB / IMDb ids) show their movie or show dialog first; files, links, YouTube and
//! channels go straight to the player.

use engine::models::{ContentType, MediaItem, MediaType};

pub fn open_media(item: MediaItem) {
    let library = item.tmdb_id > 0 || item.id.starts_with("tt");
    match item.content_type {
        ContentType::Movie if library => crate::dialogs::title::open_movie(item),
        ContentType::Shows if library => crate::dialogs::title::open_show(item),
        _ if item.media_type == MediaType::Local && item.url.is_empty() => crate::dialogs::title::open_movie(item),
        _ => play(item),
    }
}

/// Opens the player on `item`.
pub fn play(item: MediaItem) {
    crate::player::open(item);
}
