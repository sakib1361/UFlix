//! Online subtitle search for the playing video (provider from Settings); the picked one is downloaded and added.

use crate::runtime::{bg, spawn_ui};
use crate::ui::{self, SubtitleRow, SubtitlesState};
use engine::models::MediaItem;
use engine::subtitles::{self, SubtitleResult};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

type OnPick = Rc<dyn Fn(String)>;

thread_local! {
    static RESULTS: RefCell<Vec<SubtitleResult>> = const { RefCell::new(Vec::new()) };
    static ON_PICK: RefCell<Option<OnPick>> = const { RefCell::new(None) };
}

fn ss<R>(f: impl FnOnce(&SubtitlesState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<SubtitlesState>())
}

pub fn init() {
    ss(|s| {
        s.on_close(|| ss(|s| s.set_open(false)));
        s.on_pick(|i| pick(i as usize));
    });
}

/// Searches for `item`; `on_pick` gets the downloaded subtitle's path.
pub fn open(item: MediaItem, on_pick: impl Fn(String) + 'static) {
    ON_PICK.with(|p| *p.borrow_mut() = Some(Rc::new(on_pick)));
    ss(|s| {
        s.set_provider(subtitles::Provider::current().label().into());
        s.set_results(ModelRc::default());
        s.set_searching(true);
        s.set_downloading(-1);
        s.set_open(true);
    });
    spawn_ui(async move {
        let results = bg(async move { subtitles::search(&item).await }).await;
        let rows: Vec<SubtitleRow> = results
            .iter()
            .map(|r| SubtitleRow { name: r.name.clone().into(), release: r.release.clone().into(), meta: r.description.clone().into() })
            .collect();
        RESULTS.with(|x| *x.borrow_mut() = results);
        ss(|s| {
            s.set_searching(false);
            s.set_results(ModelRc::from(Rc::new(VecModel::from(rows))));
        });
    });
}

fn pick(index: usize) {
    let Some(result) = RESULTS.with(|r| r.borrow().get(index).cloned()) else { return };
    ss(|s| s.set_downloading(index as i32));
    spawn_ui(async move {
        let file = bg(async move { subtitles::download(&result).await }).await;
        ss(|s| s.set_downloading(-1));
        match file {
            Some(path) => {
                ss(|s| s.set_open(false));
                if let Some(cb) = ON_PICK.with(|p| p.borrow().clone()) {
                    cb(path.to_string_lossy().into_owned());
                }
                ui::toast("Subtitle added");
            }
            None => ui::toast("Couldn't download that subtitle"),
        }
    });
}
