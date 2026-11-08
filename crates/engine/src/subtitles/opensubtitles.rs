//! OpenSubtitles REST API (key and account from secrets.env). Local files are matched by their movie hash too.

use super::{Download, SubtitleResult, imdb_id};
use crate::http::client;
use crate::models::MediaItem;
use crate::secrets;
use serde::Deserialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const API: &str = "https://api.opensubtitles.com/api/v1";

static TOKEN: Mutex<Option<String>> = Mutex::new(None);

fn request(method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
    let mut r = client()
        .request(method, format!("{API}/{path}"))
        .header("User-Agent", "UFlix v2")
        .header("Api-Key", secrets::opensubtitles_api_key())
        .header("Accept", "application/json");
    if let Some(t) = TOKEN.lock().unwrap().as_deref() {
        r = r.bearer_auth(t);
    }
    r
}

#[derive(Deserialize)]
struct Login {
    token: Option<String>,
}

async fn login() -> bool {
    if TOKEN.lock().unwrap().is_some() {
        return true;
    }
    let body = serde_json::json!({ "username": secrets::opensubtitles_username(), "password": secrets::opensubtitles_password() });
    let Ok(resp) = request(reqwest::Method::POST, "login").json(&body).send().await else { return false };
    let Ok(login) = resp.json::<Login>().await else { return false };
    let ok = login.token.is_some();
    *TOKEN.lock().unwrap() = login.token;
    ok
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchResponse {
    data: Vec<Item>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    attributes: Attributes,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Attributes {
    release: String,
    download_count: i64,
    ratings: f64,
    files: Vec<SubFile>,
    feature_details: FeatureDetails,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SubFile {
    file_id: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FeatureDetails {
    title: String,
}

/// OpenSubtitles' hash: size plus the 64-bit little-endian sums of the first and last 64 KiB.
fn movie_hash(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut hash = size;
    let mut sum = |f: &mut std::fs::File| -> Option<()> {
        let mut buf = vec![0u8; 65536];
        let n = f.read(&mut buf).ok()?;
        for chunk in buf[..n].as_chunks::<8>().0 {
            hash = hash.wrapping_add(u64::from_le_bytes(*chunk));
        }
        Some(())
    };
    sum(&mut f)?;
    f.seek(SeekFrom::Start(size.saturating_sub(65536))).ok()?;
    sum(&mut f)?;
    Some(format!("{hash:016x}"))
}

pub(super) async fn search(media: &MediaItem) -> Vec<SubtitleResult> {
    if !login().await {
        return Vec::new();
    }
    let mut query: Vec<(&str, String)> = Vec::new();
    let path = Path::new(&media.url);
    if path.is_file() {
        if let Some(h) = movie_hash(path) {
            query.push(("moviehash", h));
        }
        query.push(("query", path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    } else {
        query.push(("query", media.title.replace(':', "")));
    }
    if let Some(id) = imdb_id(media) {
        query.push(("imdb_id", id[2..].to_string()));
    }
    query.push(("languages", "en".into()));
    let Ok(resp) = request(reqwest::Method::GET, "subtitles").query(&query).send().await else { return Vec::new() };
    let data: SearchResponse = resp.json().await.unwrap_or_default();
    data.data
        .into_iter()
        .filter_map(|i| {
            let a = i.attributes;
            let file_id = a.files.first()?.file_id;
            Some(SubtitleResult {
                name: a.feature_details.title,
                release: a.release,
                description: format!("{} downloads · rating {:.1}", a.download_count, a.ratings),
                download: Download::OpenSubtitles { file_id },
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct DownloadLink {
    link: Option<String>,
}

pub(super) async fn download(file_id: i64) -> Option<PathBuf> {
    login().await;
    let resp = request(reqwest::Method::POST, "download").json(&serde_json::json!({ "file_id": file_id })).send().await.ok()?;
    let link = resp.json::<DownloadLink>().await.ok()?.link?;
    let text = crate::http::get_text(&link).await.ok()?;
    let folder = crate::paths::temp_folder().join("Subtitles");
    std::fs::create_dir_all(&folder).ok()?;
    let file = folder.join(format!("opensubtitles-{file_id}.srt"));
    std::fs::write(&file, text).ok()?;
    Some(file)
}
