//! External tools: yt-dlp (downloaded from its GitHub releases) and ffmpeg/ffprobe (the system copy linked in on
//! Linux, else downloaded from ffbinaries), all kept in `<data>/YT`. Plus file-name sanitising.

use crate::paths::tools_folder;
use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

pub fn yt_dlp_path() -> PathBuf {
    tools_folder().join(exe("yt-dlp"))
}

pub fn ffmpeg_path() -> PathBuf {
    tools_folder().join(exe("ffmpeg"))
}

pub fn ffprobe_path() -> PathBuf {
    tools_folder().join(exe("ffprobe"))
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut p = meta.permissions();
            p.set_mode(p.mode() | 0o111);
            let _ = std::fs::set_permissions(path, p);
        }
    }
    let _ = path;
}

/// Downloads whatever of yt-dlp, ffmpeg and ffprobe is missing.
pub async fn ensure_tools() -> anyhow::Result<()> {
    std::fs::create_dir_all(tools_folder())?;
    if !yt_dlp_path().exists() {
        download_yt_dlp().await?;
    }
    for (name, path) in [("ffmpeg", ffmpeg_path()), ("ffprobe", ffprobe_path())] {
        if !path.exists() && !link_system(name, &path) {
            download_ff(name, &path).await?;
        }
    }
    Ok(())
}

/// Re-downloads yt-dlp (YouTube changes often break old versions).
pub async fn download_yt_dlp() -> anyhow::Result<()> {
    let base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";
    let url = if cfg!(windows) {
        format!("{base}.exe")
    } else if cfg!(target_os = "macos") {
        format!("{base}_macos")
    } else {
        base.into()
    };
    let bytes = crate::http::get_bytes(&url).await?;
    std::fs::create_dir_all(tools_folder())?;
    std::fs::write(yt_dlp_path(), bytes)?;
    make_executable(&yt_dlp_path());
    Ok(())
}

/// Links the distro's ffmpeg/ffprobe into the tools folder (Linux/macOS).
fn link_system(name: &str, target: &Path) -> bool {
    #[cfg(unix)]
    {
        let Some(path) = std::env::var_os("PATH") else { return false };
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() && candidate != target {
                let _ = std::fs::remove_file(target);
                return std::os::unix::fs::symlink(&candidate, target).is_ok();
            }
        }
    }
    let _ = (name, target);
    false
}

#[derive(Deserialize)]
struct FfBinaries {
    bin: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

async fn download_ff(name: &str, target: &Path) -> anyhow::Result<()> {
    let info: FfBinaries = crate::http::client().get("https://ffbinaries.com/api/v1/version/latest").send().await?.json().await?;
    let os = if cfg!(windows) {
        "windows-64"
    } else if cfg!(target_os = "macos") {
        "osx-64"
    } else {
        "linux-64"
    };
    let url = info.bin.get(os).and_then(|b| b.get(name)).context("no ffbinaries build for this OS")?;
    let bytes = crate::http::get_bytes(url).await?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut entry = zip.by_index(0)?;
    let mut out = std::fs::File::create(target)?;
    std::io::copy(&mut entry, &mut out)?;
    make_executable(target);
    Ok(())
}

/// A string made safe as a file name (yt-dlp's sanitize_filename, non-restricted).
pub fn sanitize_file_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '?' | '\u{0}'..='\u{1f}' | '\u{7f}' => {}
            '"' => out.push('\''),
            ':' => out.push_str(" -"),
            '\\' | '/' | '|' | '*' | '<' | '>' => out.push('_'),
            c => out.push(c),
        }
    }
    let mut out = out.replace("__", "_").trim_matches('_').trim_start_matches('.').to_string();
    if out.starts_with('-') {
        out.replace_range(..1, "_");
    }
    if out.trim().is_empty() { "_".into() } else { out }
}

/// The folder holding ffmpeg/ffprobe, for yt-dlp's --ffmpeg-location.
pub fn tools_ffmpeg_dir() -> PathBuf {
    tools_folder()
}
