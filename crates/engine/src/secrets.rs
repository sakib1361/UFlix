//! API keys from the git-ignored secrets.env, compiled in by build.rs. An environment variable of the same name wins.

use std::collections::HashMap;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!(concat!(env!("OUT_DIR"), "/secrets.env"));

fn values() -> &'static HashMap<String, String> {
    static V: OnceLock<HashMap<String, String>> = OnceLock::new();
    V.get_or_init(|| {
        EMBEDDED
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| {
                let v = v.trim();
                let unquoted = v
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')));
                (k.trim().to_string(), unquoted.unwrap_or(v).to_string())
            })
            .collect()
    })
}

pub fn get(name: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => values().get(name).cloned().unwrap_or_default(),
    }
}

pub fn tmdb_api_key() -> String {
    get("UFLIX_TMDB_API_KEY")
}
pub fn opensubtitles_api_key() -> String {
    get("UFLIX_OPENSUBTITLES_API_KEY")
}
pub fn opensubtitles_username() -> String {
    get("UFLIX_OPENSUBTITLES_USERNAME")
}
pub fn opensubtitles_password() -> String {
    get("UFLIX_OPENSUBTITLES_PASSWORD")
}
pub fn subdl_api_key() -> String {
    get("UFLIX_SUBDL_API_KEY")
}
