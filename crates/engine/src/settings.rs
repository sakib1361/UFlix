//! Typed user settings in the permanent store. Each setting is a getter plus a `set_` function.

use crate::db::db;
use crate::models::{TorrentSource, TrackedTorrent};

macro_rules! setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal, $ty:ty, $default:expr) => {
        $(#[$doc])*
        pub fn $get() -> $ty {
            db().get::<$ty>($key).unwrap_or_else(|| $default)
        }
        pub fn $set(value: $ty) {
            db().set($key, &value)
        }
    };
}

setting!(volume, set_volume, "Volume", f64, 100.0);
setting!(
    /// Size cap of the torrent folder in GB; 0 = no cap.
    max_folder_size, set_max_folder_size, "MaxFolderSize", i64, 0);
setting!(torrent_download_path, set_torrent_download_path, "TorrentDownloadPath", String, String::new());
setting!(youtube_download_path, set_youtube_download_path, "YoutubeDownloadPath", String, String::new());
setting!(subtitle_provider, set_subtitle_provider, "SubtitleProvider", String, "OpenSubtitle".into());
setting!(last_search, set_last_search, "LastSearch", String, String::new());
setting!(torrent_sources, set_torrent_sources, "TorrentSources", Vec<TorrentSource>, Vec::new());
setting!(
    /// Comma-separated M3U playlist URLs for the TV page.
    tv_links, set_tv_links, "TvLinks", String, String::new());
setting!(
    /// Play YouTube through yt-dlp first (else the built-in client first).
    use_ytdl_first, set_use_ytdl_first, "UseYtDLFirst", bool, true);
setting!(tracked_torrents, set_tracked_torrents, "TrackedTorrents", Vec<TrackedTorrent>, Vec::new());
setting!(youtube_search_history, set_youtube_search_history, "YtSearchHistory", Vec<String>, Vec::new());
setting!(subtitle_font_size, set_subtitle_font_size, "SubtitleFontSize", f64, 46.0);
setting!(subtitle_color, set_subtitle_color, "SubtitleColor", String, "#FFFFFFFF".into());
setting!(subtitle_font, set_subtitle_font, "SubtitleFont", String, String::new());
setting!(check_updates_daily, set_check_updates_daily, "CheckUpdatesDaily", bool, true);
setting!(
    /// Video decoder: "auto" (mpv's auto-safe) or an mpv hwdec value (nvdec, nvdec-copy, vaapi, ..., "no").
    video_decoder, set_video_decoder, "VideoDecoder", String, "auto".into());
setting!(
    /// mpv's GPU interops seen on this machine ("vaapi,cuda"), so Settings can offer only decoders that can work.
    decoder_interops, set_decoder_interops, "DecoderInterops", String, String::new());
setting!(
    /// Autoplay the next YouTube video / playlist item.
    autoplay, set_autoplay, "Autoplay", bool, true);

/// Adds a YouTube search to the history that drives the "For you" feed (newest first, 12 kept).
pub fn add_youtube_search(query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    let mut history: Vec<String> = youtube_search_history().into_iter().filter(|q| !q.eq_ignore_ascii_case(query)).take(11).collect();
    history.insert(0, query.to_string());
    set_youtube_search_history(history);
}

pub fn remove_youtube_search(query: &str) {
    set_youtube_search_history(youtube_search_history().into_iter().filter(|q| q != query).collect());
}

/// Picture adjustments for dark films (mpv's brightness/contrast/saturation/gamma, -100..100; 0 = unchanged).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Picture {
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub gamma: f64,
}

setting!(picture, set_picture, "Picture", Picture, Picture::default());
setting!(
    /// Preferred YouTube height (0 = best); the quality menu sets it.
    youtube_height, set_youtube_height, "YoutubeHeight", u32, 1080);
