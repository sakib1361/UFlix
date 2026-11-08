//! "Continue watching": what was played in the last 10 days, newest first, with the last position.

use crate::db::db;
use crate::models::{MediaItem, MediaType};

const KEY: &str = "ContinueWatch";

pub fn items() -> Vec<MediaItem> {
    db().get::<Vec<MediaItem>>(KEY).unwrap_or_default()
}

/// Records `media` as just played (IPTV channels are never saved).
pub fn add_or_update(media: &MediaItem) {
    if media.media_type == MediaType::Unknown {
        return;
    }
    let now = chrono::Utc::now();
    let mut entry = media.clone();
    entry.last_played = Some(now);
    let key = entry.key();
    let mut list: Vec<MediaItem> =
        items().into_iter().filter(|x| x.key() != key && x.last_played.is_some_and(|t| (now - t).num_days() < 10)).collect();
    list.insert(0, entry);
    db().set(KEY, &list);
}

pub fn remove(media: &MediaItem) {
    let key = media.key();
    let list: Vec<MediaItem> = items().into_iter().filter(|x| x.key() != key).collect();
    db().set(KEY, &list);
}

/// The saved position of `media`, if it was played before.
pub fn position_of(media: &MediaItem) -> Option<f64> {
    let key = media.key();
    items().into_iter().find(|x| x.key() == key).map(|x| x.position).filter(|p| *p > 5.0)
}
