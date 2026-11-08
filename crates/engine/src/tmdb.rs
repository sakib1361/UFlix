//! TMDB: trending and genre catalogues, search, and movie/show/season details. Results are cached in the store.

use crate::db::db;
use crate::http::client;
use crate::models::{ContentType, MediaItem, MediaType};
use crate::{parse, secrets};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const IMAGE_300: &str = "https://image.tmdb.org/t/p/w300";
pub const IMAGE_500: &str = "https://image.tmdb.org/t/p/w500";
pub const IMAGE_780: &str = "https://image.tmdb.org/t/p/w780";
pub const IMAGE_185: &str = "https://image.tmdb.org/t/p/w185";

/// Full image URL for a TMDB path ("/abc.jpg"), or "" when there is none.
pub fn image(size: &str, path: &Option<String>) -> String {
    match path {
        Some(p) if !p.is_empty() => format!("{size}{p}"),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Genre {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CastMember {
    pub name: String,
    pub character: String,
    pub profile_path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Credits {
    pub cast: Vec<CastMember>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalIds {
    pub imdb_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Movie {
    pub id: i64,
    pub imdb_id: Option<String>,
    pub title: String,
    pub overview: String,
    pub tagline: String,
    pub status: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub release_date: String,
    pub runtime: Option<i64>,
    pub vote_average: f64,
    pub vote_count: i64,
    pub genres: Vec<Genre>,
    pub credits: Option<Credits>,
    pub external_ids: Option<ExternalIds>,
}

impl Movie {
    pub fn year(&self) -> String {
        self.release_date.get(..4).unwrap_or_default().to_string()
    }
    pub fn imdb(&self) -> Option<String> {
        self.imdb_id.clone().or_else(|| self.external_ids.as_ref().and_then(|e| e.imdb_id.clone())).filter(|s| !s.is_empty())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SeasonSummary {
    pub season_number: i32,
    pub name: String,
    pub episode_count: i32,
    pub poster_path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Show {
    pub id: i64,
    pub name: String,
    pub overview: String,
    pub tagline: String,
    pub status: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub first_air_date: String,
    pub vote_average: f64,
    pub vote_count: i64,
    pub genres: Vec<Genre>,
    pub seasons: Vec<SeasonSummary>,
    pub credits: Option<Credits>,
    pub external_ids: Option<ExternalIds>,
}

impl Show {
    pub fn year(&self) -> String {
        self.first_air_date.get(..4).unwrap_or_default().to_string()
    }
    pub fn imdb(&self) -> Option<String> {
        self.external_ids.as_ref().and_then(|e| e.imdb_id.clone()).filter(|s| !s.is_empty())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Episode {
    pub season_number: i32,
    pub episode_number: i32,
    pub name: String,
    pub overview: String,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
    pub vote_average: f64,
    pub runtime: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Season {
    pub season_number: i32,
    pub episodes: Vec<Episode>,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct SearchItem {
    id: i64,
    title: Option<String>,
    name: Option<String>,
    overview: String,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: f64,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    results: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct GenreList {
    #[serde(default)]
    genres: Vec<Genre>,
}

#[derive(Deserialize)]
struct FindResult {
    #[serde(default)]
    tv_results: Vec<SearchItem>,
}

const API: &str = "https://api.themoviedb.org/3";

async fn get<T: DeserializeOwned>(path: &str, query: &[(&str, String)]) -> anyhow::Result<T> {
    let mut params: Vec<(&str, String)> = vec![("api_key", secrets::tmdb_api_key()), ("language", "en-US".into())];
    params.extend_from_slice(query);
    Ok(client().get(format!("{API}/{path}")).query(&params).send().await?.error_for_status()?.json().await?)
}

/// Cache-first: `fetch` runs only when nothing usable is cached under `key`.
async fn cached<T, F>(key: &str, days: i64, fetch: F) -> Option<T>
where
    T: DeserializeOwned + Serialize,
    F: std::future::Future<Output = anyhow::Result<T>>,
{
    if let Some(v) = db().cache_get::<T>(key) {
        return Some(v);
    }
    let v = fetch.await.ok()?;
    db().cache_put(key, &v, days);
    Some(v)
}

fn to_media(items: Vec<SearchItem>, content: ContentType) -> Vec<MediaItem> {
    items
        .into_iter()
        .map(|x| {
            let date = x.release_date.or(x.first_air_date).unwrap_or_default();
            MediaItem {
                tmdb_id: x.id,
                title: x.title.or(x.name).unwrap_or_default(),
                description: x.overview,
                image: image(IMAGE_300, &x.poster_path),
                backdrop: image(IMAGE_780, &x.backdrop_path),
                year: date.get(..4).unwrap_or_default().to_string(),
                rating: x.vote_average,
                media_type: MediaType::Local,
                content_type: content,
                ..Default::default()
            }
        })
        .collect()
}

fn kind(content: ContentType) -> &'static str {
    if content == ContentType::Shows { "tv" } else { "movie" }
}

pub async fn genres(content: ContentType) -> Vec<Genre> {
    let k = kind(content);
    cached(&format!("tmdb_genres_{k}"), 360, async { Ok(get::<GenreList>(&format!("genre/{k}/list"), &[]).await?.genres) })
        .await
        .unwrap_or_default()
}

async fn page(key: String, path: String, query: Vec<(&str, String)>, content: ContentType) -> Vec<MediaItem> {
    let items: Option<Vec<MediaItem>> = cached(&key, 3, async {
        let items = to_media(get::<Page>(&path, &query).await?.results, content);
        anyhow::ensure!(!items.is_empty(), "empty page");
        Ok(items)
    })
    .await;
    items.unwrap_or_default()
}

/// This week's trending titles; `page` starts at 0.
pub async fn trending(content: ContentType, page_no: u32) -> Vec<MediaItem> {
    let k = kind(content);
    page(format!("tmdb_trending_{k}_{page_no}"), format!("trending/{k}/week"), vec![("page", (page_no + 1).to_string())], content).await
}

pub async fn by_genre(content: ContentType, genre: &Genre, page_no: u32) -> Vec<MediaItem> {
    let k = kind(content);
    page(
        format!("tmdb_genre_{k}_{}_{page_no}", genre.id),
        format!("discover/{k}"),
        vec![("with_genres", genre.id.to_string()), ("page", (page_no + 1).to_string())],
        content,
    )
    .await
}

pub async fn search(content: ContentType, query: &str, year: Option<i32>) -> Vec<MediaItem> {
    let k = kind(content);
    let mut q = vec![("query", query.to_string()), ("include_adult", "true".to_string())];
    if let Some(y) = year.filter(|_| content == ContentType::Movie) {
        q.push(("year", y.to_string()));
    }
    page(format!("tmdb_search_{k}_{query}_{}", year.unwrap_or(0)), format!("search/{k}"), q, content).await
}

/// Details of a movie by TMDB id, else IMDb id, else by searching its (release) title.
pub async fn movie(item: &MediaItem) -> Option<Movie> {
    let id = if item.tmdb_id > 0 {
        item.tmdb_id.to_string()
    } else if item.id.starts_with("tt") {
        item.id.clone()
    } else {
        let parsed = parse::parse_name(&item.title);
        search(ContentType::Movie, &parsed.title, parsed.year).await.first()?.tmdb_id.to_string()
    };
    cached(&format!("tmdb_movie_{id}"), 30, async {
        get::<Movie>(&format!("movie/{id}"), &[("append_to_response", "external_ids,credits".into())]).await
    })
    .await
}

/// Details of a show by TMDB id, else by IMDb id.
pub async fn show(item: &MediaItem, force: bool) -> Option<Show> {
    let mut tmdb_id = item.tmdb_id;
    if tmdb_id == 0 && item.id.starts_with("tt") {
        let imdb = item.id.clone();
        let found: Option<Vec<SearchItem>> = cached(&format!("tmdb_find_{imdb}"), 30, async {
            Ok(get::<FindResult>(&format!("find/{imdb}"), &[("external_source", "imdb_id".into())]).await?.tv_results)
        })
        .await;
        tmdb_id = found?.first()?.id;
    }
    let key = format!("tmdb_show_{tmdb_id}");
    let fetch = async { get::<Show>(&format!("tv/{tmdb_id}"), &[("append_to_response", "external_ids,credits".into())]).await };
    let mut show = if force {
        let s = fetch.await.ok()?;
        db().cache_put(&key, &s, 7);
        s
    } else {
        cached(&key, 7, fetch).await?
    };
    show.seasons.retain(|s| s.name != "Specials" && s.season_number > 0);
    Some(show)
}

/// A season's episodes; refetched when a cached season still has upcoming episodes without details.
pub async fn season(show_id: i64, number: i32) -> Option<Season> {
    let key = format!("tmdb_season_{show_id}_{number}");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if let Some(s) = db().cache_get::<Season>(&key) {
        let stale = s.episodes.iter().any(|e| e.air_date.as_deref().is_some_and(|d| d >= today.as_str()) && e.overview.is_empty());
        if !stale {
            return Some(s);
        }
    }
    let s = get::<Season>(&format!("tv/{show_id}/season/{number}"), &[]).await.ok()?;
    db().cache_put(&key, &s, 7);
    Some(s)
}
