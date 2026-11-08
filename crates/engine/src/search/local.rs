//! Local "search": folders on disk named after the title (the engine's URL is a comma-separated folder list).

use crate::models::{MediaType, Quality, TorrentInfo};
use crate::parse::parse_name;
use std::path::{Path, PathBuf};

fn roots(urls: &str) -> Vec<PathBuf> {
    urls.split(',').map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).filter(|p| p.is_dir()).collect()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn local(title: String, path: &Path) -> TorrentInfo {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    TorrentInfo {
        quality: Quality::parse(&title),
        title,
        url: path.to_string_lossy().into_owned(),
        size: crate_size(size),
        seeders: "1".into(),
        leechers: "1".into(),
        media_type: MediaType::Local,
        provider: "Local".into(),
    }
}

fn crate_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", units[i])
}

/// The largest file of each folder whose name starts with the movie's title.
pub(super) fn movie(urls: &str, title: &str) -> Vec<TorrentInfo> {
    let wanted = parse_name(&title.replace(':', "")).title.to_lowercase();
    let mut out = Vec::new();
    for root in roots(urls) {
        for folder in subdirs(&root).into_iter().filter(|f| name_of(f).to_lowercase().starts_with(&wanted)) {
            let largest = std::fs::read_dir(&folder)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
            if let Some(file) = largest {
                out.push(local(name_of(&folder), &file));
            }
        }
    }
    out
}

/// Video files of the episode inside folders named after the show.
pub(super) fn episode(urls: &str, show: &str, season: i32, episode: i32) -> Vec<TorrentInfo> {
    let wanted = show.replace(':', "").to_lowercase();
    let mut out = Vec::new();
    for root in roots(urls) {
        for folder in subdirs(&root).into_iter().filter(|f| name_of(f).to_lowercase().starts_with(&wanted)) {
            let mut stack = vec![folder];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if crate::models::is_video(&p) {
                        let parsed = parse_name(&name_of(&p));
                        if parsed.season == Some(season) && parsed.episode == Some(episode) {
                            out.push(local(name_of(&p), &p));
                        }
                    }
                }
            }
        }
    }
    out
}
