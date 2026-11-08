//! YouTube for the YouTube page and the player: search feeds (interleaved, de-duplicated, paged on demand), channels,
//! playlists, video details with "up next", link resolving, suggestions, streams and captions. Data comes from
//! rustypipe (YouTube's InnerTube API); yt-dlp is the other stream source (Settings → "yt-dlp first").

mod feed;
mod streams;

pub use feed::Feed;
pub use streams::{Caption, StreamOption, StreamSet, streams, streams_ytdlp};

use crate::models::{ContentType, MediaItem, MediaType};
use rustypipe::client::RustyPipe;
use rustypipe::model::{Thumbnail, UrlTarget, VideoItem, YouTubeItem};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const TOPICS: [&str; 10] = ["Music", "Gaming", "News", "Live", "Podcasts", "Trailers", "Sports", "Technology", "Science", "Cooking"];

pub(crate) fn rp() -> &'static RustyPipe {
    static C: OnceLock<RustyPipe> = OnceLock::new();
    C.get_or_init(|| {
        let dir = crate::paths::data_folder().join("rustypipe");
        let _ = std::fs::create_dir_all(&dir);
        RustyPipe::builder().storage_dir(dir).build().unwrap_or_else(|_| RustyPipe::new())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Video,
    Playlist,
    Channel,
}

/// The search filter chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Videos,
    Playlists,
    Channels,
}

/// One tile on the YouTube page.
#[derive(Clone, Debug)]
pub struct YtItem {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub author: String,
    pub channel_id: String,
    pub thumbnail: String,
    /// Seconds; None for live videos.
    pub duration: Option<u32>,
    /// "1.2M views · 3 days ago" when known.
    pub meta: String,
    /// 0..1 for continue-watching tiles.
    pub progress: f64,
    pub source: Option<MediaItem>,
}

impl YtItem {
    /// "12:34", "LIVE", "Playlist", or "" for channels.
    pub fn badge(&self) -> String {
        match self.kind {
            Kind::Video => format_duration(self.duration),
            Kind::Playlist => "Playlist".into(),
            Kind::Channel => String::new(),
        }
    }

    pub fn to_media(&self) -> MediaItem {
        if let Some(m) = &self.source {
            return m.clone();
        }
        MediaItem {
            id: self.id.clone(),
            url: self.id.clone(),
            title: self.title.clone(),
            author: self.author.clone(),
            duration: self.duration.map(f64::from),
            image: if self.kind == Kind::Video { thumb(&self.id, "hqdefault") } else { self.thumbnail.clone() },
            media_type: if self.kind == Kind::Playlist { MediaType::PlayList } else { MediaType::Youtube },
            content_type: ContentType::Youtube,
            ..Default::default()
        }
    }
}

pub fn thumb(video_id: &str, size: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/{size}.jpg")
}

fn best_thumb(thumbs: &[Thumbnail], width: u32) -> String {
    let t = thumbs.iter().filter(|t| t.width >= width).min_by_key(|t| t.width).or_else(|| thumbs.iter().max_by_key(|t| t.width));
    t.map(|t| if t.url.starts_with("//") { format!("https:{}", t.url) } else { t.url.clone() }).unwrap_or_default()
}

pub(crate) fn from_video(v: VideoItem) -> YtItem {
    let mut meta = Vec::new();
    if let Some(views) = v.view_count {
        meta.push(format!("{} views", format_count(views)));
    }
    if let Some(age) = v.publish_date_txt.clone() {
        meta.push(age);
    }
    YtItem {
        thumbnail: thumb(&v.id, "mqdefault"),
        id: v.id,
        kind: Kind::Video,
        title: v.name,
        author: v.channel.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
        channel_id: v.channel.as_ref().map(|c| c.id.clone()).unwrap_or_default(),
        duration: v.duration,
        meta: meta.join(" · "),
        progress: 0.0,
        source: None,
    }
}

pub(crate) fn from_item(item: YouTubeItem) -> YtItem {
    match item {
        YouTubeItem::Video(v) => from_video(v),
        YouTubeItem::Playlist(p) => YtItem {
            meta: p.video_count.map(|c| format!("{c} videos")).unwrap_or_default(),
            thumbnail: best_thumb(&p.thumbnail, 480),
            id: p.id,
            kind: Kind::Playlist,
            title: p.name,
            author: p.channel.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
            channel_id: p.channel.as_ref().map(|c| c.id.clone()).unwrap_or_default(),
            duration: None,
            progress: 0.0,
            source: None,
        },
        YouTubeItem::Channel(c) => YtItem {
            meta: c.subscriber_count.map(|s| format!("{} subscribers", format_count(s))).unwrap_or_default(),
            thumbnail: best_thumb(&c.avatar, 176),
            channel_id: c.id.clone(),
            id: c.id,
            kind: Kind::Channel,
            title: c.name,
            author: "Channel".into(),
            duration: None,
            progress: 0.0,
            source: None,
        },
    }
}

/// Continue-watching tiles: videos and playlists from the shared watchlist.
pub fn continue_watching() -> Vec<YtItem> {
    crate::watchlist::items()
        .into_iter()
        .filter(|m| matches!(m.media_type, MediaType::Youtube | MediaType::PlayList) && !m.url.is_empty())
        .take(20)
        .map(|m| YtItem {
            id: m.url.clone(),
            kind: if m.media_type == MediaType::PlayList { Kind::Playlist } else { Kind::Video },
            title: m.title.clone(),
            author: m.author.clone(),
            channel_id: String::new(),
            thumbnail: if m.media_type == MediaType::Youtube { thumb(&m.url, "mqdefault") } else { m.image.clone() },
            duration: m.duration.map(|d| d as u32),
            meta: String::new(),
            progress: m.progress(),
            source: Some(m),
        })
        .collect()
}

