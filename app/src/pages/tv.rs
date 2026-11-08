//! Live TV: every channel of the configured M3U playlists, filterable by name; a channel opens in the player.

use crate::cards::{CardList, CardSpec};
use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, TvState};
use engine::models::MediaItem;
use slint::ComponentHandle;
use std::cell::RefCell;

#[derive(Default)]
struct State {
    all: Vec<MediaItem>,
    shown: Vec<MediaItem>,
    cards: Option<CardList>,
    loaded: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn tv<R>(f: impl FnOnce(&TvState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<TvState>())
}

pub fn init() {
    let cards = CardList::new(168);
    tv(|t| {
        t.set_channels(cards.model());
        t.on_filter_edited(|f| apply_filter(&f));
        t.on_open(|i| {
            if let Some(item) = STATE.with(|s| s.borrow().shown.get(i as usize).cloned()) {
                crate::open::play(item);
            }
        });
        t.on_refresh(|| load(true));
    });
    STATE.with(|s| s.borrow_mut().cards = Some(cards));
}

/// Loads the channels the first time the page is shown (or again on refresh).
pub fn shown() {
    if !STATE.with(|s| s.borrow().loaded) {
        load(false);
    }
}

fn load(force: bool) {
    if force {
        for link in engine::settings::tv_links().split(',').map(str::trim).filter(|l| !l.is_empty()) {
            engine::db::db().cache_put::<Option<String>>(link, &None, -1);
        }
    }
    STATE.with(|s| s.borrow_mut().loaded = true);
    tv(|t| t.set_loading(true));
    spawn_ui(async {
        let channels = bg(engine::tv::channels()).await;
        STATE.with(|s| s.borrow_mut().all = channels);
        tv(|t| t.set_loading(false));
        let filter = tv(|t| t.get_filter()).to_string();
        apply_filter(&filter);
    });
}

fn apply_filter(filter: &str) {
    let f = filter.trim().to_lowercase();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let shown: Vec<MediaItem> = s.all.iter().filter(|c| f.is_empty() || c.title.to_lowercase().contains(&f)).cloned().collect();
        let cards = s.cards.clone().unwrap();
        cards.clear();
        cards.push_all(
            shown.iter().map(|c| CardSpec { title: c.title.clone(), image_url: c.image.clone(), ..Default::default() }).collect(),
        );
        let total = s.all.len();
        s.shown = shown;
        tv(|t| t.set_total(total as i32));
    });
}
