//! YouTube page: a home view (continue watching, recent searches, topics, "For you"), searches with filters and channel
//! uploads, all paged in as the page scrolls; an in-page back stack; links are resolved and opened directly.

use crate::cards::{CardList, CardSpec};
use crate::runtime::{bg, sleep, spawn_ui};
use crate::ui::{self, MenuEntry, Page, Shell, YoutubeState};
use engine::youtube::{self, Feed, Filter, Kind, YtItem};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Mutex;

const PAGE: usize = 24;

#[derive(Clone, PartialEq)]
enum View {
    Home,
    Search(String, Filter),
    Channel(String, String),
}

struct State {
    view: View,
    back: Vec<View>,
    feed: Option<Arc<Mutex<Feed>>>,
    items: Vec<YtItem>,
    cards: CardList,
    continue_items: Vec<YtItem>,
    continue_cards: CardList,
    loading: bool,
    done: bool,
    seq: u64,
    edit_seq: u64,
    suggestions: Vec<String>,
    /// The item a context menu is open for.
    menu_item: Option<YtItem>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

fn ys<R>(f: impl FnOnce(&YoutubeState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<YoutubeState>())
}

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(items.into_iter().map(SharedString::from).collect::<Vec<_>>())))
}

fn spec(item: &YtItem) -> CardSpec {
    let subtitle = [item.author.clone(), item.meta.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    CardSpec {
        title: item.title.clone(),
        subtitle,
        image_url: item.thumbnail.clone(),
        badge: item.badge(),
        progress: item.progress as f32,
        ..Default::default()
    }
}

pub fn init() {
    let cards = CardList::new(300);
    let continue_cards = CardList::new(296);
    ys(|y| {
        y.set_feed(cards.model());
        y.set_continue_cards(continue_cards.model());
        y.set_topics(strings(youtube::TOPICS.iter().map(|t| t.to_string()).collect()));
        y.on_edited(|t| suggest(t.to_string()));
        y.on_submit(|t| submit(t.to_string()));
        y.on_pick_suggestion(|i| {
            if let Some(Some(q)) = with(|s| s.suggestions.get(i as usize).cloned()) {
                ys(|y| y.set_query(q.clone().into()));
                submit(q);
            }
        });
        y.on_back(back);
        y.on_open(|i| {
            if let Some(Some(item)) = with(|s| s.items.get(i as usize).cloned()) {
                open_item(item);
            }
        });
        y.on_open_continue(|i| {
            if let Some(Some(item)) = with(|s| s.continue_items.get(i as usize).cloned()) {
                open_item(item);
            }
        });
        y.on_remove_continue(|i| {
            if let Some(Some(item)) = with(|s| s.continue_items.get(i as usize).cloned()) {
                engine::watchlist::remove(&item.to_media());
                refresh_continue_watching();
            }
        });
        y.on_near_end(load_page);
        y.on_select_filter(|i| {
            let q = ys(|y| y.get_query()).to_string();
            let filter = [Filter::All, Filter::Videos, Filter::Playlists, Filter::Channels][i as usize % 4];
            show(View::Search(q, filter), false);
        });
        y.on_history_select(|i| {
            if let Some(q) = engine::settings::youtube_search_history().get(i as usize).cloned() {
                ys(|y| y.set_query(q.clone().into()));
                submit(q);
            }
        });
        y.on_history_remove(|i| {
            if let Some(q) = engine::settings::youtube_search_history().get(i as usize).cloned() {
                engine::settings::remove_youtube_search(&q);
                refresh_history();
            }
        });
        y.on_topic(|i| {
            let q = youtube::TOPICS[i as usize % youtube::TOPICS.len()].to_string();
            ys(|y| y.set_query(q.clone().into()));
            submit(q);
        });
        y.on_context(|from_continue, i, x, y| context_menu(from_continue, i as usize, x, y));
        y.on_menu_pick(menu_pick);
    });
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            view: View::Home,
            back: Vec::new(),
            feed: None,
            items: Vec::new(),
            cards,
            continue_items: Vec::new(),
            continue_cards,
            loading: false,
            done: false,
            seq: 0,
            edit_seq: 0,
            suggestions: Vec::new(),
            menu_item: None,
        })
    });
    refresh_history();
}

