//! UI-free torrent engine for UFlix on librqbit (pure Rust, no native library).
//!
//! [`Engine`] owns the session (resume state persisted under the app's data folder); [`Torrent`] is a handle to one
//! torrent; [`Torrent::stream`] opens a file for a player as a [`BlockingStream`], whose reads wait for pieces (librqbit
//! prioritises the pieces being read) and can be cancelled from another thread.

mod stream;

pub use stream::BlockingStream;

use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ListenerMode, ListenerOptions, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

#[derive(Clone, Debug)]
pub struct TorrentFile {
    pub index: usize,
    /// Relative to the torrent's output folder, '/'-separated.
    pub path: String,
    pub size: u64,
}

impl TorrentFile {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Checking,
    Downloading,
    Paused,
    Finished,
    Error,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub state: State,
    /// 0..1
    pub progress: f64,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub peers: u32,
    pub seeds: u32,
    /// Bytes per second.
    pub download_rate: f64,
    pub upload_rate: f64,
    pub eta: Option<std::time::Duration>,
    /// Downloaded bytes per file, by index.
    pub file_progress: Vec<u64>,
    pub error: Option<String>,
}

/// One torrent in the session (cheap to clone).
#[derive(Clone)]
pub struct Torrent {
    handle: Arc<ManagedTorrent>,
    session: Arc<Session>,
    rt: tokio::runtime::Handle,
}

impl Torrent {
    pub fn id(&self) -> usize {
        self.handle.id()
    }

    pub fn info_hash(&self) -> String {
        self.handle.info_hash().as_string()
    }

    pub fn name(&self) -> String {
        self.handle.name().unwrap_or_default()
    }

    pub fn output_folder(&self) -> PathBuf {
        self.handle.output_folder().to_path_buf()
    }

