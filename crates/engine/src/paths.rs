//! Per-OS folders (same locations as the C# app): data in `Documents\UFlix` on Windows and `$XDG_DATA_HOME/UFlix` on
//! Linux; downloads in the data folder on Windows and `XDG_DOWNLOAD_DIR/UFlix` on Linux.

use crate::APP_NAME;
use directories::{BaseDirs, UserDirs};
use std::path::PathBuf;

fn home() -> PathBuf {
    BaseDirs::new().map(|b| b.home_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_folder() -> PathBuf {
    let root = if cfg!(windows) {
        UserDirs::new().and_then(|u| u.document_dir().map(|d| d.to_path_buf())).unwrap_or_else(home)
    } else {
        BaseDirs::new().map(|b| b.data_dir().to_path_buf()).unwrap_or_else(|| home().join(".local/share"))
    };
    root.join(APP_NAME)
}

pub fn download_folder() -> PathBuf {
    if cfg!(windows) {
        return data_folder();
    }
    UserDirs::new().and_then(|u| u.download_dir().map(|d| d.to_path_buf())).unwrap_or_else(|| home().join("Downloads")).join(APP_NAME)
}

/// Where torrents download: the Settings path, else `<downloads>/Torrents`.
pub fn torrent_folder() -> PathBuf {
    let custom = crate::settings::torrent_download_path();
    let base = if custom.trim().is_empty() { download_folder() } else { PathBuf::from(custom) };
    base.join("Torrents")
}

/// Where YouTube downloads go: the Settings path, else `<downloads>/Youtube`.
pub fn youtube_folder() -> PathBuf {
    let custom = crate::settings::youtube_download_path();
    if custom.trim().is_empty() { download_folder().join("Youtube") } else { PathBuf::from(custom) }
}

/// yt-dlp and ffmpeg live here.
pub fn tools_folder() -> PathBuf {
    data_folder().join("YT")
}

pub fn thumbs_folder() -> PathBuf {
    data_folder().join("Thumbs")
}

pub fn cache_folder() -> PathBuf {
    data_folder().join("Cache")
}

/// The torrent session's resume state.
pub fn torrent_state_folder() -> PathBuf {
    data_folder().join("TorrentSession")
}

pub fn temp_folder() -> PathBuf {
    std::env::temp_dir().join(APP_NAME)
}
