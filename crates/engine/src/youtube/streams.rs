//! Playable streams of a video: a quality list (video URL + separate audio URL for mpv's audio-add) and captions, from
//! rustypipe or from yt-dlp (`yt-dlp -J`, which also offers HLS).

use super::rp;
use rustypipe::model::{AudioCodec, VideoCodec};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct StreamOption {
    /// "1080p60 · VP9", "720p · HLS".
    pub label: String,
    pub url: String,
    pub audio_url: Option<String>,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct Caption {
    /// "English", "English (auto)".
    pub label: String,
    pub url: String,
    pub auto: bool,
}

#[derive(Clone, Debug, Default)]
pub struct StreamSet {
    /// Best first.
    pub options: Vec<StreamOption>,
    /// Manual captions first.
    pub captions: Vec<Caption>,
}

impl StreamSet {
    /// The best option no taller than `max_height` (0 = no limit).
    pub fn pick(&self, max_height: u32) -> Option<&StreamOption> {
        self.options.iter().find(|o| max_height == 0 || o.height <= max_height).or(self.options.last())
    }
}

fn codec_name(c: VideoCodec) -> &'static str {
    match c {
        VideoCodec::Avc1 => "H.264",
        VideoCodec::Vp9 => "VP9",
        VideoCodec::Av01 => "AV1",
        _ => "",
    }
}

fn with_vtt(url: &str) -> String {
    if url.contains("fmt=") { url.to_string() } else { format!("{url}&fmt=vtt") }
}

fn sort_captions(captions: &mut [Caption]) {
    captions.sort_by(|a, b| a.auto.cmp(&b.auto).then_with(|| a.label.cmp(&b.label)));
}

/// Streams through rustypipe.
pub async fn streams(video_id: &str) -> anyhow::Result<StreamSet> {
    let p = rp().query().player(video_id).await?;
    let audio = |prefer_opus: bool| {
        p.audio_streams.iter().max_by_key(|a| ((a.codec == AudioCodec::Opus) == prefer_opus, a.bitrate)).map(|a| a.url.clone())
    };
    let mut options: Vec<StreamOption> = Vec::new();
    if p.details.is_live
        && let Some(hls) = &p.hls_manifest_url
    {
        options.push(StreamOption { label: "Live · HLS".into(), url: hls.clone(), audio_url: None, height: 0 });
    }
    let mut videos: Vec<_> = p.video_only_streams.iter().collect();
    videos.sort_by_key(|v| std::cmp::Reverse((v.height, v.fps, v.bitrate)));
    for v in videos {
        let fps = if v.fps > 30 { v.fps.to_string() } else { String::new() };
        let codec = codec_name(v.codec);
        let label = if codec.is_empty() { format!("{}p{fps}", v.height) } else { format!("{}p{fps} · {codec}", v.height) };
        if options.iter().any(|o| o.label == label) {
            continue;
        }
        let opus = v.codec == VideoCodec::Vp9 || v.codec == VideoCodec::Av01;
        options.push(StreamOption { label, url: v.url.clone(), audio_url: audio(opus), height: v.height });
    }
    for v in &p.video_streams {
        let label = format!("{}p · muxed", v.height);
        if !options.iter().any(|o| o.label == label) {
            options.push(StreamOption { label, url: v.url.clone(), audio_url: None, height: v.height });
        }
    }
    let mut captions: Vec<Caption> = p
        .subtitles
        .iter()
        .map(|s| Caption {
            label: if s.auto_generated { format!("{} (auto)", s.lang_name) } else { s.lang_name.clone() },
            url: with_vtt(&s.url),
            auto: s.auto_generated,
        })
        .collect();
    sort_captions(&mut captions);
    anyhow::ensure!(!options.is_empty(), "no playable streams");
    Ok(StreamSet { options, captions })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct YtdlpInfo {
    formats: Vec<YtdlpFormat>,
    subtitles: std::collections::HashMap<String, Vec<YtdlpSub>>,
    automatic_captions: std::collections::HashMap<String, Vec<YtdlpSub>>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct YtdlpFormat {
    url: String,
    height: Option<u32>,
    fps: Option<f64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    protocol: Option<String>,
    tbr: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct YtdlpSub {
    ext: String,
    url: String,
    name: Option<String>,
}

/// Streams through yt-dlp (downloaded on first use).
pub async fn streams_ytdlp(video_id: &str) -> anyhow::Result<StreamSet> {
    if !crate::tools::yt_dlp_path().exists() {
        crate::tools::download_yt_dlp().await?;
    }
    let out = tokio::process::Command::new(crate::tools::yt_dlp_path())
        .args(["-J", "--no-warnings", "--no-playlist", &format!("https://www.youtube.com/watch?v={video_id}")])
        .output()
        .await?;
    anyhow::ensure!(out.status.success(), "yt-dlp: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or(""));
    let info: YtdlpInfo = serde_json::from_slice(&out.stdout)?;
    let none = |c: &Option<String>| c.as_deref().is_none_or(|c| c == "none");
    let best_audio = info
        .formats
        .iter()
        .filter(|f| none(&f.vcodec) && !none(&f.acodec))
        .max_by(|a, b| a.tbr.unwrap_or(0.0).total_cmp(&b.tbr.unwrap_or(0.0)))
        .map(|f| f.url.clone());
    let mut formats: Vec<&YtdlpFormat> = info.formats.iter().filter(|f| !none(&f.vcodec) && f.height.is_some()).collect();
    formats.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then(b.fps.unwrap_or(0.0).total_cmp(&a.fps.unwrap_or(0.0)))
            .then(b.tbr.unwrap_or(0.0).total_cmp(&a.tbr.unwrap_or(0.0)))
    });
    let mut options: Vec<StreamOption> = Vec::new();
    for f in formats {
        let h = f.height.unwrap_or(0);
        let fps = f.fps.filter(|x| *x > 30.0).map(|x| format!("{}", x as u32)).unwrap_or_default();
        let hls = f.protocol.as_deref().is_some_and(|p| p.starts_with("m3u8"));
        let codec = f.vcodec.as_deref().unwrap_or("");
        let codec = if codec.starts_with("avc") {
            "H.264"
        } else if codec.starts_with("vp9") || codec.starts_with("vp09") {
            "VP9"
        } else if codec.starts_with("av01") {
            "AV1"
        } else {
            ""
        };
        let label = match (hls, codec.is_empty()) {
            (true, _) => format!("{h}p{fps} · HLS"),
            (false, true) => format!("{h}p{fps}"),
            (false, false) => format!("{h}p{fps} · {codec}"),
        };
        if options.iter().any(|o| o.label == label) {
            continue;
        }
        let audio = if none(&f.acodec) && !hls { best_audio.clone() } else { None };
        options.push(StreamOption { label, url: f.url.clone(), audio_url: audio, height: h });
    }
    let pick = |subs: &std::collections::HashMap<String, Vec<YtdlpSub>>, auto: bool| -> Vec<Caption> {
        subs.iter()
            .filter_map(|(lang, list)| {
                let s = list.iter().find(|s| s.ext == "vtt")?;
                let name = s.name.clone().unwrap_or_else(|| lang.clone());
                Some(Caption { label: if auto { format!("{name} (auto)") } else { name }, url: s.url.clone(), auto })
            })
            .collect()
    };
    let mut captions = pick(&info.subtitles, false);
    captions.extend(pick(&info.automatic_captions, true).into_iter().filter(|c| c.label.contains("English")));
    sort_captions(&mut captions);
    anyhow::ensure!(!options.is_empty(), "no playable streams");
    Ok(StreamSet { options, captions })
}
