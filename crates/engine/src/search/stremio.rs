//! Stremio torrent addons (Torrentio, TorrentsDB): `<base>/stream/<movie|series>/<imdb>[:s:e].json`.

use crate::http::get_json_cached_as;
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
    /// The file in the torrent (an episode in a season pack).
    file_idx: Option<usize>,
    behavior_hints: BehaviorHints,
    /// Where to find peers: "tracker:<announce url>" and "dht:<hash>".
    sources: Vec<String>,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct BehaviorHints {
    filename: Option<String>,
}

pub(super) async fn search(provider: &str, base: &str, path: &str, forced: bool) -> Vec<TorrentInfo> {
    let url = format!("{}/stream/{path}.json", base.trim_end_matches('/'));
    // "#2": entries cached before the file and tracker fields were kept would lack them
    let resp: Response = get_json_cached_as(&url, &format!("{url}#2"), forced, 3).await.unwrap_or_default();
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
                url: magnet(&hash, &s.sources),
                file_name: s.behavior_hints.filename.filter(|f| !f.is_empty()),
                file_index: s.file_idx,
                ..Default::default()
            })
        })
        .collect()
}

/// A magnet with the stream's trackers. A bare hash leaves only the DHT to find peers (a magnet's metadata carries no
/// trackers), which is slow to start and often finds few.
fn magnet(hash: &str, sources: &[String]) -> String {
    let mut url = format!("magnet:?xt=urn:btih:{hash}");
    for tracker in sources.iter().filter_map(|s| s.strip_prefix("tracker:")) {
        url.push_str("&tr=");
        url.push_str(&urlencoding::encode(tracker));
    }
    url
}

#[cfg(test)]
mod tests {
    #[test]
    fn magnet_keeps_trackers_only() {
        let sources = ["tracker:udp://tracker.opentrackr.org:1337/announce".to_string(), "dht:abcd".to_string()];
        assert_eq!(super::magnet("abcd", &sources), "magnet:?xt=urn:btih:abcd&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce");
        assert_eq!(super::magnet("abcd", &[]), "magnet:?xt=urn:btih:abcd");
    }
}