thread_local! {
    static STARTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The first time the page shows, its feeds load.
pub fn shown() {
    if !STARTED.with(|s| s.replace(true)) {
        refresh_continue_watching();
        show(View::Home, false);
    }
}

/// Called when the player closes, so "Continue watching" shows the latest positions.
pub fn refresh_continue_watching() {
    if !STARTED.with(|s| s.get()) {
        return;
    }
    let items = youtube::continue_watching();
    with(|s| {
        s.continue_cards.clear();
        s.continue_cards.push_all(items.iter().map(spec).collect());
        s.continue_items = items;
    });
}

fn refresh_history() {
    ys(|y| y.set_history(strings(engine::settings::youtube_search_history())));
}

/// Shows a view and starts its feed. `push` keeps the current view for Back.
fn show(view: View, push: bool) {
    let feed = match &view {
        View::Home => Feed::for_you(),
        View::Search(q, f) => Feed::search(q, *f),
        View::Channel(id, _) => Feed::uploads(id),
    };
    let can_back = with(|s| {
        if push && s.view != view {
            s.back.push(s.view.clone());
        }
        s.view = view.clone();
        s.feed = Some(Arc::new(Mutex::new(feed)));
        s.items.clear();
        s.cards.clear();
        s.loading = false;
        s.done = false;
        s.seq += 1;
        !s.back.is_empty()
    })
    .unwrap_or(false);
    ys(|y| {
        y.set_can_go_back(can_back);
        match &view {
            View::Home => {
                y.set_view(0);
                y.set_feed_title("For you".into());
            }
            View::Search(q, f) => {
                y.set_view(1);
                y.set_filter(match f {
                    Filter::All => 0,
                    Filter::Videos => 1,
                    Filter::Playlists => 2,
                    Filter::Channels => 3,
                });
                y.set_feed_title(format!("Results for “{q}”").into());
            }
            View::Channel(_, name) => {
                y.set_view(2);
                y.set_channel_title(name.clone().into());
                y.set_channel_subtitle("".into());
                y.set_channel_avatar(Default::default());
                y.set_feed_title("Uploads".into());
            }
        }
    });
    if let View::Channel(id, _) = view {
        load_channel_header(id);
    }
    load_page();
}

fn load_channel_header(id: String) {
    spawn_ui(async move {
        let key = id.clone();
        let Ok(c) = bg(async move { youtube::channel(&key).await }).await else { return };
        let still = with(|s| matches!(&s.view, View::Channel(cur, _) if *cur == id)).unwrap_or(false);
        if !still {
            return;
        }
        ys(|y| {
            y.set_channel_title(c.title.clone().into());
            y.set_channel_subtitle(c.subscribers.clone().into());
        });
        let scale = ui::scale_factor();
        if let Some(img) = crate::images::load(c.avatar, (88.0 * scale) as u32).await {
            ys(|y| y.set_channel_avatar(img));
        }
    });
}

fn back() {
    let prev = with(|s| s.back.pop()).flatten();
    if let Some(v) = prev {
        if let View::Search(q, _) = &v {
            ys(|y| y.set_query(q.clone().into()));
        }
        if v == View::Home {
            ys(|y| y.set_query("".into()));
        }
        show(v, false);
    }
}

fn load_page() {
    let Some(Some((feed, seq))) = with(|s| {
        if s.loading || s.done {
            return None;
        }
        s.loading = true;
        Some((s.feed.clone()?, s.seq))
    }) else {
        return;
    };
    ys(|y| y.set_loading(true));
    spawn_ui(async move {
        let items = bg(async move { feed.lock().await.next(PAGE).await }).await;
        let current = with(|s| {
            if s.seq != seq {
                return false;
            }
            s.loading = false;
            s.done = items.is_empty();
            s.cards.push_all(items.iter().map(spec).collect());
            s.items.extend(items);
            true
        })
        .unwrap_or(false);
        if current {
            ys(|y| y.set_loading(false));
        }
    });
}

/// Suggestions after a short pause in typing.
fn suggest(text: String) {
    let seq = with(|s| {
        s.edit_seq += 1;
        s.edit_seq
    })
    .unwrap_or(0);
    if youtube::is_link(&text) || text.trim().len() < 2 {
        with(|s| s.suggestions.clear());
        ys(|y| y.set_suggestions(strings(Vec::new())));
        return;
    }
    spawn_ui(async move {
        sleep(250).await;
        if with(|s| s.edit_seq) != Some(seq) {
            return;
        }
        let q = text.clone();
        let list = bg(async move { youtube::suggestions(&q).await }).await;
        if with(|s| s.edit_seq) != Some(seq) {
            return;
        }
        with(|s| s.suggestions = list.clone());
        ys(|y| y.set_suggestions(strings(list)));
    });
}

/// A search, or a pasted link opened directly.
fn submit(text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    ys(|y| y.set_suggestions_open(false));
    if youtube::is_link(&text) {
        ys(|y| y.set_resolving(true));
        spawn_ui(async move {
            let t = text.clone();
            let item = bg(async move { youtube::resolve(&t).await }).await;
            ys(|y| y.set_resolving(false));
            match item {
                Some(item) => open_item(item),
                None => ui::toast("That link isn't a YouTube video, playlist or channel"),
            }
        });
        return;
    }
    engine::settings::add_youtube_search(&text);
    engine::settings::set_last_search(text.clone());
    refresh_history();
    show(View::Search(text, Filter::All), true);
}

fn open_item(item: YtItem) {
    match item.kind {
        Kind::Channel => show_channel(item.channel_id.clone(), item.title.clone()),
        _ => crate::open::play(item.to_media()),
    }
}

/// Shows a channel's uploads (also from the player's channel link).
pub fn show_channel(id: String, name: String) {
    if id.is_empty() {
        return;
    }
    ui::window().global::<Shell>().set_page(Page::Youtube);
    STARTED.with(|s| s.set(true));
    show(View::Channel(id, name), true);
}

fn link_of(item: &YtItem) -> String {
    match item.kind {
        Kind::Video => format!("https://www.youtube.com/watch?v={}", item.id),
        Kind::Playlist => format!("https://www.youtube.com/playlist?list={}", item.id),
        Kind::Channel => format!("https://www.youtube.com/channel/{}", item.id),
    }
}

fn context_menu(from_continue: bool, index: usize, x: f32, y: f32) {
    let item = with(|s| if from_continue { s.continue_items.get(index).cloned() } else { s.items.get(index).cloned() }).flatten();
    let Some(item) = item else { return };
    let w = ui::window();
    let icons = w.global::<ui::Icons>();
    let mut entries = vec![MenuEntry { text: "Play".into(), icon: icons.get_play() }];
    if !item.channel_id.is_empty() && item.kind != Kind::Channel {
        entries.push(MenuEntry { text: "Go to channel".into(), icon: icons.get_person() });
    }
    entries.push(MenuEntry { text: "Copy link".into(), icon: icons.get_copy() });
    entries.push(MenuEntry { text: "Open in browser".into(), icon: icons.get_open_in_new() });
    with(|s| s.menu_item = Some(item));
    ys(|ys| {
        ys.set_menu_entries(ModelRc::from(Rc::new(VecModel::from(entries))));
        ys.set_menu_x(x);
        ys.set_menu_y(y);
        ys.set_menu_open(true);
    });
}

fn menu_pick(index: i32) {
    let Some(Some(item)) = with(|s| s.menu_item.take()) else { return };
    let has_channel = !item.channel_id.is_empty() && item.kind != Kind::Channel;
    let actions: Vec<&str> = if has_channel { vec!["play", "channel", "copy", "browser"] } else { vec!["play", "copy", "browser"] };
    match actions.get(index as usize).copied() {
        Some("play") => open_item(item),
        Some("channel") => show_channel(item.channel_id.clone(), item.author.clone()),
        Some("copy") => {
            let link = link_of(&item);
            match arboard::Clipboard::new().and_then(|mut c| c.set_text(link)) {
                Ok(()) => ui::toast("Link copied"),
                Err(e) => ui::toast(format!("Couldn't copy: {e}")),
            }
        }
        Some("browser") => {
            let _ = open::that_detached(link_of(&item));
        }
        _ => {}
    }
}
