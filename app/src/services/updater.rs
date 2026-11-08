//! A small self-updater over GitHub releases (repo baked in at build time as UFLIX_UPDATE_REPO):
//! check `releases/latest`, download this OS's asset (verified against the release's `SHA256SUMS`), then
//! - Linux: atomically replace the running AppImage ($APPIMAGE) and restart it;
//! - Windows: run the downloaded installer silently and exit (it relaunches UFlix).
//!
//! Copies that can't replace themselves (development builds, unpacked folders) only link to the releases page.
//! The daily check is a one-shot timer re-armed after each check (first one 30 s after start).

use crate::runtime::{bg, spawn_ui};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

pub const REPO: Option<&str> = option_env!("UFLIX_UPDATE_REPO");

thread_local! {
    static TIMER: slint::Timer = slint::Timer::default();
    static PENDING: std::cell::RefCell<Option<Release>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub enum Outcome {
    UpToDate,
    Available(String),
    NotInstalled,
    Failed(String),
}

/// What this copy would replace: the AppImage on Linux, the installed exe on Windows.
fn target() -> Option<PathBuf> {
    REPO?;
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").map(PathBuf::from)
    } else if cfg!(windows) {
        std::env::current_exe().ok()
    } else {
        None
    }
}

/// Can this copy update itself?
pub fn available() -> bool {
    target().is_some()
}

pub fn releases_url() -> String {
    format!("https://github.com/{}/releases", REPO.unwrap_or("sakib1361/UFlix"))
}

fn version_tuple(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect()
}

fn asset_for(release: &Release) -> Option<&Asset> {
    let wanted: &[&str] = if cfg!(windows) { &["setup.exe"] } else { &[".appimage"] };
    release.assets.iter().find(|a| wanted.iter().any(|w| a.name.to_lowercase().ends_with(w)))
}

/// Checks GitHub for a newer release; a found one is kept for `install`.
pub async fn check() -> Outcome {
    let Some(repo) = REPO else { return Outcome::NotInstalled };
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let release = bg(async move {
        let resp =
            engine::http::client().get(url).header("Accept", "application/vnd.github+json").send().await.map_err(|e| e.to_string())?;
        resp.error_for_status().map_err(|e| e.to_string())?.json::<Release>().await.map_err(|e| e.to_string())
    })
    .await;
    match release {
        Err(e) => Outcome::Failed(e),
        Ok(r) if version_tuple(&r.tag_name) > version_tuple(env!("UFLIX_VERSION")) && asset_for(&r).is_some() => {
            let v = r.tag_name.trim_start_matches('v').to_string();
            PENDING.with(|p| *p.borrow_mut() = Some(r));
            if available() { Outcome::Available(v) } else { Outcome::NotInstalled }
        }
        Ok(_) => Outcome::UpToDate,
    }
}

/// Downloads the found update (progress 0..100), verifies it and applies it. Doesn't return on success.
pub async fn install(progress: impl Fn(f32) + Send + Sync + 'static) -> Result<(), String> {
    let release = PENDING.with(|p| p.borrow().clone()).ok_or("no update found yet")?;
    let target = target().ok_or("this copy can't update itself")?;
    let asset = asset_for(&release).cloned().ok_or("no download for this OS")?;
    let sums = release.assets.iter().find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS")).cloned();
    let file = bg(async move {
        let client = engine::http::client();
        let mut resp =
            client.get(&asset.browser_download_url).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
        let total = resp.content_length().unwrap_or(0);
        let mut data = Vec::with_capacity(total as usize);
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            data.extend_from_slice(&chunk);
            if total > 0 {
                progress(data.len() as f32 * 100.0 / total as f32);
            }
        }
        if let Some(sums) = sums {
            let text =
                client.get(&sums.browser_download_url).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
            let expected = text.lines().find(|l| l.ends_with(&asset.name)).and_then(|l| l.split_whitespace().next()).map(str::to_lowercase);
            let actual = format!("{:x}", Sha256::digest(&data));
            if expected.as_deref() != Some(actual.as_str()) {
                return Err("the download doesn't match the release's checksum".to_string());
            }
        }
        let dir = engine::paths::temp_folder();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(&asset.name);
        std::fs::write(&file, &data).map_err(|e| e.to_string())?;
        Ok::<_, String>(file)
    })
    .await?;
    apply(&file, &target)
}

#[cfg(target_os = "linux")]
fn apply(file: &std::path::Path, appimage: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // stage next to the target (same filesystem), then rename over it: the running image stays valid until exit
    let staged = appimage.with_extension("new");
    std::fs::copy(file, &staged).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    std::fs::rename(&staged, appimage).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new(appimage).spawn();
    crate::player::close();
    std::process::exit(0);
}

#[cfg(windows)]
fn apply(file: &std::path::Path, _exe: &std::path::Path) -> Result<(), String> {
    // the installer (Inno Setup) runs silently, closes nothing of ours (we exit) and starts UFlix again
    std::process::Command::new(file).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).spawn().map_err(|e| e.to_string())?;
    crate::player::close();
    std::process::exit(0);
}

#[cfg(not(any(target_os = "linux", windows)))]
fn apply(_file: &std::path::Path, _target: &std::path::Path) -> Result<(), String> {
    Err("updates aren't supported on this OS".into())
}

/// The daily background check (when enabled in Settings).
pub fn schedule(first: Duration) {
    if REPO.is_none() {
        return;
    }
    TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, first, || {
            spawn_ui(async {
                if engine::settings::check_updates_daily()
                    && let Outcome::Available(v) = check().await
                {
                    crate::ui::toast(format!("UFlix {v} is available: Settings → Updates"));
                }
                schedule(Duration::from_secs(24 * 3600));
            });
        })
    });
}