    pub fn files(&self) -> Vec<TorrentFile> {
        self.handle
            .with_metadata(|m| {
                m.file_infos
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| !f.attrs.padding)
                    .map(|(index, f)| TorrentFile { index, path: f.relative_filename.to_string_lossy().replace('\\', "/"), size: f.len })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Absolute path of a file on disk (it may still be incomplete).
    pub fn file_path(&self, file: &TorrentFile) -> PathBuf {
        self.output_folder().join(&file.path)
    }

    pub fn status(&self) -> Status {
        use librqbit::TorrentStatsState as S;
        let s = self.handle.stats();
        let state = match s.state {
            _ if s.error.is_some() => State::Error,
            _ if s.finished => State::Finished,
            S::Initializing { .. } => State::Checking,
            S::Live => State::Downloading,
            S::Paused => State::Paused,
            S::Error => State::Error,
        };
        let live = s.live.as_ref();
        let mbps = |m: f64| m * 1024.0 * 1024.0;
        let rate = live.map_or(0.0, |l| mbps(l.download_speed.mbps));
        let left = s.total_bytes.saturating_sub(s.progress_bytes);
        Status {
            eta: (rate > 1.0 && left > 0).then(|| std::time::Duration::from_secs_f64(left as f64 / rate)),
            state,
            progress: if s.total_bytes == 0 { 0.0 } else { s.progress_bytes as f64 / s.total_bytes as f64 },
            total_bytes: s.total_bytes,
            downloaded_bytes: s.progress_bytes,
            uploaded_bytes: s.uploaded_bytes,
            peers: live.map_or(0, |l| l.snapshot.peer_stats.live),
            seeds: live.map_or(0, |l| l.snapshot.peer_stats.live_seeders),
            download_rate: rate,
            upload_rate: live.map_or(0.0, |l| mbps(l.upload_speed.mbps)),
            file_progress: s.file_progress.clone(),
            error: s.error.clone(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.handle.stats().finished
    }

    pub fn is_paused(&self) -> bool {
        self.handle.is_paused()
    }

    pub async fn pause(&self) -> Result<()> {
        self.session.pause(&self.handle).await.map_err(err)
    }

    pub async fn resume(&self) -> Result<()> {
        self.session.unpause(&self.handle).await.map_err(err)
    }

    /// Downloads only these files (by index); the rest are skipped.
    pub async fn select_files(&self, indexes: &[usize]) -> Result<()> {
        let set: HashSet<usize> = indexes.iter().copied().collect();
        self.session.update_only_files(&self.handle, &set).await.map_err(err)
    }

    /// Opens a file for playback. Reads block until their pieces are downloaded; librqbit fetches the pieces at the
    /// read position first.
    pub async fn stream(&self, file: &TorrentFile) -> Result<BlockingStream> {
        let s = self.handle.clone().stream(file.index).await.map_err(err)?;
        Ok(BlockingStream::new(Box::new(s), file.size, self.rt.clone()))
    }
}

/// Session-wide counters (diagnostics).
#[derive(Clone, Debug)]
pub struct SessionStats {
    pub torrents: usize,
    pub live_peers: u32,
    pub fetched_bytes: u64,
    pub uploaded_bytes: u64,
}

/// The app-wide torrent session.
pub struct Engine {
    session: Arc<Session>,
    rt: tokio::runtime::Handle,
}

impl Engine {
    /// Forgets saved torrents whose info hash `keep` rejects, before [`Engine::start`] would restore and resume them:
    /// drops them from librqbit's `session.json` and deletes their `.torrent` / `.bitv` state files. Downloaded data
    /// stays on disk. A session file that can't be read is left alone.
    pub fn forget_saved(state_folder: &Path, keep: impl Fn(&str) -> bool) {
        let file = state_folder.join("session.json");
        let Ok(text) = std::fs::read_to_string(&file) else { return };
        let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&text) else { return };
        let Some(torrents) = json.get_mut("torrents").and_then(|t| t.as_object_mut()) else { return };
        let mut dropped = Vec::new();
        torrents.retain(|_, t| {
            let hash = t.get("info_hash").and_then(|h| h.as_str()).unwrap_or_default().to_ascii_lowercase();
            let kept = hash.is_empty() || keep(&hash);
            if !kept {
                dropped.push(hash);
            }
            kept
        });
        if dropped.is_empty() {
            return;
        }
        let Ok(text) = serde_json::to_string(&json) else { return };
        // write-then-rename so a crash can't leave a half-written session
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_err() || std::fs::rename(&tmp, &file).is_err() {
            return;
        }
        for hash in dropped {
            let _ = std::fs::remove_file(state_folder.join(format!("{hash}.torrent")));
            let _ = std::fs::remove_file(state_folder.join(format!("{hash}.bitv")));
        }
    }

    /// Starts the session. `state_folder` keeps the torrent list and fast-resume data between runs; torrents download
    /// into `default_folder` unless `add` names another folder. Must run inside a tokio runtime.
    pub async fn start(default_folder: &Path, state_folder: &Path) -> Result<Self> {
        std::fs::create_dir_all(default_folder).map_err(err)?;
        std::fs::create_dir_all(state_folder).map_err(err)?;
        let options = SessionOptions {
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json { folder: Some(state_folder.to_path_buf()) }),
            // accept incoming peers (and forward the port with UPnP), so torrents upload while they play or seed
            listen: Some(ListenerOptions { mode: ListenerMode::TcpOnly, enable_upnp_port_forwarding: true, ..Default::default() }),
            // librqbit's default is 128 peers per torrent with no session-wide cap: a few torrents then hold hundreds of
            // sockets. 50 is plenty for streaming and keeps several torrents well under the open-file limit.
            peer_limit: Some(50),
            ..Default::default()
        };
        let session = Session::new_with_opts(default_folder.to_path_buf(), options).await.map_err(err)?;
        Ok(Self { session, rt: tokio::runtime::Handle::current() })
    }

    fn wrap(&self, handle: Arc<ManagedTorrent>) -> Torrent {
        Torrent { handle, session: self.session.clone(), rt: self.rt.clone() }
    }

    /// Adds a magnet link, http(s) .torrent URL or local .torrent file, or returns the torrent if it is already in the
    /// session. Waits until its metadata (file list) is known. `only` selects the files to download (None = all).
    /// Files go under the session's folder: a multi-file torrent in a folder named after it, a single file as is
    /// (libtorrent's layout, so downloads made by either are found again).
    pub async fn add(&self, url: &str, only: Option<Vec<usize>>) -> Result<Torrent> {
        let source = if Path::new(url).is_file() { AddTorrent::from_local_filename(url).map_err(err)? } else { AddTorrent::from_url(url) };
        let options = AddTorrentOptions { overwrite: true, only_files: only, ..Default::default() };
        let response = self.session.add_torrent(source, Some(options)).await.map_err(err)?;
        let handle = match response {
            AddTorrentResponse::Added(_, h) | AddTorrentResponse::AlreadyManaged(_, h) => h,
            AddTorrentResponse::ListOnly(_) => return Err("torrent was only listed".into()),
        };
        handle.wait_until_initialized().await.map_err(err)?;
        Ok(self.wrap(handle))
    }

    pub fn torrents(&self) -> Vec<Torrent> {
        self.session.with_torrents(|it| it.map(|(_, h)| self.wrap(h.clone())).collect())
    }

    pub fn find(&self, info_hash: &str) -> Option<Torrent> {
        self.torrents().into_iter().find(|t| t.info_hash().eq_ignore_ascii_case(info_hash))
    }

    /// Removes a torrent from the session; its files stay on disk unless `delete_files`.
    pub async fn remove(&self, torrent: &Torrent, delete_files: bool) -> Result<()> {
        self.session.delete(torrent.id().into(), delete_files).await.map_err(err)
    }

    pub fn stats(&self) -> SessionStats {
        let s = self.session.stats_snapshot();
        SessionStats {
            torrents: self.session.with_torrents(|it| it.count()),
            live_peers: s.peers.live,
            fetched_bytes: s.counters.fetched_bytes,
            uploaded_bytes: s.counters.uploaded_bytes,
        }
    }

    pub async fn stop(&self) {
        self.session.stop().await;
    }
}

/// The btih info hash in a magnet link, lower-case, if any.
pub fn magnet_info_hash(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let start = lower.find("urn:btih:")? + 9;
    let hash: String = lower[start..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!hash.is_empty()).then_some(hash)
}

/// Video extensions UFlix plays from a torrent.
const MOVIE_EXTS: &[&str] = &[
    "mp4", "m4v", "m4e", "mkv", "mpg", "mpeg", "mpv", "mp4p", "mpe", "m1v", "m2ts", "m2p", "m2v", "movhd", "moov", "movie", "movx", "mjp",
    "mjpeg", "mjpg", "amv", "asf", "3gp", "ogm", "ogg", "vob", "ts", "rm", "3gp2", "3gpp", "3g2", "f4v", "f4a", "f4p", "f4b", "mts",
    "gifv", "avi", "mov", "flv", "wmv", "qt", "avchd", "swf", "cam", "nsv", "ram", "x264", "xvid", "wmx", "wvx", "wx", "video", "viv",
    "vivo", "vid", "dat", "bik", "bix", "dmf", "divx", "webm",
];

pub fn is_movie(path: &str) -> bool {
    Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| MOVIE_EXTS.iter().any(|m| m.eq_ignore_ascii_case(e)))
}

/// Case-insensitive compare where digit runs compare by value, so "ep2" sorts before "ep10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let trim = |s: &[u8]| -> Vec<u8> { s.iter().skip_while(|c| **c == b'0').copied().collect() };
            let (na, nb) = (trim(&a[si..i]), trim(&b[sj..j]));
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb)) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        match a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase()) {
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
            o => return o,
        }
    }
    (a.len() - i).cmp(&(b.len() - j)).then_with(|| a.cmp(b))
}

