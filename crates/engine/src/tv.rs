//! IPTV channels from M3U playlists (Settings → Live TV; iptv-org's Bangladesh list by default).
//!
//! Entries that differ only by a quality tag ("News 24 (720p)", "News 24 (1080p)") become one channel with several
//! streams; [`probe_all`] checks which streams answer.

use crate::db::db;
use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

pub const DEFAULT_LINK: &str = "https://iptv-org.github.io/iptv/countries/bd.m3u";
/// Streams probed at once.
const PROBE_PARALLEL: usize = 10;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const RECENT_KEY: &str = "TvRecent";
const RECENT_MAX: usize = 5;

/// One `#EXTINF` entry of a playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub logo: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    /// The quality and tags stripped from the name ("720p", "1080p · Geo-blocked"), or "" when there were none.
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: String,
    pub logo: String,
    pub streams: Vec<Stream>,
}

/// A channel played from the Live TV page (kept apart from Home's "Continue watching").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recent {
    pub name: String,
    pub logo: String,
    /// The stream's label and URL, so it replays the same stream.
    pub label: String,
    pub url: String,
}

/// The last channels played, newest first.
pub fn recent() -> Vec<Recent> {
    db().get::<Vec<Recent>>(RECENT_KEY).unwrap_or_default()
}

/// Records a played channel: one entry per channel name, at most [`RECENT_MAX`].
pub fn add_recent(entry: Recent) {
    let key = entry.name.to_lowercase();
    let mut list: Vec<Recent> = recent().into_iter().filter(|r| r.name.to_lowercase() != key).collect();
    list.insert(0, entry);
    list.truncate(RECENT_MAX);
    db().set(RECENT_KEY, &list);
}

pub fn remove_recent(index: usize) {
    let mut list = recent();
    if index < list.len() {
        list.remove(index);
        db().set(RECENT_KEY, &list);
    }
}

/// The playlist URLs from settings (stored comma-separated).
pub fn links() -> Vec<String> {
    split_links(&crate::settings::tv_links())
}

