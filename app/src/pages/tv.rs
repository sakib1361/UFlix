//! Live TV: every channel of the configured M3U playlists, filterable by name. After loading, every stream is
//! probed (10 at a time) and each card shows whether its channel is live; a channel with several streams opens a
//! picker, otherwise it plays straight away.

use crate::cards::{CardList, CardSpec};
use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, TvState, TvStream};
use engine::models::{ContentType, MediaItem, MediaType};
use engine::tv::{Channel, Recent};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Checking,
    Live,
    Offline,
}

#[derive(Default)]
struct State {
    all: Vec<Channel>,
    /// Per channel, per stream.
    status: Vec<Vec<Status>>,
    /// Indices into `all`, in card order.
    shown: Vec<usize>,
    cards: Option<CardList>,
    /// The last channels played (newest first) and their cards.
    recent: Vec<Recent>,
    recent_cards: Option<CardList>,
    loaded: bool,
    /// Bumped on every load so results of an older probe run are dropped.
    generation: u64,
    probes: Option<tokio::task::AbortHandle>,
    checked: usize,
    /// The channel open in the picker.
    picker: Option<usize>,
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
    let recent_cards = CardList::new(168);
    tv(|t| {
        t.set_channels(cards.model());
        t.set_recent(recent_cards.model());
        t.on_open_recent(open_recent);
        t.on_remove_recent(|i| {
            engine::tv::remove_recent(i as usize);
            show_recent();
        });
        t.on_filter_edited(|f| apply_filter(&f));
        t.on_open(open);
        t.on_pick(pick);
        t.on_refresh(|| load(true));
    });
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.cards = Some(cards);
        s.recent_cards = Some(recent_cards);
    });
    show_recent();
}

/// Loads the channels the first time the page is shown (or again on refresh).
pub fn shown() {
    if !STATE.with(|s| s.borrow().loaded) {
        load(false);
    }
}

/// The playlists in Settings changed: reload if the page was already loaded (else it loads when shown).
pub fn links_changed() {
    if STATE.with(|s| s.borrow().loaded) {
        load(false);
    }
}

fn load(force: bool) {
    if force {
        engine::tv::clear_cache();
    }
    let generation = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.loaded = true;
        s.generation += 1;
        if let Some(p) = s.probes.take() {
            p.abort();
        }
        s.generation
    });
    tv(|t| {
        t.set_loading(true);
        t.set_picker_open(false);
    });
    spawn_ui(async move {
        let channels = bg(engine::tv::channels()).await;
        if STATE.with(|s| s.borrow().generation) != generation {
            return;
        }
        let urls: Vec<String> = channels.iter().flat_map(|c| c.streams.iter().map(|s| s.url.clone())).collect();
        let streams = urls.len();
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.status = channels.iter().map(|c| vec![Status::Checking; c.streams.len()]).collect();
            s.all = channels;
            s.checked = 0;
        });
        tv(|t| {
            t.set_loading(false);
            t.set_checked(0);
            t.set_streams(streams as i32);
            t.set_live(0);
        });
        let filter = tv(|t| t.get_filter()).to_string();
        apply_filter(&filter);
        update_recent();
        start_probes(generation, urls);
    });
}

fn start_probes(generation: u64, urls: Vec<String>) {
    let task = crate::runtime::handle().spawn(engine::tv::probe_all(urls, move |i, ok| {
        crate::runtime::ui(move || probed(generation, i, ok));
    }));
    STATE.with(|s| s.borrow_mut().probes = Some(task.abort_handle()));
}

/// One probe finished: `flat` counts streams across all channels in order.
fn probed(generation: u64, flat: usize, ok: bool) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.generation != generation {
            return None;
        }
        let mut rest = flat;
        let ch = s.status.iter().position(|v| {
            if rest < v.len() {
                true
            } else {
                rest -= v.len();
                false
            }
        })?;
        s.status[ch][rest] = if ok { Status::Live } else { Status::Offline };
        s.checked += 1;
        Some(ch)
    });
    let Some(ch) = changed else { return };
    let (checked, live) = STATE.with(|s| {
        let s = s.borrow();
        (s.checked, s.status.iter().filter(|v| channel_status(v) == Status::Live).count())
    });
    tv(|t| {
        t.set_checked(checked as i32);
        t.set_live(live as i32);
    });
    refresh_card(ch);
    let recent_hit = STATE.with(|s| {
        let s = s.borrow();
        s.all[ch].streams.iter().any(|st| s.recent.iter().any(|r| r.url == st.url))
    });
    if recent_hit {
        update_recent();
    }
    if STATE.with(|s| s.borrow().picker) == Some(ch) {
        fill_picker(ch);
    }
}

/// Live if any stream works, offline once all failed.
fn channel_status(streams: &[Status]) -> Status {
    if streams.contains(&Status::Live) {
        Status::Live
    } else if streams.contains(&Status::Checking) {
        Status::Checking
    } else {
        Status::Offline
    }
}

fn subtitle(ch: &Channel, status: &[Status]) -> String {
    let state = match channel_status(status) {
        Status::Checking => "Checking…",
        Status::Live => "Live",
        Status::Offline => "Offline",
    };
    match ch.streams.len() {
        1 => state.to_string(),
        n => format!("{state} · {n} streams"),
    }
}

fn hidden(status: &[Status], hide_offline: bool) -> bool {
    hide_offline && channel_status(status) == Status::Offline
}

/// Updates (or, with "Hide offline", removes) the card of channel `ch`.
fn refresh_card(ch: usize) {
    let hide = tv(|t| t.get_hide_offline());
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(pos) = s.shown.iter().position(|&i| i == ch) else { return };
        let cards = s.cards.clone().unwrap();
        if hidden(&s.status[ch], hide) {
            s.shown.remove(pos);
            cards.remove(pos);
        } else {
            let text = subtitle(&s.all[ch], &s.status[ch]);
            let live = channel_status(&s.status[ch]) == Status::Live;
            cards.update(pos, |c| {
                c.subtitle = text.into();
                c.live = live;
            });
        }
    });
}