/// The torrent's video files, naturally sorted by path.
pub fn movies_sorted(files: &[TorrentFile]) -> Vec<TorrentFile> {
    let mut movies: Vec<TorrentFile> = files.iter().filter(|f| is_movie(&f.path)).cloned().collect();
    movies.sort_by(|x, y| natural_cmp(&x.path, &y.path));
    movies
}

/// "1.4 GB", "512 B".
pub fn readable_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::Engine;

    #[test]
    fn forget_saved_keeps_only_wanted() {
        let dir = std::env::temp_dir().join(format!("uflix-forget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = r#"{"torrents":{"0":{"info_hash":"aaaa","only_files":[0]},"1":{"info_hash":"bbbb","only_files":[1]}}}"#;
        std::fs::write(dir.join("session.json"), session).unwrap();
        for f in ["aaaa.torrent", "aaaa.bitv", "bbbb.torrent", "bbbb.bitv"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        Engine::forget_saved(&dir, |h| h == "aaaa");
        let text = std::fs::read_to_string(dir.join("session.json")).unwrap();
        assert!(text.contains("aaaa") && !text.contains("bbbb"));
        assert!(dir.join("aaaa.torrent").exists() && dir.join("aaaa.bitv").exists());
        assert!(!dir.join("bbbb.torrent").exists() && !dir.join("bbbb.bitv").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
