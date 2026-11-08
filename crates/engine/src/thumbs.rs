//! Thumbnails of local videos: ffprobe's duration, then ffmpeg's frame at 10 %, cached as a JPEG in `<data>/Thumbs`.

use std::path::{Path, PathBuf};

/// The cached thumbnail of `video`, making it first if needed (None without ffmpeg or on failure).
pub async fn get(video: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(video).ok()?;
    let stamp = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let key = format!("{:x}", md5::compute(format!("{}|{}|{stamp}", video.display(), meta.len())));
    let out = crate::paths::thumbs_folder().join(format!("{key}.jpg"));
    if out.is_file() {
        return Some(out);
    }
    std::fs::create_dir_all(crate::paths::thumbs_folder()).ok()?;
    crate::tools::ensure_tools().await.ok()?;
    let probe = tokio::process::Command::new(crate::tools::ffprobe_path())
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(video)
        .output()
        .await
        .ok()?;
    let duration: f64 = String::from_utf8_lossy(&probe.stdout).trim().parse().unwrap_or(0.0);
    let status = tokio::process::Command::new(crate::tools::ffmpeg_path())
        .args(["-v", "error", "-y", "-ss", &format!("{:.1}", duration * 0.1), "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf", "scale=480:-2", "-q:v", "4"])
        .arg(&out)
        .status()
        .await
        .ok()?;
    (status.success() && out.is_file()).then_some(out)
}

/// A path the image loader should treat as a video (not an image).
pub fn is_video_path(s: &str) -> bool {
    !s.starts_with("http") && crate::models::is_video(Path::new(s)) && Path::new(s).is_file()
}
