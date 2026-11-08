//! A paged feed over one or more result streams: interleaved, de-duplicated, and pulled only when asked (incremental
//! loading as the page scrolls).

use super::{Filter, YtItem, from_item, from_video, rp};
use rustypipe::model::paginator::Paginator;
use rustypipe::model::{VideoItem, YouTubeItem};
use rustypipe::param::search_filter::{ItemType, SearchFilter};
use std::collections::{HashSet, VecDeque};

enum Origin {
    Search(String, Filter),
    Uploads(String),
}

enum Next {
    Search(Paginator<YouTubeItem>),
    Videos(Paginator<VideoItem>),
}

struct Source {
    origin: Origin,
    started: bool,
    buffer: VecDeque<YtItem>,
    next: Option<Next>,
}

impl Source {
    fn new(origin: Origin) -> Self {
        Self { origin, started: false, buffer: VecDeque::new(), next: None }
    }

    /// Fetches the next page into the buffer; false when the source is exhausted.
    async fn fill(&mut self) -> bool {
        let q = rp().query();
        if !self.started {
            self.started = true;
            match &self.origin {
                Origin::Search(query, filter) => {
                    let item_type = match filter {
                        Filter::All => None,
                        Filter::Videos => Some(ItemType::Video),
                        Filter::Playlists => Some(ItemType::Playlist),
                        Filter::Channels => Some(ItemType::Channel),
                    };
                    let result = match item_type {
                        None => q.search::<YouTubeItem, _>(query).await,
                        Some(t) => q.search_filter::<YouTubeItem, _>(query, &SearchFilter::new().item_type(t)).await,
                    };
                    let Ok(r) = result else { return false };
                    self.buffer.extend(r.items.items.iter().cloned().map(from_item));
                    self.next = Some(Next::Search(r.items));
                }
                Origin::Uploads(id) => {
                    let Ok(c) = q.channel_videos(id).await else { return false };
                    self.buffer.extend(c.content.items.iter().cloned().map(from_video));
                    self.next = Some(Next::Videos(c.content));
                }
            }
            return !self.buffer.is_empty();
        }
        match self.next.take() {
            Some(Next::Search(p)) => match p.next(&q).await {
                Ok(Some(p)) => {
                    self.buffer.extend(p.items.iter().cloned().map(from_item));
                    self.next = Some(Next::Search(p));
                }
                _ => return false,
            },
            Some(Next::Videos(p)) => match p.next(&q).await {
                Ok(Some(p)) => {
                    self.buffer.extend(p.items.iter().cloned().map(from_video));
                    self.next = Some(Next::Videos(p));
                }
                _ => return false,
            },
            None => return false,
        }
        !self.buffer.is_empty()
    }
}

pub struct Feed {
    sources: Vec<Source>,
    seen: HashSet<String>,
}

impl Feed {
    fn from(origins: Vec<Origin>) -> Self {
        Self { sources: origins.into_iter().map(Source::new).collect(), seen: HashSet::new() }
    }

    pub fn search(query: &str, filter: Filter) -> Self {
        Self::from(vec![Origin::Search(query.to_string(), filter)])
    }

    /// "For you": the last three searches interleaved (or the last search, or a few general topics).
    pub fn for_you() -> Self {
        let mut seeds: Vec<String> = crate::settings::youtube_search_history().into_iter().take(3).collect();
        if seeds.is_empty() {
            let last = crate::settings::last_search();
            if !last.is_empty() {
                seeds.push(last);
            }
        }
        if seeds.is_empty() {
            seeds = vec!["trending".into(), "music".into(), "documentary".into()];
        }
        Self::from(seeds.into_iter().map(|q| Origin::Search(q, Filter::Videos)).collect())
    }

    pub fn uploads(channel_id: &str) -> Self {
        Self::from(vec![Origin::Uploads(channel_id.to_string())])
    }

    /// The next `count` new items (fewer when the sources run out; empty at the end).
    pub async fn next(&mut self, count: usize) -> Vec<YtItem> {
        let mut page = Vec::new();
        while page.len() < count && !self.sources.is_empty() {
            let mut i = 0;
            while i < self.sources.len() && page.len() < count {
                let src = &mut self.sources[i];
                if src.buffer.is_empty() && !src.fill().await {
                    self.sources.remove(i);
                    continue;
                }
                if let Some(item) = src.buffer.pop_front() {
                    let key = format!("{:?}{}", item.kind, item.id);
                    if self.seen.insert(key) {
                        page.push(item);
                    }
                }
                i += 1;
            }
        }
        page
    }
}
