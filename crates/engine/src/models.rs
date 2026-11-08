//! The data the app passes around.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    #[default]
    Torrent,
    Local,
    Youtube,
    PlayList,
    Web,
    /// IPTV channels: never saved to the watchlist.
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    #[default]
    Movie,
    Shows,
    Youtube,
}

/// Anything the player can open: a library title, a file, a torrent, a YouTube video or playlist, a channel.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaItem {
    /// IMDb id for library titles, a YouTube id, or a path.
    pub id: String,
    pub tmdb_id: i64,
    /// What the player opens: a path, URL, magnet or YouTube id.
    pub url: String,
    pub title: String,
    pub description: String,
    pub image: String,
    pub backdrop: String,
    pub author: String,
    pub year: String,
    pub media_type: MediaType,
    pub content_type: ContentType,
    /// Seconds.
    pub duration: Option<f64>,
    /// Last position in seconds.
    pub position: f64,
    pub rating: f64,
    pub view_count: String,
    pub playlist_index: usize,
    pub last_played: Option<DateTime<Utc>>,
    /// The torrent file to play when the url is a magnet (its path inside the torrent).
    pub torrent_file: Option<String>,
}

impl MediaItem {
    /// 0..1 watched, for progress bars.
    pub fn progress(&self) -> f64 {
        match self.duration {
            Some(d) if d > 0.0 => (self.position / d).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    /// The key the watchlist matches on.
    pub fn key(&self) -> String {
        if self.id.is_empty() { self.url.clone() } else { format!("{}|{}", self.id, self.title) }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quality {
    Q2160p,
    Q1440p,
    Q1080p,
    Q720p,
    #[default]
    Other,
}

impl Quality {
    pub const ALL: [Quality; 5] = [Quality::Q2160p, Quality::Q1440p, Quality::Q1080p, Quality::Q720p, Quality::Other];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Q2160p => "4K",
            Quality::Q1440p => "1440p",
            Quality::Q1080p => "1080p",
            Quality::Q720p => "720p",
            Quality::Other => "Other",
        }
    }

    pub fn parse(text: &str) -> Self {
        let t = text.to_ascii_lowercase();
        if t.contains("2160") || t.contains("4k") {
            Quality::Q2160p
        } else if t.contains("1440") {
            Quality::Q1440p
        } else if t.contains("1080") {
            Quality::Q1080p
        } else if t.contains("720") {
            Quality::Q720p
        } else {
            Quality::Other
        }
    }
}

/// One search result: a torrent (magnet or .torrent URL) or a local file found by the local search.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TorrentInfo {
    pub title: String,
    pub url: String,
    pub seeders: String,
    pub leechers: String,
    pub quality: Quality,
    pub media_type: MediaType,
    pub provider: String,
    pub size: String,
}

impl TorrentInfo {
    pub fn description(&self) -> String {
        let mut parts = vec![self.provider.clone()];
        if !self.size.is_empty() {
            parts.push(self.size.clone());
        }
        if !self.seeders.is_empty() {
            parts.push(format!("{} seeds", self.seeders));
        }
        parts.retain(|p| !p.is_empty());
        parts.join(" · ")
    }
}

/// A user-set URL for a torrent search engine.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TorrentSource {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TorrentKind {
    /// Streamed for the player; removed when the player closes before it completes.
    #[default]
    Play,
    /// Kept until complete.
    Download,
}

/// A torrent kept in the session, saved so unfinished ones come back at the next start.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackedTorrent {
    pub url: String,
    pub title: String,
    pub kind: TorrentKind,
    /// 0..100
    pub progress: f64,
    pub save_path: String,
    pub added: Option<DateTime<Utc>>,
}

const VIDEO_EXTS: &[&str] = &[
    "mp4", "m4v", "mkv", "mpg", "mpeg", "mpe", "m2ts", "m2v", "mts", "ts", "vob", "avi", "mov", "flv", "wmv", "webm", "ogm", "ogv", "3gp",
    "3g2", "f4v", "asf", "rm", "divx", "xvid", "qt",
];

/// A video file the player can open.
pub fn is_video(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| VIDEO_EXTS.iter().any(|v| v.eq_ignore_ascii_case(e)))
}
