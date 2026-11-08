//! Torrent search for library titles: PirateBay (HTML), two Stremio addons (Torrentio, TorrentsDB) and local folders.
//! A user-set URL per engine is kept in Settings.

mod local;
mod piratebay;
mod stremio;

use crate::models::{TorrentInfo, TorrentSource};
use crate::tmdb::{Movie, Show};

const TORRENTIO: &str = "https://torrentio.strem.fun";
const TORRENTSDB: &str = "https://torrentsdb.com/eyJwcm92aWRlcnMiOlsieXRzIiwiZXp0diIsIjEzMzd4IiwidG9ycmVudGNzdiIsInNrdG9ycmVudCIsIjF0YW1pbGJsYXN0ZXJzIiwiMXRhbWlsbXYiLCJ0aGVwaXJhdGViYXkiLCJraWNrYXNzdG9ycmVudHMiLCJleHRyZW1seW10b3JyZW50cyJdfQ==";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    PirateBay,
    Torrentio,
    TorrentsDb,
    /// Folders on disk (comma-separated in the engine's URL).
    Local,
}

impl Engine {
    pub const ALL: [Engine; 4] = [Engine::PirateBay, Engine::Torrentio, Engine::TorrentsDb, Engine::Local];

    /// The key in Settings (same names as the C# app).
    pub fn key(self) -> &'static str {
        match self {
            Engine::PirateBay => "PiratebaySearch",
            Engine::Torrentio => "Stremio-Torrentio",
            Engine::TorrentsDb => "Stremio-TorrentsDB",
            Engine::Local => "LocalSearch",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Engine::PirateBay => "PirateBay",
            Engine::Torrentio => "Torrentio",
            Engine::TorrentsDb => "TorrentsDB",
            Engine::Local => "Local folders",
        }
    }

    fn default_url(self) -> &'static str {
        match self {
            Engine::PirateBay => "https://tpb.party",
            Engine::Torrentio => TORRENTIO,
            Engine::TorrentsDb => TORRENTSDB,
            Engine::Local => "",
        }
    }

    /// The engine's URL: the user's override from Settings, else the default.
    pub fn url(self) -> String {
        crate::settings::torrent_sources()
            .into_iter()
            .find(|s| s.name == self.key())
            .map(|s| s.url)
            .filter(|u| !u.trim().is_empty() || self == Engine::Local)
            .unwrap_or_else(|| self.default_url().to_string())
    }

    pub fn set_url(self, url: &str) {
        let mut sources = crate::settings::torrent_sources();
        match sources.iter_mut().find(|s| s.name == self.key()) {
            Some(s) => s.url = url.to_string(),
            None => sources.push(TorrentSource { name: self.key().into(), url: url.into() }),
        }
        crate::settings::set_torrent_sources(sources);
    }

    pub async fn search_movie(self, movie: &Movie, forced: bool) -> Vec<TorrentInfo> {
        match self {
            Engine::PirateBay => piratebay::search(&self.url(), &movie.title, forced).await,
            Engine::Torrentio | Engine::TorrentsDb => match movie.imdb() {
                Some(id) => stremio::search(self.label(), &self.url(), &format!("movie/{id}"), forced).await,
                None => Vec::new(),
            },
            Engine::Local => local::movie(&self.url(), &movie.title),
        }
    }

    pub async fn search_episode(self, show: &Show, season: i32, episode: i32, forced: bool) -> Vec<TorrentInfo> {
        match self {
            Engine::PirateBay => {
                let url = self.url();
                let found = piratebay::search(&url, &format!("{} S{season:02}E{episode:02}", show.name), forced).await;
                if found.is_empty() { piratebay::search(&url, &show.name, forced).await } else { found }
            }
            Engine::Torrentio | Engine::TorrentsDb => match show.imdb() {
                Some(id) => stremio::search(self.label(), &self.url(), &format!("series/{id}:{season}:{episode}"), forced).await,
                None => Vec::new(),
            },
            Engine::Local => local::episode(&self.url(), &show.name, season, episode),
        }
    }

    /// Torrents of a whole season (PirateBay searches "Name Sxx"; the others fall back to the season's first episode).
    pub async fn search_season(self, show: &Show, season: i32, forced: bool) -> Vec<TorrentInfo> {
        match self {
            Engine::PirateBay => {
                let url = self.url();
                let found = piratebay::search(&url, &format!("{} S{season:02}", show.name), forced).await;
                if found.is_empty() { piratebay::search(&url, &show.name, forced).await } else { found }
            }
            _ => self.search_episode(show, season, 1, forced).await,
        }
    }
}