fn apply_filter(filter: &str) {
    let f = filter.trim().to_lowercase();
    let hide = tv(|t| t.get_hide_offline());
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let shown: Vec<usize> = (0..s.all.len())
            .filter(|&i| (f.is_empty() || s.all[i].name.to_lowercase().contains(&f)) && !hidden(&s.status[i], hide))
            .collect();
        let cards = s.cards.clone().unwrap();
        cards.clear();
        cards.push_all(
            shown
                .iter()
                .map(|&i| CardSpec {
                    title: s.all[i].name.clone(),
                    subtitle: subtitle(&s.all[i], &s.status[i]),
                    live: channel_status(&s.status[i]) == Status::Live,
                    image_url: s.all[i].logo.clone(),
                    ..Default::default()
                })
                .collect(),
        );
        let total = s.all.len();
        s.shown = shown;
        tv(|t| t.set_total(total as i32));
    });
}

fn open(card: i32) {
    let Some(ch) = STATE.with(|s| s.borrow().shown.get(card as usize).copied()) else { return };
    let single = STATE.with(|s| s.borrow().all[ch].streams.len() == 1);
    if single {
        play(ch, 0);
    } else {
        STATE.with(|s| s.borrow_mut().picker = Some(ch));
        fill_picker(ch);
        tv(|t| t.set_picker_open(true));
    }
}

fn fill_picker(ch: usize) {
    let (name, rows) = STATE.with(|s| {
        let s = s.borrow();
        let c = &s.all[ch];
        let rows: Vec<TvStream> = c
            .streams
            .iter()
            .zip(&s.status[ch])
            .map(|(st, status)| TvStream {
                label: st.label.clone().into(),
                host: host(&st.url).into(),
                status: match status {
                    Status::Checking => 0,
                    Status::Live => 1,
                    Status::Offline => 2,
                },
            })
            .collect();
        (c.name.clone(), rows)
    });
    tv(|t| {
        t.set_picker_name(name.into());
        t.set_picker_streams(ModelRc::from(Rc::new(VecModel::from(rows))));
    });
}

fn pick(stream: i32) {
    let Some(ch) = STATE.with(|s| s.borrow_mut().picker.take()) else { return };
    tv(|t| t.set_picker_open(false));
    play(ch, stream as usize);
}

fn play(ch: usize, stream: usize) {
    let entry = STATE.with(|s| {
        let s = s.borrow();
        let c = s.all.get(ch)?;
        let st = c.streams.get(stream)?;
        Some(Recent { name: c.name.clone(), logo: c.logo.clone(), label: st.label.clone(), url: st.url.clone() })
    });
    if let Some(entry) = entry {
        play_entry(entry);
    }
}

/// Plays a recently watched channel's stream again.
fn open_recent(i: i32) {
    if let Some(entry) = STATE.with(|s| s.borrow().recent.get(i as usize).cloned()) {
        play_entry(entry);
    }
}

fn play_entry(entry: Recent) {
    let title = if entry.label.is_empty() { entry.name.clone() } else { format!("{} ({})", entry.name, entry.label) };
    let item = MediaItem {
        title,
        image: entry.logo.clone(),
        url: entry.url.clone(),
        media_type: MediaType::Unknown,
        content_type: ContentType::Movie,
        ..Default::default()
    };
    engine::tv::add_recent(entry);
    show_recent();
    crate::open::play(item);
}

/// The status of a recent channel's stream, if it is in the loaded playlists.
fn recent_status(s: &State, r: &Recent) -> Option<Status> {
    s.all.iter().zip(&s.status).find_map(|(c, st)| c.streams.iter().position(|x| x.url == r.url).map(|i| st[i]))
}

fn recent_card(s: &State, r: &Recent) -> (String, bool) {
    let status = recent_status(s, r);
    let state = match status {
        Some(Status::Live) => "Live",
        Some(Status::Offline) => "Offline",
        Some(Status::Checking) => "Checking…",
        None => "",
    };
    let text = [state, r.label.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" · ");
    (text, status == Some(Status::Live))
}

/// Rebuilds the "Recently watched" row from the store.
fn show_recent() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.recent = engine::tv::recent();
        let specs = s
            .recent
            .iter()
            .map(|r| {
                let (subtitle, live) = recent_card(&s, r);
                CardSpec { title: r.name.clone(), subtitle, live, image_url: r.logo.clone(), ..Default::default() }
            })
            .collect();
        let cards = s.recent_cards.clone().unwrap();
        cards.clear();
        cards.push_all(specs);
    });
}

/// Refreshes the status of the recent cards in place (keeping their logos).
fn update_recent() {
    STATE.with(|s| {
        let s = s.borrow();
        let cards = s.recent_cards.clone().unwrap();
        for (i, r) in s.recent.iter().enumerate() {
            let (text, live) = recent_card(&s, r);
            cards.update(i, |c| {
                c.subtitle = text.into();
                c.live = live;
            });
        }
    });
}

fn host(url: &str) -> String {
    url.split("://").nth(1).and_then(|r| r.split(['/', '?']).next()).unwrap_or(url).to_string()
}

/// For the screenshot tour: the picker of the first shown channel with several streams.
pub fn open_first_picker() {
    tv(|t| t.set_filter("".into()));
    apply_filter("");
    let card = STATE.with(|s| {
        let s = s.borrow();
        s.shown.iter().position(|&i| s.all[i].streams.len() > 1)
    });
    if let Some(card) = card {
        open(card as i32);
    }
}
