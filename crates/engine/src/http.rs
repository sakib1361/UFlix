//! The shared HTTP client and a cached JSON GET.

use crate::db::db;
use serde::{Serialize, de::DeserializeOwned};
use std::sync::OnceLock;
use std::time::Duration;

pub fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("UFlix/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client")
    })
}

/// GETs JSON, caching it for `days` under the URL; serves the cache unless `forced`.
pub async fn get_json_cached<T: DeserializeOwned + Serialize>(url: &str, forced: bool, days: i64) -> Option<T> {
    if !forced && let Some(v) = db().cache_get::<T>(url) {
        return Some(v);
    }
    let value: T = client().get(url).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
    db().cache_put(url, &value, days);
    Some(value)
}

pub async fn get_text(url: &str) -> anyhow::Result<String> {
    Ok(client().get(url).send().await?.error_for_status()?.text().await?)
}

pub async fn get_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    Ok(client().get(url).send().await?.error_for_status()?.bytes().await?.to_vec())
}
