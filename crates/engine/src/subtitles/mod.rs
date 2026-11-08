//! Subtitles: online search (OpenSubtitles, SubDL; chosen in Settings) and finding the files that belong to a video.

pub mod finder;
mod opensubtitles;
mod subdl;

use crate::models::MediaItem;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    OpenSubtitles,
    SubDl,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::OpenSubtitles, Provider::SubDl];

    /// The name saved in Settings (same as the C# app).
    pub fn key(self) -> &'static str {
        match self {
            Provider::OpenSubtitles => "OpenSubtitle",
            Provider::SubDl => "SubDL",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::OpenSubtitles => "OpenSubtitles",
            Provider::SubDl => "SubDL",
        }
    }

    /// The provider chosen in Settings (OpenSubtitles if the saved one no longer exists).
    pub fn current() -> Provider {
        let saved = crate::settings::subtitle_provider();
        Self::ALL.into_iter().find(|p| p.key() == saved).unwrap_or(Provider::OpenSubtitles)
    }
}

/// One subtitle found online.
#[derive(Clone, Debug)]
pub struct SubtitleResult {
    pub name: String,
    pub release: String,
    pub description: String,
    download: Download,
}

#[derive(Clone, Debug)]
enum Download {
    OpenSubtitles { file_id: i64 },
    SubDl { url: String, season: Option<i32>, episode: Option<i32> },
}

pub async fn search(media: &MediaItem) -> Vec<SubtitleResult> {
    match Provider::current() {
        Provider::OpenSubtitles => opensubtitles::search(media).await,
        Provider::SubDl => subdl::search(media).await,
    }
}

/// Downloads a result to a local .srt/.ass file for mpv's sub-add.
pub async fn download(result: &SubtitleResult) -> Option<PathBuf> {
    match &result.download {
        Download::OpenSubtitles { file_id } => opensubtitles::download(*file_id).await,
        Download::SubDl { url, season, episode } => subdl::download(url, *season, *episode).await,
    }
}

/// A real IMDb id ("tt1234567"); local files carry their path in `id`.
fn imdb_id(media: &MediaItem) -> Option<&str> {
    let id = media.id.as_str();
    (id.len() > 2 && id.starts_with("tt") && id[2..].chars().all(|c| c.is_ascii_digit())).then_some(id)
}
