//! Keeps the screen on while video plays. Linux: org.freedesktop.ScreenSaver.Inhibit on the session bus, else the
//! xdg-desktop-portal Inhibit (idle + suspend). Windows: SetThreadExecutionState. The D-Bus work runs off the UI thread.

use std::sync::Mutex;

#[cfg(target_os = "linux")]
enum Cookie {
    ScreenSaver(zbus::blocking::Connection, u32),
    Portal(zbus::blocking::Connection, zbus::zvariant::OwnedObjectPath),
}

#[cfg(target_os = "linux")]
static HELD: Mutex<Option<Cookie>> = Mutex::new(None);

/// `true` while a video plays (loaded and not paused).
pub fn set(on: bool) {
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        let mut held = HELD.lock().unwrap();
        match (on, held.is_some()) {
            (true, false) => *held = inhibit(),
            (false, true) => {
                if let Some(c) = held.take() {
                    release(c);
                }
            }
            _ => {}
        }
    });
    #[cfg(windows)]
    windows::set(on);
}

#[cfg(target_os = "linux")]
fn inhibit() -> Option<Cookie> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn.call_method(
        Some("org.freedesktop.ScreenSaver"),
        "/org/freedesktop/ScreenSaver",
        Some("org.freedesktop.ScreenSaver"),
        "Inhibit",
        &("UFlix", "Playing video"),
    );
    if let Ok(msg) = reply
        && let Ok(cookie) = msg.body().deserialize::<u32>()
    {
        return Some(Cookie::ScreenSaver(conn, cookie));
    }
    // the portal: flags 4 (suspend) | 8 (idle)
    let options: std::collections::HashMap<&str, zbus::zvariant::Value> = [("reason", zbus::zvariant::Value::from("Playing video"))].into();
    let msg = conn
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Inhibit"),
            "Inhibit",
            &("", 12u32, options),
        )
        .ok()?;
    let handle = msg.body().deserialize::<zbus::zvariant::OwnedObjectPath>().ok()?;
    Some(Cookie::Portal(conn, handle))
}

#[cfg(target_os = "linux")]
fn release(cookie: Cookie) {
    match cookie {
        Cookie::ScreenSaver(conn, c) => {
            let _ = conn.call_method(
                Some("org.freedesktop.ScreenSaver"),
                "/org/freedesktop/ScreenSaver",
                Some("org.freedesktop.ScreenSaver"),
                "UnInhibit",
                &(c,),
            );
        }
        Cookie::Portal(conn, handle) => {
            let _ = conn.call_method(
                Some("org.freedesktop.portal.Desktop"),
                handle.as_str(),
                Some("org.freedesktop.portal.Request"),
                "Close",
                &(),
            );
        }
    }
}

#[cfg(windows)]
mod windows {
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;

    /// Called on the UI thread (the state belongs to the calling thread).
    pub fn set(on: bool) {
        let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS };
        unsafe { SetThreadExecutionState(flags) };
    }
}
