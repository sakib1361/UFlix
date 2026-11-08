//! Download: a video's formats from yt-dlp, then the download with live progress (yt-dlp `--newline` output); each
//! download can be cancelled. Files go to the YouTube folder from Settings.

use crate::runtime::{bg, handle, spawn_ui, ui as on_ui};
use crate::ui::{self, DownloadJob, DownloadState};
use serde::Deserialize;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
struct Format {
    format_id: String,
    ext: String,
    height: Option<u32>,
    fps: Option<f64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    format_note: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Info {
    title: String,
    formats: Vec<Format>,
}

struct Job {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
}

struct State {
    jobs: Rc<VecModel<DownloadJob>>,
    handles: Vec<Job>,
    /// (label, yt-dlp format selector)
    choices: Vec<(String, String)>,
    title: String,
    url: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn ds<R>(f: impl FnOnce(&DownloadState) -> R) -> R {
    let w = ui::window();
    f(&w.global::<DownloadState>())
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("download::init")))
}

pub fn init() {
    let jobs = Rc::new(VecModel::default());
    ds(|d| {
        d.set_jobs(ModelRc::from(jobs.clone()));
        d.on_close(|| ds(|d| d.set_open(false)));
        d.on_parse(parse);
        d.on_download(start);
        d.on_cancel(|i| cancel(i as usize));
        d.on_open_folder(|| {
            let folder = engine::paths::youtube_folder();
            let _ = std::fs::create_dir_all(&folder);
            let _ = open::that_detached(folder);
        });
    });
    STATE.with(|s| {
        *s.borrow_mut() = Some(State { jobs, handles: Vec::new(), choices: Vec::new(), title: String::new(), url: String::new() })
    });
}

pub fn open() {
    ds(|d| d.set_open(true));
}

fn parse() {
    let url = ds(|d| d.get_url()).trim().to_string();
    if url.is_empty() {
        return;
    }
    ds(|d| {
        d.set_parsing(true);
        d.set_formats(ModelRc::default());
    });
    spawn_ui(async move {
        let u = url.clone();
        let info = bg(async move {
            engine::tools::ensure_tools().await.map_err(|e| e.to_string())?;
            let out = tokio::process::Command::new(engine::tools::yt_dlp_path())
                .args(["-J", "--no-warnings", "--no-playlist", &u])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("yt-dlp failed").to_string());
            }
            serde_json::from_slice::<Info>(&out.stdout).map_err(|e| e.to_string())
        })
        .await;
        ds(|d| d.set_parsing(false));
        let info = match info {
            Ok(i) => i,
            Err(e) => return ui::toast(format!("Couldn't read the link: {e}")),
        };
        let choices = choices(&info);
        let labels: Vec<SharedString> = choices.iter().map(|(l, _)| l.as_str().into()).collect();
        with(|s| {
            s.choices = choices;
            s.title = info.title.clone();
            s.url = url;
        });
        ds(|d| {
            d.set_title(info.title.into());
            d.set_formats(ModelRc::from(Rc::new(VecModel::from(labels))));
            d.set_format(0);
        });
    });
}

/// "Best", one entry per video height (merged with the best audio), and "Audio only".
fn choices(info: &Info) -> Vec<(String, String)> {
    let mut list = vec![("Best quality".to_string(), "bestvideo+bestaudio/best".to_string())];
    let mut heights: Vec<(u32, &Format)> =
        info.formats.iter().filter(|f| f.vcodec.as_deref().is_some_and(|v| v != "none")).filter_map(|f| f.height.map(|h| (h, f))).collect();
    heights.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.fps.unwrap_or(0.0).total_cmp(&a.1.fps.unwrap_or(0.0))));
    heights.dedup_by_key(|(h, _)| *h);
    for (h, f) in heights {
        let size = f.filesize.or(f.filesize_approx).map(|b| format!(" · {}", torrent::readable_size(b))).unwrap_or_default();
        let note = f.format_note.clone().filter(|n| !n.is_empty()).map(|n| format!(" · {n}")).unwrap_or_default();
        let selector =
            if f.acodec.as_deref().is_some_and(|a| a != "none") { f.format_id.clone() } else { format!("{}+bestaudio/best", f.format_id) };
        list.push((format!("{h}p {}{note}{size}", f.ext), selector));
    }
    list.push(("Audio only".to_string(), "bestaudio/best".to_string()));
    list
}

