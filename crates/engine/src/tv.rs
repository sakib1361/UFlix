//! IPTV channels from M3U playlists (Settings → TV links; iptv-org's Bangladesh list by default).

use crate::db::db;
use crate::models::{ContentType, MediaItem, MediaType};
use regex::Regex;
use std::sync::OnceLock;

const DEFAULT_LINK: &str = "https://iptv-org.github.io/iptv/countries/bd.m3u";

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: String,
    pub logo: String,
    pub url: String,
}

/// Every channel of every configured playlist, sorted by name.
pub async fn channels() -> Vec<MediaItem> {
    let links = crate::settings::tv_links();
    let links: Vec<String> = links.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let links = if links.is_empty() { vec![DEFAULT_LINK.to_string()] } else { links };
    let mut all = Vec::new();
    for link in links {
        let Some(text) = fetch(&link).await else { continue };
        all.extend(parse(&text).into_iter().map(|c| MediaItem {
            title: c.name,
            image: c.logo,
            url: c.url,
            media_type: MediaType::Unknown,
            content_type: ContentType::Movie,
            ..Default::default()
        }));
    }
    all.sort_by_key(|m| m.title.to_lowercase());
    all
}

async fn fetch(url: &str) -> Option<String> {
    if let Some(text) = db().cache_get_fresh::<String>(url) {
        return Some(text);
    }
    let text = crate::http::get_text(url).await.ok()?;
    db().cache_put(url, &text, 3);
    Some(text)
}

/// `#EXTINF:-1 tvg-logo="…",Name` followed by the stream URL.
pub fn parse(data: &str) -> Vec<Channel> {
    static LOGO: OnceLock<Regex> = OnceLock::new();
    static NAME: OnceLock<Regex> = OnceLock::new();
    let logo_re = LOGO.get_or_init(|| Regex::new(r#"tvg-logo="([^"]*)""#).unwrap());
    let name_re = NAME.get_or_init(|| Regex::new(r",(.+)$").unwrap());
    let lines: Vec<&str> = data.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let line = lines[i];
        if line.to_ascii_uppercase().starts_with("#EXTINF") && lines[i + 1].to_ascii_lowercase().starts_with("http") {
            out.push(Channel {
                name: name_re.captures(line).map(|c| c[1].trim().to_string()).unwrap_or_default(),
                logo: logo_re.captures(line).map(|c| c[1].to_string()).unwrap_or_default(),
                url: lines[i + 1].to_string(),
            });
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}
