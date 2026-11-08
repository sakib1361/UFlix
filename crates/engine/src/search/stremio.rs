//! Stremio torrent addons (Torrentio, TorrentsDB): `<base>/stream/<movie|series>/<imdb>[:s:e].json`.

use crate::http::get_json_cached;
use crate::models::{Quality, TorrentInfo};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct Response {
    streams: Vec<Stream>,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Stream {
    title: Option<String>,
    description: Option<String>,
    info_hash: Option<String>,
}

pub(super) async fn search(provider: &str, base: &str, path: &str, forced: bool) -> Vec<TorrentInfo> {
    let url = format!("{}/stream/{path}.json", base.trim_end_matches('/'));
    let resp: Response = get_json_cached(&url, forced, 3).await.unwrap_or_default();
    static SIZE: OnceLock<Regex> = OnceLock::new();
    static SEEDS: OnceLock<Regex> = OnceLock::new();
    let size_re = SIZE.get_or_init(|| Regex::new(r"💾\s*([\d.]+\s*[KMGT]?B)").unwrap());
    let seed_re = SEEDS.get_or_init(|| Regex::new(r"👤\s*(\d+)").unwrap());
    resp.streams
        .into_iter()
        .filter_map(|s| {
            let text = s.title.or(s.description).filter(|t| !t.is_empty())?;
            let hash = s.info_hash.filter(|h| !h.is_empty())?;
            Some(TorrentInfo {
                quality: Quality::parse(&text),
                title: text.lines().next().unwrap_or_default().trim().to_string(),
                size: size_re.captures(&text).map(|c| c[1].to_string()).unwrap_or_default(),
                seeders: seed_re.captures(&text).map(|c| c[1].to_string()).unwrap_or_default(),
                leechers: "0".into(),
                provider: provider.to_string(),
                url: format!("magnet:?xt=urn:btih:{hash}"),
                ..Default::default()
            })
        })
        .collect()
}
