//! Open: a video file from disk, or a pasted link (web video, IPTV stream, magnet / .torrent, YouTube link).

use crate::runtime::{blocking, spawn_ui};
use crate::ui::{self, OpenState};
use engine::models::{ContentType, MediaItem, MediaType};
use slint::ComponentHandle;

fn os<R>(f: impl FnOnce(&OpenState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<OpenState>())
}

pub fn init() {
    os(|o| {
        o.on_close(|| os(|o| o.set_open(false)));
        o.on_pick_file(|| {
            spawn_ui(async {
                let file = blocking(|| {
                    rfd::FileDialog::new()
                        .set_title("Open a video")
                        .add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m4v", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg"])
                        .add_filter("All files", &["*"])
                        .pick_file()
                })
                .await;
                if let Some(path) = file {
                    os(|o| o.set_open(false));
                    crate::open::play(local(&path.to_string_lossy()));
                }
            });
        });
        o.on_watch(|| {
            let text = os(|o| o.get_url()).trim().to_string();
            if text.is_empty() {
                return;
            }
            os(|o| {
                o.set_open(false);
                o.set_url("".into());
            });
            open_text(&text);
        });
    });
}

pub fn open() {
    os(|o| o.set_open(true));
}

/// A local video file as a media item.
pub fn local(path: &str) -> MediaItem {
    let p = std::path::Path::new(path);
    MediaItem {
        id: path.to_string(),
        url: path.to_string(),
        title: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        image: path.to_string(),
        media_type: MediaType::Local,
        content_type: ContentType::Movie,
        ..Default::default()
    }
}

/// Opens a pasted link, magnet or path.
pub fn open_text(text: &str) {
    let lower = text.to_lowercase();
    if engine::youtube::is_link(text) {
        let t = text.to_string();
        spawn_ui(async move {
            match crate::runtime::bg(async move { engine::youtube::resolve(&t).await }).await {
                Some(item) if item.kind == engine::youtube::Kind::Channel => {
                    crate::pages::youtube::show_channel(item.channel_id, item.title)
                }
                Some(item) => crate::open::play(item.to_media()),
                None => ui::toast("That YouTube link couldn't be opened"),
            }
        });
    } else if lower.starts_with("magnet:") || lower.ends_with(".torrent") {
        let title = magnet_name(text).unwrap_or_else(|| "Torrent".into());
        crate::open::play(MediaItem { url: text.to_string(), title, media_type: MediaType::Torrent, ..Default::default() });
    } else if std::path::Path::new(text).is_file() {
        crate::open::play(local(text));
    } else {
        let title = text.rsplit('/').next().unwrap_or(text).split('?').next().unwrap_or(text).to_string();
        let live = lower.contains(".m3u8") || lower.contains(".m3u");
        crate::open::play(MediaItem {
            url: text.to_string(),
            title,
            media_type: if live { MediaType::Unknown } else { MediaType::Web },
            ..Default::default()
        });
    }
}

/// The `dn=` display name of a magnet link.
fn magnet_name(url: &str) -> Option<String> {
    let dn = url.split(['?', '&']).find_map(|p| p.strip_prefix("dn="))?;
    urlencoding_decode(dn)
}

fn urlencoding_decode(s: &str) -> Option<String> {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}
