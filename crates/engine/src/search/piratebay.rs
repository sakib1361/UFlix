//! PirateBay proxy search (HTML result table).

use crate::db::db;
use crate::models::{Quality, TorrentInfo};
use regex::Regex;
use scraper::{Html, Selector};
use std::sync::OnceLock;

pub(super) async fn search(base: &str, query: &str, forced: bool) -> Vec<TorrentInfo> {
    let url = format!("{}/search/{}/1/99/0", base.trim_end_matches('/'), urlencoding::encode(query));
    if !forced && let Some(cached) = db().cache_get::<Vec<TorrentInfo>>(&url).filter(|c| !c.is_empty()) {
        return cached;
    }
    let Ok(html) = crate::http::get_text(&url).await else { return Vec::new() };
    let results = parse(&html);
    if !results.is_empty() {
        db().cache_put(&url, &results, 3);
    }
    results
}

fn parse(html: &str) -> Vec<TorrentInfo> {
    let doc = Html::parse_document(html);
    let sel = |s: &str| Selector::parse(s).unwrap();
    let (rows, title_sel, title_alt, magnet_sel, desc_sel, size_alt, right_sel) = (
        sel("#searchResult tbody tr"),
        sel("a.detLink"),
        sel("td:nth-child(2) > a"),
        sel("a[href^='magnet']"),
        sel("font.detDesc"),
        sel("td:nth-child(5)"),
        sel("td[align='right']"),
    );
    let text = |e: scraper::ElementRef| e.text().collect::<String>().trim().to_string();
    doc.select(&rows)
        .filter_map(|row| {
            let title = row.select(&title_sel).next().or_else(|| row.select(&title_alt).next()).map(text)?;
            let magnet = row.select(&magnet_sel).next()?.value().attr("href")?.to_string();
            let size_text = row.select(&desc_sel).next().or_else(|| row.select(&size_alt).next()).map(text).unwrap_or_default();
            let right: Vec<String> = row.select(&right_sel).map(text).collect();
            let (seeds, leech) = if right.len() > 2 { (1, 2) } else { (0, 1) };
            Some(TorrentInfo {
                quality: Quality::parse(&title),
                title,
                url: magnet,
                size: parse_size(&size_text),
                seeders: right.get(seeds).cloned().unwrap_or_default(),
                leechers: right.get(leech).cloned().unwrap_or_default(),
                provider: "PirateBay".into(),
                ..Default::default()
            })
        })
        .collect()
}

fn parse_size(text: &str) -> String {
    static SIZE: OnceLock<Regex> = OnceLock::new();
    let re = SIZE.get_or_init(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)\s?(GiB|MiB|KiB)").unwrap());
    re.captures(text).map(|c| format!("{} {}", &c[1], c[2].replace("i", ""))).unwrap_or_default()
}
