//! Release-name parser ("Show.Name.S01E02.1080p.WEB-DL.x264-GRP.mkv" → title, season, episode, year, ...).
//! Each pattern that matches is recorded; the title is the text before the first match that isn't at the start.

use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedName {
    pub name: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub resolution: Option<String>,
    pub quality: Option<String>,
    pub codec: Option<String>,
    pub group: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Flag,
    Text,
    Number,
    Year,
    Season,
    Episode,
    Resolution,
    Quality,
    Codec,
    Group,
}

struct Pattern {
    field: Field,
    regex: Regex,
    alternate: Option<Regex>,
    replacements: &'static [(&'static str, &'static str)],
}

fn patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        let p = |field, re: &str, ci: bool, alt: Option<&str>, replacements: &'static [(&'static str, &'static str)]| Pattern {
            field,
            regex: RegexBuilder::new(re).case_insensitive(ci).build().expect("pattern"),
            alternate: alt.map(|a| Regex::new(a).expect("pattern")),
            replacements,
        };
        use Field::*;
        // order matters: it is the order the C# parser applied them in
        vec![
            p(Flag, "AMZN", false, None, &[]),
            p(Text, r"MP3|DDP?\+?|Dual[\- ]Audio|LiNE|D[Tt][Ss](?:-?6[Cc][Hh])?(?:-?HD)?(?:[ \.]?MA)?|AAC(?:\.?2\.0)?|[Aa][Cc]-?3(?:\s?DD)?", false, None, &[(".", " ")]),
            p(Text, r"[257][\s\.][01]", false, None, &[(" ", ".")]),
            p(Number, r"((\d+)\s?bit)", true, None, &[]),
            p(Flag, "BLURRED", false, None, &[]),
            p(Codec, r"xvid|[xh]\.?26[45]|hevc", true, None, &[]),
            p(Flag, "COMPLETE", false, None, &[]),
            p(Text, r"MKV|AVI|MP4|mkv|avi|mp4", false, None, &[]),
            p(Flag, r"ATMOS|Atmos\b", false, None, &[]),
            p(Flag, "DUBBED", false, None, &[]),
            p(Episode, r"([Eex]([0-9]{2})(?:[^0-9]|$))", false, None, &[]),
            p(Flag, "EXTENDED", false, None, &[]),
            p(Group, r"(- ?(?:.+\])?([^-\[]+)(?:\[.+\])?)$", false, Some(r"(([A-Za-z0-9]+))$"), &[]),
            p(Flag, r"HDR(?:\s?10)?(?:[\s\.\]])", false, None, &[]),
            p(Flag, "HC", false, None, &[]),
            p(Flag, "([^A-Za-z0-9](3D)[^A-Za-z0-9])", false, None, &[]),
            p(Flag, "MULT[iI]-?(?:[0-9]+)?", false, None, &[]),
            p(Flag, "NF", false, None, &[]),
            p(Flag, "PROPER", false, None, &[]),
            p(
                Quality,
                r"(?:PPV\.)?[HP]DTV|(?:HD)?CAM|B[DrR]R[iI][pP][sS]?|TS|(?:PPV )?WEB[- ]?DL(?: DVDRip)?|H[dD]Rip|DVDRip|DVDRiP|DVDRIP|CamRip|W[EB]B[rR]ip|[Bb]lu[ -]?[Rr]ay|DvDScr|hdtv|UHD(?: B[Ll][Uu][- ]?[Rr][Aa][Yy])",
                false,
                None,
                &[],
            ),
            p(Text, r"R[0-9]", false, None, &[]),
            p(Flag, "REMASTERED", false, None, &[]),
            p(Flag, "REMUX", false, None, &[]),
            p(Flag, "REPACK", false, None, &[]),
            p(Resolution, r"(([0-9]{3,4}p))[^M]", false, None, &[]),
            p(Season, r"([Ss]([0-9]{1,2}))[Eex\s]", false, None, &[]),
            p(Text, r"(?:full|half)[-\s](?:sbs|ou)", true, None, &[]),
            p(Flag, r"TrueHD(?:[ \.]MA)?", false, None, &[]),
            p(Text, r"^(\[ ?([^\]]+?) ?\])", false, None, &[]),
            p(Year, r"([\[\(]?((?:19[0-9]|20[0-9])[0-9])[\]\)]?)", false, None, &[]),
            p(Text, r"1400Mb|3rd Nov| ((Rip))| \[no rar\]|[\[\(]?[Rr][Ee][Qq][\]\)]?", false, None, &[]),
        ]
    })
}

