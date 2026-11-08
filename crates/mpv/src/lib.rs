//! UI-free libmpv binding for UFlix: [`Player`] (client API + events), [`RenderContext`] (OpenGL rendering into the
//! embedder's frame) and [`StreamSource`] (reader-backed "uflix://" streams for torrent playback).
//! libmpv is loaded at runtime from the bundled copy (see [`ffi::api`]).

pub mod ffi;
mod player;
mod render;
mod stream;

pub use player::{EndReason, Event, LoadOptions, Options, Player, Track, TrackType, client_api_version};
pub use render::{NativeDisplay, RenderContext};
pub use stream::StreamSource;

/// Where libmpv was loaded from, or why it couldn't be.
pub fn library_path() -> Result<std::path::PathBuf, String> {
    ffi::api().map(|a| a.path.clone())
}