fn start() {
    let index = ds(|d| d.get_format()) as usize;
    let Some((selector, title, url, jobs)) =
        with(|s| s.choices.get(index).map(|c| (c.1.clone(), s.title.clone(), s.url.clone(), s.jobs.clone())))
    else {
        return;
    };
    let row = jobs.row_count();
    jobs.push(DownloadJob { title: title.clone().into(), state: "Starting…".into(), progress: 0.0, done: false, failed: false });
    let child: Arc<Mutex<Option<tokio::process::Child>>> = Arc::default();
    with(|s| s.handles.push(Job { child: child.clone() }));
    ds(|d| {
        d.set_formats(ModelRc::default());
        d.set_url("".into());
    });
    let folder = engine::paths::youtube_folder();
    let _ = std::fs::create_dir_all(&folder);
    let update = move |state: String, progress: Option<f32>, done: bool, failed: bool| {
        on_ui(move || {
            let jobs = with(|s| s.jobs.clone());
            if let Some(mut j) = jobs.row_data(row) {
                j.state = state.into();
                if let Some(p) = progress {
                    j.progress = p;
                }
                j.done = done;
                j.failed = failed;
                jobs.set_row_data(row, j);
            }
        });
    };
    handle().spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut cmd = tokio::process::Command::new(engine::tools::yt_dlp_path());
        cmd.args(["--newline", "--no-playlist", "-f", &selector, "--ffmpeg-location"])
            .arg(engine::tools::tools_ffmpeg_dir())
            .arg("-o")
            .arg(folder.join("%(title)s.%(ext)s"))
            .arg(&url)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut spawned = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return update(format!("yt-dlp: {e}"), None, false, true),
        };
        let stdout = spawned.stdout.take();
        *child.lock().unwrap() = Some(spawned);
        if let Some(out) = stdout {
            let mut lines = tokio::io::BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // "[download]  42.0% of ~ 120.33MiB at  5.21MiB/s ETA 00:14"
                if let Some(rest) = line.strip_prefix("[download]") {
                    let rest = rest.trim();
                    let pct = rest.split('%').next().and_then(|p| p.trim().parse::<f32>().ok());
                    update(rest.to_string(), pct.map(|p| p / 100.0), false, false);
                } else if line.starts_with("[Merger]") {
                    update("Merging audio and video…".into(), None, false, false);
                }
            }
        }
        let status = {
            let taken = child.lock().unwrap().take();
            match taken {
                Some(mut c) => c.wait().await.ok(),
                None => None,
            }
        };
        match status {
            Some(s) if s.success() => update(format!("Saved to {}", folder.display()), Some(1.0), true, false),
            Some(_) => update("Failed (see yt-dlp)".into(), None, false, true),
            None => update("Cancelled".into(), None, false, true),
        }
    });
}

fn cancel(index: usize) {
    let (jobs, child) = with(|s| (s.jobs.clone(), s.handles.get(index).map(|j| j.child.clone())));
    let running = jobs.row_data(index).is_some_and(|j| !j.done && !j.failed);
    if running {
        if let Some(child) = child
            && let Some(mut c) = child.lock().unwrap().take()
        {
            let _ = c.start_kill();
        }
    } else if index < jobs.row_count() {
        jobs.remove(index);
        with(|s| {
            if index < s.handles.len() {
                s.handles.remove(index);
            }
        });
    }
}