/// Strip "(…)" and "[…]" from a release group.
fn group_cleaners() -> (&'static Regex, &'static Regex) {
    static PAREN: OnceLock<Regex> = OnceLock::new();
    static BRACKET: OnceLock<Regex> = OnceLock::new();
    (PAREN.get_or_init(|| Regex::new(r" *\([^)]*\) *").unwrap()), BRACKET.get_or_init(|| Regex::new(r" *\[[^)]*\] *").unwrap()))
}

/// Parses the file name of `path` (directories are ignored).
pub fn parse(path: &str) -> ParsedName {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    parse_name(file)
}

pub fn parse_name(raw: &str) -> ParsedName {
    let name = html_escape::decode_html_entities(raw).into_owned();
    let mut out = ParsedName { name: name.clone(), title: name.clone(), ..Default::default() };
    if name.is_empty() {
        return out;
    }
    let mut start = 0usize;
    let mut end = usize::MAX;
    for pat in patterns() {
        let caps = pat.regex.captures(&name).or_else(|| pat.alternate.as_ref().and_then(|a| a.captures(&name)));
        let Some(caps) = caps else { continue };
        let whole = caps.get(0).unwrap();
        let mut clean = if caps.len() > 2 { caps.get(2).map_or("", |m| m.as_str()) } else { whole.as_str() }.to_string();
        for (from, to) in pat.replacements {
            clean = clean.replace(from, to);
        }
        match pat.field {
            Field::Year => out.year = clean.parse().ok(),
            Field::Season => out.season = clean.parse().ok(),
            Field::Episode => out.episode = clean.parse().ok(),
            Field::Resolution => out.resolution = Some(clean),
            Field::Quality => out.quality = Some(clean),
            Field::Codec => out.codec = Some(clean),
            Field::Group => {
                let (paren, bracket) = group_cleaners();
                let c = paren.replace_all(&clean, "");
                let c = bracket.replace_all(&c, "");
                out.group = Some(c.into_owned());
            }
            Field::Flag | Field::Text | Field::Number => {}
        }
        if whole.start() == 0 {
            start = whole.len();
        } else if whole.start() < end {
            end = whole.start();
        }
    }
    let end = end.min(name.len());
    let start = if start == name.len() { 0 } else { start };
    if start < end && name.is_char_boundary(start) && name.is_char_boundary(end) {
        let raw = name[start..end].split('(').next().unwrap_or("");
        let mut clean = raw.strip_prefix(" -").unwrap_or(raw).to_string();
        if !clean.contains(' ') && clean.contains('.') {
            clean = clean.replace('.', " ");
        }
        clean = clean.replace(['_', '.'], " ");
        static TAIL: OnceLock<Regex> = OnceLock::new();
        clean = TAIL.get_or_init(|| Regex::new(r"([\(_]|- ?)$").unwrap()).replace(&clean, "").trim().to_string();
        if !clean.is_empty() {
            out.title = clean;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_and_title() {
        let p = parse_name("The.Show.S02E05.1080p.WEB-DL.x264-GRP.mkv");
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episode, Some(5));
        assert_eq!(p.title, "The Show");
        assert_eq!(p.resolution.as_deref(), Some("1080p"));
    }

    #[test]
    fn movie_year() {
        let p = parse_name("The Odyssey 2026 1080p BluRay HEVC x265 5.1 BONE.mkv");
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.title, "The Odyssey");
    }
}