pub fn split_links(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Forgets the cached playlists so the next [`channels`] downloads them again.
pub fn clear_cache() {
    for link in links() {
        db().cache_put::<Option<String>>(&link, &None, -1);
    }
}

/// Every channel of every configured playlist (or the default one), grouped by name and sorted.
pub async fn channels() -> Vec<Channel> {
    let mut links = links();
    if links.is_empty() {
        links.push(DEFAULT_LINK.to_string());
    }
    let texts = futures::future::join_all(links.iter().map(|l| fetch(l))).await;
    let entries: Vec<Entry> = texts.into_iter().flatten().flat_map(|t| parse(&t)).collect();
    group(entries)
}

async fn fetch(url: &str) -> Option<String> {
    if let Some(text) = db().cache_get_fresh::<String>(url) {
        return Some(text);
    }
    let text = crate::http::get_text(url).await.ok()?;
    db().cache_put(url, &text, 3);
    Some(text)
}

/// `#EXTINF:-1 tvg-logo="…",Name`, then (after any `#EXTVLCOPT`-style lines) the stream URL.
pub fn parse(data: &str) -> Vec<Entry> {
    static LOGO: OnceLock<Regex> = OnceLock::new();
    let logo_re = LOGO.get_or_init(|| Regex::new(r#"tvg-logo="([^"]*)""#).unwrap());
    let mut out = Vec::new();
    let mut pending: Option<(String, String)> = None;
    for line in data.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.to_ascii_uppercase().starts_with("#EXTINF") {
            // the name follows the first comma after the attributes, whose quoted values may hold commas
            let after_attrs = line.rfind('"').map_or(0, |q| q + 1);
            let name = line[after_attrs..].split_once(',').map(|(_, n)| n.trim().to_string()).unwrap_or_default();
            let logo = logo_re.captures(line).map(|c| c[1].to_string()).unwrap_or_default();
            pending = Some((name, logo));
        } else if line.starts_with('#') {
            continue;
        } else if let Some((name, logo)) = pending.take()
            && line.contains("://")
        {
            out.push(Entry { name, logo, url: line.to_string() });
        }
    }
    out
}

/// Splits "News 24 (720p) [Geo-blocked]" into ("News 24", "720p · Geo-blocked"). Only quality-like groups and
/// `[…]` tags are stripped, so "Channel S (Bangladesh)" keeps its country.
pub fn split_quality(name: &str) -> (String, String) {
    static TAIL: OnceLock<Regex> = OnceLock::new();
    let re =
        TAIL.get_or_init(|| Regex::new(r"(?i)\s*(\[[^\]]*\]|\((?:\d{3,4}[pi]|4k|8k|uhd|fhd|hd|sd|hq|lq|backup|alt|\d+)\))\s*$").unwrap());
    let mut base = name.trim().to_string();
    let mut tags = Vec::new();
    while let Some(m) = re.captures(&base) {
        let whole = m.get(0).unwrap();
        tags.push(m[1].trim_matches(|c| matches!(c, '(' | ')' | '[' | ']')).to_string());
        base.truncate(whole.start());
    }
    if base.trim().is_empty() {
        return (name.trim().to_string(), String::new());
    }
    tags.reverse();
    (base.trim().to_string(), tags.join(" · "))
}

/// Merges entries with the same base name (case-insensitive) into channels, keeping playlist order for streams.
pub fn group(entries: Vec<Entry>) -> Vec<Channel> {
    let mut out: Vec<Channel> = Vec::new();
    let mut index = std::collections::HashMap::<String, usize>::new();
    for e in entries {
        let (name, label) = split_quality(&e.name);
        let key = name.to_lowercase();
        let i = *index.entry(key).or_insert_with(|| {
            out.push(Channel { name: name.clone(), logo: String::new(), streams: Vec::new() });
            out.len() - 1
        });
        let ch = &mut out[i];
        if ch.logo.is_empty() {
            ch.logo = e.logo;
        }
        if !ch.streams.iter().any(|s| s.url == e.url) {
            ch.streams.push(Stream { label, url: e.url });
        }
    }
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// Whether a stream answers: a 2xx response whose first bytes arrive in time (an HLS playlist must start with
/// `#EXTM3U`). Only the first chunk is read, so endless TS streams are fine.
pub async fn probe(url: &str) -> bool {
    let attempt = async {
        let mut resp = crate::http::client().get(url).timeout(PROBE_TIMEOUT).send().await.ok()?;
        if !resp.status().is_success() {
            return Some(false);
        }
        let chunk = resp.chunk().await.ok()??;
        let is_html =
            resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
        let hls = url.to_ascii_lowercase().contains(".m3u8");
        Some(!chunk.is_empty() && !is_html && (!hls || String::from_utf8_lossy(&chunk).trim_start().starts_with("#EXTM3U")))
    };
    tokio::time::timeout(PROBE_TIMEOUT, attempt).await.ok().flatten().unwrap_or(false)
}

/// Probes `urls`, [`PROBE_PARALLEL`] at a time, reporting `(index, works)` as each finishes.
pub async fn probe_all(urls: Vec<String>, on_result: impl Fn(usize, bool)) {
    futures::stream::iter(urls.into_iter().enumerate())
        .map(|(i, url)| async move { (i, probe(&url).await) })
        .buffer_unordered(PROBE_PARALLEL)
        .for_each(|(i, ok)| {
            on_result(i, ok);
            async {}
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quality() {
        assert_eq!(split_quality("ATN Bangla (720p)"), ("ATN Bangla".into(), "720p".into()));
        assert_eq!(split_quality("Somoy TV (1080p) [Geo-blocked]"), ("Somoy TV".into(), "1080p · Geo-blocked".into()));
        assert_eq!(split_quality("Channel S (Bangladesh) (720p)"), ("Channel S (Bangladesh)".into(), "720p".into()));
        assert_eq!(split_quality("Azan TV"), ("Azan TV".into(), String::new()));
        assert_eq!(split_quality("(720p)"), ("(720p)".into(), String::new()));
    }

    #[test]
    fn groups_by_name() {
        let data = "#EXTM3U\n#EXTINF:-1 tvg-logo=\"a.png\",News 24 (720p)\nhttp://a/1.m3u8\n\
                    #EXTINF:-1,news 24 (1080p)\n#EXTVLCOPT:http-referrer=x\nhttp://a/2.m3u8\n\
                    #EXTINF:-1 tvg-name=\"x, y\",Other, Live\nhttp://b/1.m3u8\n";
        let ch = group(parse(data));
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[1].name, "Other, Live");
        assert_eq!(ch[0].name, "News 24");
        assert_eq!(ch[0].logo, "a.png");
        assert_eq!(ch[0].streams.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["720p", "1080p"]);
    }
}
