//! Card lists backed by a `VecModel`, with images filled in as they load.

use crate::images;
use crate::runtime::spawn_ui;
use crate::ui::Card;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// What a card shows, before its image is loaded.
#[derive(Default)]
pub struct CardSpec {
    pub title: String,
    pub subtitle: String,
    pub image_url: String,
    pub badge: String,
    pub progress: f32,
    pub detail: String,
    pub live: bool,
}

pub fn card(spec: &CardSpec) -> Card {
    Card {
        title: SharedString::from(spec.title.as_str()),
        subtitle: SharedString::from(spec.subtitle.as_str()),
        image: Default::default(),
        badge: SharedString::from(spec.badge.as_str()),
        progress: spec.progress,
        detail: SharedString::from(spec.detail.as_str()),
        live: spec.live,
    }
}

/// A list of cards whose images load in the background.
#[derive(Clone)]
pub struct CardList {
    model: Rc<VecModel<Card>>,
    /// Bumped on `clear`, so images of a previous list never land in a new one.
    generation: Rc<std::cell::Cell<u64>>,
    width: u32,
}

impl CardList {
    /// `width`: the image width to decode, in logical pixels (scaled for HiDPI).
    pub fn new(width: u32) -> Self {
        Self { model: Rc::new(VecModel::default()), generation: Default::default(), width }
    }

    pub fn model(&self) -> ModelRc<Card> {
        ModelRc::from(self.model.clone())
    }

    pub fn clear(&self) {
        self.generation.set(self.generation.get() + 1);
        self.model.set_vec(Vec::new());
    }

    pub fn remove(&self, index: usize) {
        if index < self.model.row_count() {
            self.model.remove(index);
        }
    }

    /// Changes a card in place (keeping its loaded image).
    pub fn update(&self, index: usize, f: impl FnOnce(&mut Card)) {
        if let Some(mut c) = self.model.row_data(index) {
            f(&mut c);
            self.model.set_row_data(index, c);
        }
    }

    pub fn push_all(&self, specs: Vec<CardSpec>) {
        let start = self.model.row_count();
        for spec in &specs {
            self.model.push(card(spec));
        }
        let scale = crate::ui::scale_factor();
        for (i, spec) in specs.into_iter().enumerate() {
            self.load_image(start + i, spec.image_url, (self.width as f32 * scale) as u32);
        }
    }

    fn load_image(&self, index: usize, url: String, width: u32) {
        if url.is_empty() {
            return;
        }
        let model = Rc::downgrade(&self.model);
        let generation = self.generation.clone();
        let expected = generation.get();
        spawn_ui(async move {
            let Some(img) = images::load(url, width).await else { return };
            let Some(model) = model.upgrade() else { return };
            if generation.get() != expected || index >= model.row_count() {
                return;
            }
            if let Some(mut c) = model.row_data(index) {
                c.image = img;
                model.set_row_data(index, c);
            }
        });
    }
}