/// Details for the player's side panel.
#[derive(Clone, Debug, Default)]
pub struct Details {
    pub title: String,
    pub author: String,
    pub channel_id: String,
    pub avatar: String,
    pub description: String,
    /// "1.2M views · 3 days ago · 45K likes"
    pub stats: String,
    pub related: Vec<YtItem>,
}

pub async fn details(video_id: &str) -> anyhow::Result<Details> {
    static CACHE: OnceLock<Mutex<HashMap<String, Details>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(d) = cache.lock().unwrap().get(video_id) {
        return Ok(d.clone());
    }
    use rustypipe::model::richtext::ToPlaintext;
    let v = rp().query().video_details(video_id).await?;
    let mut stats = vec![format!("{} views", format_count(v.view_count))];
    if let Some(d) = &v.publish_date_txt {
        stats.push(d.clone());
    }
    if let Some(l) = v.like_count {
        stats.push(format!("{} likes", format_count(l as u64)));
    }
    let d = Details {
        title: v.name,
        author: v.channel.name.clone(),
        channel_id: v.channel.id.clone(),
        avatar: best_thumb(&v.channel.avatar, 88),
        description: v.description.to_plaintext(),
        stats: stats.join("  ·  "),
        related: v.recommended.items.into_iter().map(from_video).filter(|r| r.id != video_id).collect(),
    };
    cache.lock().unwrap().insert(video_id.to_string(), d.clone());
    Ok(d)
}

#[derive(Clone, Debug, Default)]
pub struct ChannelInfo {
    pub id: String,
    pub title: String,
    pub avatar: String,
    pub subscribers: String,
}

pub async fn channel(channel_id: &str) -> anyhow::Result<ChannelInfo> {
    let c = rp().query().channel_videos(channel_id).await?;
    Ok(ChannelInfo {
        id: c.id,
        title: c.name,
        avatar: best_thumb(&c.avatar, 176),
        subscribers: c.subscriber_count.map(|s| format!("{} subscribers", format_count(s))).unwrap_or_default(),
    })
}

/// The videos of a playlist (all pages when `all`, else the first ~100).
pub async fn playlist_items(playlist_id: &str, all: bool) -> anyhow::Result<Vec<MediaItem>> {
    let key = format!("yt_playlist_{playlist_id}_{all}");
    if let Some(items) = crate::db::db().cache_get::<Vec<MediaItem>>(&key) {
        return Ok(items);
    }
    let mut p = rp().query().playlist(playlist_id).await?;
    if all {
        p.videos.extend_all(rp().query()).await?;
    } else {
        p.videos.extend_limit(rp().query(), 100).await?;
    }
    let items: Vec<MediaItem> = p.videos.items.into_iter().map(|v| from_video(v).to_media()).collect();
    if !items.is_empty() {
        crate::db::db().cache_put(&key, &items, 7);
    }
    Ok(items)
}

/// Resolves a pasted link or id: a video, a playlist or a channel (including @handles).
pub async fn resolve(text: &str) -> Option<YtItem> {
    let target = rp().query().resolve_string(text.trim(), false).await.ok()?;
    match target {
        UrlTarget::Video { id, .. } => {
            let d = details(&id).await.ok();
            Some(YtItem {
                thumbnail: thumb(&id, "mqdefault"),
                title: d.as_ref().map(|d| d.title.clone()).unwrap_or_else(|| id.clone()),
                author: d.as_ref().map(|d| d.author.clone()).unwrap_or_default(),
                channel_id: d.map(|d| d.channel_id).unwrap_or_default(),
                id,
                kind: Kind::Video,
                duration: None,
                meta: String::new(),
                progress: 0.0,
                source: None,
            })
        }
        UrlTarget::Playlist { id } => {
            let p = rp().query().playlist(&id).await.ok()?;
            Some(YtItem {
                id,
                kind: Kind::Playlist,
                title: p.name,
                author: p.channel.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
                channel_id: p.channel.map(|c| c.id).unwrap_or_default(),
                thumbnail: best_thumb(&p.thumbnail, 480),
                duration: None,
                meta: format!("{} videos", p.video_count),
                progress: 0.0,
                source: None,
            })
        }
        UrlTarget::Channel { id } => {
            let c = channel(&id).await.ok()?;
            Some(YtItem {
                id: c.id.clone(),
                kind: Kind::Channel,
                title: c.title,
                author: "Channel".into(),
                channel_id: c.id,
                thumbnail: c.avatar,
                duration: None,
                meta: c.subscribers,
                progress: 0.0,
                source: None,
            })
        }
        _ => None,
    }
}

/// True when `text` looks like a YouTube link rather than a search.
pub fn is_link(text: &str) -> bool {
    let t = text.trim();
    t.contains("youtube.com/") || t.contains("youtu.be/") || t.starts_with('@')
}

pub async fn suggestions(query: &str) -> Vec<String> {
    if query.trim().len() < 2 {
        return Vec::new();
    }
    rp().query().search_suggestion(query).await.unwrap_or_default().into_iter().take(8).collect()
}

pub fn format_duration(seconds: Option<u32>) -> String {
    match seconds {
        None => "LIVE".into(),
        Some(s) if s >= 3600 => format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
        Some(s) => format!("{}:{:02}", s / 60, s % 60),
    }
}

pub fn format_count(n: u64) -> String {
    let f = |v: f64, suffix: &str| {
        let s = format!("{v:.1}");
        format!("{}{suffix}", s.trim_end_matches(".0"))
    };
    match n {
        1_000_000_000.. => f(n as f64 / 1e9, "B"),
        1_000_000.. => f(n as f64 / 1e6, "M"),
        1_000.. => f(n as f64 / 1e3, "K"),
        _ => n.to_string(),
    }
}
