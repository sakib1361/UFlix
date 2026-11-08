//! Home: catalogue rows (loaded a few at a time as the page scrolls; each row pages in as it scrolls) and the TMDB
//! search with Movies/Shows results.

use crate::cards::{CardList, CardSpec};
use crate::runtime::{bg, sleep, spawn_ui};
use crate::ui::{self, HomeState, RowData};
use engine::models::{ContentType, MediaItem};
use engine::providers::Provider;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

const ROWS_PER_PAGE: usize = 6;

struct Row {
    provider: Provider,
    items: Vec<MediaItem>,
    cards: CardList,
    page: u32,
    loading: bool,
    done: bool,
}

#[derive(Default)]
struct State {
    rows: Vec<Row>,
    model: Rc<VecModel<RowData>>,
    /// Providers not shown yet (genres arrive after the fixed rows).
    queue: Vec<Provider>,
    loading_rows: bool,
    genres_loaded: bool,
    search_seq: u64,
    movie_results: Vec<MediaItem>,
    show_results: Vec<MediaItem>,
    movie_cards: Option<CardList>,
    show_cards: Option<CardList>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub fn spec(item: &MediaItem) -> CardSpec {
    let progress = item.progress() as f32;
    let subtitle = if progress > 0.0 {
        format!("{} · {:.0}% watched", if item.year.is_empty() { "Continue" } else { &item.year }, progress * 100.0)
    } else {
        item.year.clone()
    };
    CardSpec { title: item.title.clone(), subtitle, image_url: item.image.clone(), badge: String::new(), progress, detail: String::new() }
}

pub fn init() {
    let w = ui::window();
    let home = w.global::<HomeState>();
    let (model, movie_cards, show_cards) = with(|s| {
        s.movie_cards = Some(CardList::new(86));
        s.show_cards = Some(CardList::new(86));
        (s.model.clone(), s.movie_cards.clone().unwrap(), s.show_cards.clone().unwrap())
    });
    home.set_rows(ModelRc::from(model));
    home.set_movie_results(movie_cards.model());
    home.set_show_results(show_cards.model());
    home.on_rows_near_end(load_more_rows);
    home.on_row_near_end(|r| load_row_page(r as usize));
    home.on_open(|r, i| {
        if let Some(item) = with(|s| s.rows.get(r as usize).and_then(|row| row.items.get(i as usize).cloned())) {
            crate::open::open_media(item);
        }
    });
    home.on_remove(|r, i| remove_item(r as usize, i as usize));
    home.on_explore(|r| {
        if let Some(p) = with(|s| s.rows.get(r as usize).map(|row| row.provider.clone())) {
            crate::pages::discover::show_provider(p);
        }
    });
    home.on_search_edited(|text| search(text.to_string()));
    home.on_open_result(|tab, i| {
        let item = with(|s| if tab == 0 { s.movie_results.get(i as usize).cloned() } else { s.show_results.get(i as usize).cloned() });
        if let Some(item) = item {
            crate::open::open_media(item);
        }
    });
    with(|s| s.queue = Provider::home().into_iter().rev().collect());
    load_more_rows();
}

/// Adds the next few rows (fetching TMDB's genres once the fixed rows are used up).
fn load_more_rows() {
    if with(|s| std::mem::replace(&mut s.loading_rows, true)) {
        return;
    }
    ui::window().global::<HomeState>().set_loading(true);
    spawn_ui(async {
        if with(|s| s.queue.is_empty() && !s.genres_loaded) {
            let (movies, shows) =
                bg(async { tokio::join!(engine::tmdb::genres(ContentType::Movie), engine::tmdb::genres(ContentType::Shows)) }).await;
            let mut genres = Vec::new();
            for i in 0..movies.len().max(shows.len()) {
                if let Some(g) = movies.get(i) {
                    genres.push(Provider::Genre(ContentType::Movie, g.clone()));
                }
                if let Some(g) = shows.get(i) {
                    genres.push(Provider::Genre(ContentType::Shows, g.clone()));
                }
            }
            with(|s| {
                s.genres_loaded = true;
                s.queue = genres.into_iter().rev().collect();
            });
        }
        let batch: Vec<Provider> = with(|s| (0..ROWS_PER_PAGE).filter_map(|_| s.queue.pop()).collect());
        for provider in batch {
            add_row(provider).await;
        }
        with(|s| s.loading_rows = false);
        ui::window().global::<HomeState>().set_loading(false);
    });
}

async fn add_row(provider: Provider) {
    let p = provider.clone();
    let items = bg(async move { p.load(0).await }).await;
    if items.is_empty() {
        return; // an empty watchlist or a failed catalogue isn't shown
    }
    let (wide, can_delete) = (false, provider.can_delete());
    let cards = CardList::new(if wide { 296 } else { 172 });
    cards.push_all(items.iter().map(spec).collect());
    let data = RowData {
        title: provider.name().into(),
        cards: cards.model(),
        can_delete,
        can_explore: !matches!(provider, Provider::Watchlist),
        loading: false,
    };
    with(|s| {
        s.rows.push(Row { provider, items, cards, page: 0, loading: false, done: false });
        s.model.push(data);
    });
}

fn set_row_loading(index: usize, loading: bool) {
    with(|s| {
        if let Some(mut d) = s.model.row_data(index) {
            d.loading = loading;
            s.model.set_row_data(index, d);
        }
    });
}

/// The next page of one row, when it scrolls near its end.
fn load_row_page(index: usize) {
    let Some((provider, page)) = with(|s| {
        let row = s.rows.get_mut(index)?;
        if row.loading || row.done {
            return None;
        }
        row.loading = true;
        Some((row.provider.clone(), row.page + 1))
    }) else {
        return;
    };
    set_row_loading(index, true);
    spawn_ui(async move {
        let items = bg(async move { provider.load(page).await }).await;
        with(|s| {
            if let Some(row) = s.rows.get_mut(index) {
                row.loading = false;
                row.page = page;
                row.done = items.is_empty();
                row.cards.push_all(items.iter().map(spec).collect());
                row.items.extend(items);
            }
        });
        set_row_loading(index, false);
    });
}

fn remove_item(row: usize, index: usize) {
    with(|s| {
        if let Some(r) = s.rows.get_mut(row)
            && index < r.items.len()
        {
            let item = r.items.remove(index);
            engine::watchlist::remove(&item);
            r.cards.remove(index);
        }
    });
}

/// Reloads "Continue watching" after the player closes.
pub fn refresh_watchlist() {
    spawn_ui(async {
        let items = engine::watchlist::items();
        let existing = with(|s| s.rows.iter().position(|r| matches!(r.provider, Provider::Watchlist)));
        match existing {
            Some(i) if items.is_empty() => with(|s| {
                s.rows.remove(i);
                s.model.remove(i);
            }),
            Some(i) => with(|s| {
                let row = &mut s.rows[i];
                row.cards.clear();
                row.cards.push_all(items.iter().map(spec).collect());
                row.items = items;
            }),
            None if !items.is_empty() => {
                let cards = CardList::new(172);
                cards.push_all(items.iter().map(spec).collect());
                let data = RowData {
                    title: Provider::Watchlist.name().into(),
                    cards: cards.model(),
                    can_delete: true,
                    can_explore: false,
                    loading: false,
                };
                with(|s| {
                    s.rows.insert(0, Row { provider: Provider::Watchlist, items, cards, page: 0, loading: false, done: true });
                    s.model.insert(0, data);
                });
            }
            None => {}
        }
    });
}

/// TMDB search as the user types (after a short pause), movies and shows at once.
fn search(text: String) {
    let seq = with(|s| {
        s.search_seq += 1;
        s.search_seq
    });
    let w = ui::window();
    let home = w.global::<HomeState>();
    if text.trim().len() < 3 {
        with(|s| {
            s.movie_results.clear();
            s.show_results.clear();
            s.movie_cards.as_ref().unwrap().clear();
            s.show_cards.as_ref().unwrap().clear();
        });
        home.set_searching(false);
        return;
    }
    spawn_ui(async move {
        sleep(350).await;
        if with(|s| s.search_seq) != seq {
            return;
        }
        let w = ui::window();
        w.global::<HomeState>().set_searching(true);
        let q = text.clone();
        let (movies, shows) = bg(async move {
            tokio::join!(engine::tmdb::search(ContentType::Movie, &q, None), engine::tmdb::search(ContentType::Shows, &q, None))
        })
        .await;
        if with(|s| s.search_seq) != seq {
            return;
        }
        let home = w.global::<HomeState>();
        home.set_searching(false);
        // switch to the tab that has results
        if home.get_tab() == 0 && movies.is_empty() && !shows.is_empty() {
            home.set_tab(1);
        } else if home.get_tab() == 1 && shows.is_empty() && !movies.is_empty() {
            home.set_tab(0);
        }
        let to_spec = |m: &MediaItem| CardSpec {
            title: m.title.clone(),
            subtitle: if m.rating > 0.0 { format!("{}  ·  ★ {:.1}", m.year, m.rating) } else { m.year.clone() },
            image_url: m.image.clone(),
            badge: String::new(),
            progress: 0.0,
            detail: String::new(),
        };
        with(|s| {
            let mc = s.movie_cards.clone().unwrap();
            let sc = s.show_cards.clone().unwrap();
            mc.clear();
            sc.clear();
            mc.push_all(movies.iter().map(to_spec).collect());
            sc.push_all(shows.iter().map(to_spec).collect());
            s.movie_results = movies;
            s.show_results = shows;
        });
    });
}
