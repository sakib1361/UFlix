//! Finds the subtitle files that belong to a video: next to it, in a Subs/Sub/Subtitles folder beside it, or in a folder
//! inside that one named after the video or with the same SxxEyy (RARBG style "Subs/<video name>/2_English.srt").

use crate::parse::parse_name;
use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt"];
const FOLDERS: &[&str] = &["subs", "sub", "subtitles", "subtitle"];

pub fn is_subtitle(path: &str) -> bool {
    Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn is_sub_folder(dir: &Path) -> bool {
    dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| FOLDERS.iter().any(|f| f.eq_ignore_ascii_case(n)))
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default()
}

fn dirs_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// Subtitles for a video on disk.
pub fn find(video: &Path, episode_hint: Option<&str>) -> Vec<PathBuf> {
    let Some(folder) = video.parent() else { return Vec::new() };
    let mut candidates = files_in(folder);
    for sub in dirs_in(folder).into_iter().filter(|d| is_sub_folder(d)) {
        candidates.extend(files_in(&sub));
        for inner in dirs_in(&sub) {
            candidates.extend(files_in(&inner));
        }
    }
    let as_strings: Vec<String> = candidates.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    select(&video.to_string_lossy(), &as_strings, episode_hint).into_iter().map(PathBuf::from).collect()
}

/// Picks the subtitles of `video` out of `paths` (e.g. a torrent's file list, '/'-separated relative paths work too).
pub fn select(video: &str, paths: &[String], episode_hint: Option<&str>) -> Vec<String> {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_string();
    let parent = |p: &str| -> String {
        let p = norm(p);
        p.rfind('/').map(|i| p[..i].to_string()).unwrap_or_default()
    };
    let file_name = |p: &str| norm(p).rsplit('/').next().unwrap_or("").to_string();
    let folder = parent(video);
    let video_name = Path::new(&file_name(video)).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut parsed = parse_name(&video_name);
    if parsed.episode.is_none()
        && let Some(hint) = episode_hint
    {
        parsed = parse_name(hint);
    }
    let sub_folder = |dir: &str| FOLDERS.iter().any(|f| f.eq_ignore_ascii_case(&file_name(dir)));
    let matches_video = |dir_name: &str| {
        if dir_name.eq_ignore_ascii_case(&video_name) {
            return true;
        }
        let Some(ep) = parsed.episode else { return false };
        let other = parse_name(dir_name);
        other.episode == Some(ep) && (other.season.is_none() || parsed.season.is_none() || other.season == parsed.season)
    };
    let (mut beside, mut in_subs, mut in_episode) = (Vec::new(), Vec::new(), Vec::new());
    for path in paths.iter().filter(|p| is_subtitle(p)) {
        let dir = parent(path);
        if dir == folder {
            beside.push(path.clone());
        } else if parent(&dir) == folder && sub_folder(&dir) {
            in_subs.push(path.clone());
        } else {
            let sub_dir = parent(&dir);
            if parent(&sub_dir) == folder && sub_folder(&sub_dir) && matches_video(&file_name(&dir)) {
                in_episode.push(path.clone());
            }
        }
    }
    // a season pack keeps every episode's subtitles in one folder: keep only those named after this video when any are
    let prefer_named = |files: Vec<String>| -> Vec<String> {
        let lower = video_name.to_lowercase();
        let named: Vec<String> = files.iter().filter(|f| file_name(f).to_lowercase().starts_with(&lower)).cloned().collect();
        if named.is_empty() { files } else { named }
    };
    in_episode.extend(prefer_named(beside));
    in_episode.extend(prefer_named(in_subs));
    in_episode
}
