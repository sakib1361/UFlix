//! The catalogue rows on Home and Discover: TMDB trending and genres, Cinemeta's top-seeded titles, the watchlist.

use crate::http::get_json_cached;
use crate::models::{ContentType, MediaItem, MediaType};
use crate::tmdb::{self, Genre};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub enum Provider {
    /// TMDB trending this week.
    Popular(ContentType),
    Genre(ContentType, Genre),
    /// Cinemeta's catalogue, ordered by torrent seeds.
    TopSeeded(ContentType),
    /// "Continue watching".
    Watchlist,
}

impl Provider {
    /// The Home rows, in order.
    pub fn home() -> Vec<Provider> {
        vec![
            Provider::Watchlist,
            Provider::Popular(ContentType::Movie),
            Provider::Popular(ContentType::Shows),
            Provider::TopSeeded(ContentType::Movie),
            Provider::TopSeeded(ContentType::Shows),
        ]
    }

    pub fn name(&self) -> String {
        match self {
            Provider::Popular(ContentType::Shows) => "Popular Shows".into(),
            Provider::Popular(_) => "Popular Movies".into(),
            Provider::TopSeeded(ContentType::Shows) => "Top Seeded Shows".into(),
            Provider::TopSeeded(_) => "Top Seeded Movies".into(),
            Provider::Genre(_, g) => g.name.clone(),
            Provider::Watchlist => "Continue Watching".into(),
        }
    }

    pub fn content_type(&self) -> ContentType {
        match self {
            Provider::Popular(c) | Provider::Genre(c, _) | Provider::TopSeeded(c) => *c,
            Provider::Watchlist => ContentType::Movie,
        }
    }

    /// Items in this row can be removed (the watchlist).
    pub fn can_delete(&self) -> bool {
        matches!(self, Provider::Watchlist)
    }

    /// One page of items; `page` starts at 0. An empty page ends the row.
    pub async fn load(&self, page: u32) -> Vec<MediaItem> {
        match self {
            Provider::Popular(c) => tmdb::trending(*c, page).await,
            Provider::Genre(c, g) => tmdb::by_genre(*c, g, page).await,
            Provider::TopSeeded(c) => top_seeded(*c, page).await,
            Provider::Watchlist if page == 0 => crate::watchlist::items(),
            Provider::Watchlist => Vec::new(),
        }
    }
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct Catalog {
    metas: Vec<CatalogMeta>,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct CatalogMeta {
    id: String,
    name: String,
    poster: Option<String>,
    background: Option<String>,
    release_info: Option<String>,
}

async fn top_seeded(content: ContentType, page: u32) -> Vec<MediaItem> {
    const PAGE: u32 = 100;
    let kind = if content == ContentType::Shows { "series" } else { "movie" };
    let url = format!("https://v3-cinemeta.strem.io/catalog/{kind}/top/skip={}.json", page * PAGE);
    let catalog: Catalog = get_json_cached(&url, false, 3).await.unwrap_or_default();
    catalog
        .metas
        .into_iter()
        .filter(|m| !m.id.is_empty())
        .map(|m| MediaItem {
            id: m.id,
            title: m.name,
            image: m.poster.unwrap_or_default(),
            backdrop: m.background.unwrap_or_default(),
            year: m.release_info.unwrap_or_default().split('-').next().unwrap_or_default().to_string(),
            media_type: MediaType::Local,
            content_type: content,
            ..Default::default()
        })
        .collect()
}
