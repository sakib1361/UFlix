//! The generated Slint UI and access to the main window from UI-thread code.

slint::include_modules!();

use slint::ComponentHandle;
use std::cell::RefCell;

thread_local! {
    static WINDOW: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
    static TOAST_TIMER: slint::Timer = slint::Timer::default();
}

pub fn set_window(w: &AppWindow) {
    WINDOW.with(|cell| *cell.borrow_mut() = Some(w.as_weak()));
}

/// The main window (UI thread only).
pub fn window() -> AppWindow {
    WINDOW.with(|cell| cell.borrow().as_ref().and_then(|w| w.upgrade())).expect("main window")
}

pub fn scale_factor() -> f32 {
    WINDOW.with(|cell| cell.borrow().as_ref().and_then(|w| w.upgrade()).map(|w| w.window().scale_factor())).unwrap_or(1.0)
}

/// Shows a short message at the bottom of the window for 4 s.
pub fn toast(message: impl Into<String>) {
    let w = window();
    w.global::<Shell>().set_toast(message.into().into());
    TOAST_TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(4), || {
            window().global::<Shell>().set_toast("".into());
        })
    });
}

/// Toast from any thread.
pub fn toast_from_any(message: impl Into<String>) {
    let m = message.into();
    crate::runtime::ui(move || toast(m));
}

pub fn set_busy(busy: bool) {
    window().global::<Shell>().set_busy(busy);
}
