//! subdl.com API (free key from a SubDL account, UFLIX_SUBDL_API_KEY). Each result is a zip; a season pack's episode is
//! picked out of it.

use super::{Download, SubtitleResult, finder::is_subtitle, imdb_id};
use crate::models::{ContentType, MediaItem};
use crate::parse::parse_name;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Default)]
#[serde(default)]
struct Response {
    status: bool,
    subtitles: Vec<Sub>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Sub {
    release_name: String,
    name: Option<String>,
    lang: String,
    author: Option<String>,
    url: Option<String>,
    hi: bool,
    full_season: bool,
}

pub(super) async fn search(media: &MediaItem) -> Vec<SubtitleResult> {
    let key = crate::secrets::subdl_api_key();
    if key.trim().is_empty() {
        return Vec::new();
    }
    let path = Path::new(&media.url);
    let is_file = path.is_file();
    let file_name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let parsed = parse_name(if is_file { &file_name } else { &media.title });
    let is_show = media.content_type == ContentType::Shows || parsed.episode.is_some();
    let mut q: Vec<(&str, String)> = vec![
        ("api_key", key),
        ("type", if is_show { "tv" } else { "movie" }.into()),
        ("languages", "EN".into()),
        ("subs_per_page", "30".into()),
    ];
    if let Some(id) = imdb_id(media) {
        q.push(("imdb_id", id.into()));
    } else if media.tmdb_id > 0 {
        q.push(("tmdb_id", media.tmdb_id.to_string()));
    } else {
        q.push(("film_name", parsed.title.clone()));
        if let (false, Some(y)) = (is_show, parsed.year) {
            q.push(("year", y.to_string()));
        }
    }
    if is_file {
        q.push(("file_name", file_name));
    }
    if is_show {
        if let Some(s) = parsed.season {
            q.push(("season_number", s.to_string()));
        }
        if let Some(e) = parsed.episode {
            q.push(("episode_number", e.to_string()));
        }
    }
    let Ok(resp) = crate::http::client().get("https://api.subdl.com/api/v1/subtitles").query(&q).send().await else { return Vec::new() };
    let data: Response = resp.json().await.unwrap_or_default();
    if !data.status {
        return Vec::new();
    }
    data.subtitles
        .into_iter()
        .filter_map(|s| {
            let url = s.url.filter(|u| !u.is_empty())?;
            let mut desc = format!("{} · by {}", s.lang, s.author.unwrap_or_default());
            if s.hi {
                desc.push_str(" · HI");
            }
            if s.full_season {
                desc.push_str(" · full season");
            }
            Some(SubtitleResult {
                name: s.name.unwrap_or_else(|| s.release_name.clone()),
                release: s.release_name,
                description: desc,
                download: Download::SubDl { url, season: parsed.season, episode: parsed.episode },
            })
        })
        .collect()
}

pub(super) async fn download(url: &str, season: Option<i32>, episode: Option<i32>) -> Option<PathBuf> {
    let bytes = crate::http::get_bytes(&format!("https://dl.subdl.com{url}")).await.ok()?;
    let folder = crate::paths::temp_folder().join("Subtitles").join(format!("{:x}", md5::compute(url.as_bytes())));
    std::fs::create_dir_all(&folder).ok()?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    zip.extract(&folder).ok()?;
    let mut files = Vec::new();
    collect(&folder, &mut files);
    files.retain(|f| is_subtitle(&f.to_string_lossy()));
    if let Some(ep) = episode {
        let hit = files.iter().find(|f| {
            let p = parse_name(&f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            p.episode == Some(ep) && (p.season.is_none() || season.is_none() || p.season == season)
        });
        if let Some(hit) = hit {
            return Some(hit.clone());
        }
    }
    files.into_iter().next()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { collect(&p, out) } else { out.push(p) }
    }
}
