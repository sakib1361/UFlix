//! Discover: pick Movies/Shows and a category (Popular, Top Seeded or a TMDB genre); the grid pages in as it scrolls.

use crate::cards::CardList;
use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, DiscoverState, Page, Shell};
use engine::models::{ContentType, MediaItem};
use engine::providers::Provider;
use engine::tmdb::Genre;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

struct State {
    content: ContentType,
    providers: Vec<Provider>,
    current: usize,
    items: Vec<MediaItem>,
    cards: CardList,
    page: u32,
    loading: bool,
    done: bool,
    /// Bumped when the catalogue changes, so a slow page of the old one is dropped.
    seq: u64,
    genres: [Vec<Genre>; 2],
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("discover::init")))
}

fn providers(content: ContentType, genres: &[Genre]) -> Vec<Provider> {
    let mut list = vec![Provider::Popular(content), Provider::TopSeeded(content)];
    list.extend(genres.iter().map(|g| Provider::Genre(content, g.clone())));
    list
}

fn label(p: &Provider) -> String {
    match p {
        Provider::Popular(_) => "Popular".into(),
        Provider::TopSeeded(_) => "Top Seeded".into(),
        other => other.name(),
    }
}

pub fn init() {
    let cards = CardList::new(220);
    let w = ui::window();
    let d = w.global::<DiscoverState>();
    d.set_cards(cards.model());
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            content: ContentType::Movie,
            providers: providers(ContentType::Movie, &[]),
            current: 0,
            items: Vec::new(),
            cards,
            page: 0,
            loading: false,
            done: false,
            seq: 0,
            genres: Default::default(),
        })
    });
    d.on_select_content(|i| set_content(if i == 1 { ContentType::Shows } else { ContentType::Movie }, None));
    d.on_select_category(|i| select(i as usize, false));
    d.on_near_end(load_page);
    d.on_open(|i| {
        if let Some(item) = with(|s| s.items.get(i as usize).cloned()) {
            crate::open::open_media(item);
        }
    });
    // genres for both content types, once
    spawn_ui(async {
        let (m, s) = bg(async { tokio::join!(engine::tmdb::genres(ContentType::Movie), engine::tmdb::genres(ContentType::Shows)) }).await;
        let content = with(|st| {
            st.genres = [m, s];
            st.content
        });
        let current = with(|st| st.providers.get(st.current).cloned());
        set_content(content, current);
    });
}

/// The first time the page shows, its first catalogue loads.
pub fn shown() {
    if with(|s| s.items.is_empty() && !s.loading) {
        let current = with(|s| s.current);
        select(current, true);
    }
}

/// Shows Discover on `provider` (Home's "Explore").
pub fn show_provider(provider: Provider) {
    ui::window().global::<Shell>().set_page(Page::Discover);
    set_content(provider.content_type(), Some(provider));
}

fn set_content(content: ContentType, keep: Option<Provider>) {
    let changed = with(|s| s.content != content || s.items.is_empty());
    let (names, index) = with(|s| {
        let genres = if content == ContentType::Shows { &s.genres[1] } else { &s.genres[0] };
        s.content = content;
        s.providers = providers(content, genres);
        let wanted = keep.as_ref().map(label);
        let index = wanted.and_then(|w| s.providers.iter().position(|p| label(p) == w)).unwrap_or(0);
        (s.providers.iter().map(label).collect::<Vec<_>>(), index)
    });
    let w = ui::window();
    let d = w.global::<DiscoverState>();
    d.set_content(if content == ContentType::Shows { 1 } else { 0 });
    let model: Vec<SharedString> = names.into_iter().map(Into::into).collect();
    d.set_categories(ModelRc::from(Rc::new(VecModel::from(model))));
    select(index, changed);
}

/// Shows category `index`; reloads unless it is already shown (`force` reloads anyway).
fn select(index: usize, force: bool) {
    let same = with(|s| {
        let same = !force && s.current == index && !s.items.is_empty();
        s.current = index.min(s.providers.len().saturating_sub(1));
        if !same {
            s.seq += 1;
            s.items.clear();
            s.cards.clear();
            s.page = 0;
            s.done = false;
            s.loading = false;
        }
        same
    });
    ui::window().global::<DiscoverState>().set_category(index as i32);
    if !same {
        load_page();
    }
}

fn load_page() {
    let Some((provider, page, seq)) = with(|s| {
        if s.loading || s.done {
            return None;
        }
        s.loading = true;
        Some((s.providers.get(s.current)?.clone(), if s.items.is_empty() { 0 } else { s.page + 1 }, s.seq))
    }) else {
        return;
    };
    ui::window().global::<DiscoverState>().set_loading(true);
    spawn_ui(async move {
        let items = bg(async move { provider.load(page).await }).await;
        let stale = with(|s| {
            if s.seq != seq {
                return true;
            }
            s.loading = false;
            s.page = page;
            s.done = items.is_empty();
            s.cards.push_all(items.iter().map(crate::pages::home::spec).collect());
            s.items.extend(items);
            false
        });
        if !stale {
            ui::window().global::<DiscoverState>().set_loading(false);
        }
    });
}
