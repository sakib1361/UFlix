//! UI-free core of UFlix. Nothing here knows about the UI toolkit; async functions run on tokio.

pub mod cleaner;
pub mod db;
pub mod http;
pub mod image_cache;
pub mod models;
pub mod parse;
pub mod paths;
pub mod providers;
pub mod search;
pub mod secrets;
pub mod settings;
pub mod subtitles;
pub mod thumbs;
pub mod tmdb;
pub mod tools;
pub mod tv;
pub mod watchlist;
pub mod youtube;

pub use models::*;

/// The app name; it names the data and download folders.
pub const APP_NAME: &str = "UFlix";
