//! Remote images for cards: downloaded through the engine's disk cache, decoded and scaled to their on-screen width off
//! the UI thread, and kept in a small in-memory cache so revisited pages don't decode again.

use crate::runtime::{bg, blocking};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;
use tokio::sync::Semaphore;

const CACHE_SIZE: usize = 200;

/// Decoded images by (url, width), with their insertion order for eviction.
type Cache = (HashMap<(String, u32), Image>, VecDeque<(String, u32)>);

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Default::default());
}

fn decode_slots() -> &'static Semaphore {
    static S: OnceLock<Semaphore> = OnceLock::new();
    S.get_or_init(|| Semaphore::new(4))
}

/// The image at `url` scaled to `width` physical pixels (aspect kept). Call on the UI thread.
pub async fn load(url: String, width: u32) -> Option<Image> {
    if url.is_empty() {
        return None;
    }
    let key = (url.clone(), width);
    if let Some(img) = CACHE.with(|c| c.borrow().0.get(&key).cloned()) {
        return Some(img);
    }
    let buffer = bg(async move {
        // local videos (watchlist entries) show a frame of the video
        let path = if engine::thumbs::is_video_path(&url) {
            engine::thumbs::get(std::path::Path::new(&url)).await?
        } else {
            engine::image_cache::get(&url).await?
        };
        let _slot = decode_slots().acquire().await.ok()?;
        blocking(move || decode(&path, width)).await
    })
    .await?;
    let img = Image::from_rgba8(buffer);
    CACHE.with(|c| {
        let (map, order) = &mut *c.borrow_mut();
        map.insert(key.clone(), img.clone());
        order.push_back(key);
        while order.len() > CACHE_SIZE {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            }
        }
    });
    Some(img)
}

fn decode(path: &std::path::Path, width: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let img = if width > 0 && img.width() > width { img.resize(width, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
    let rgba = img.into_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height()))
}
