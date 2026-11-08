//! The local store (SQLite, `<data>/LocalData.sqlite`): a cache table with expiry for network results and a permanent
//! table for settings and history. Values are JSON.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::{Mutex, OnceLock};

pub struct Db {
    conn: Mutex<Connection>,
}

pub fn db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let folder = crate::paths::data_folder();
        let _ = std::fs::create_dir_all(&folder);
        let conn = Connection::open(folder.join("LocalData.sqlite")).or_else(|_| Connection::open_in_memory()).expect("sqlite");
        let _ = conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS cache (id TEXT PRIMARY KEY, data TEXT NOT NULL, expires INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS permanent (id TEXT PRIMARY KEY, data TEXT NOT NULL);",
        );
        let _ = conn.execute("DELETE FROM cache WHERE expires < ?1", params![now()]);
        Db { conn: Mutex::new(conn) }
    })
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Db {
    /// A cached value, even if it has expired since start-up (expired rows are purged at start).
    pub fn cache_get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.cache_get_with_expiry(key).map(|(v, _)| v)
    }

    /// A cached value only while it is fresh.
    pub fn cache_get_fresh<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.cache_get_with_expiry(key).filter(|(_, exp)| *exp > now()).map(|(v, _)| v)
    }

    fn cache_get_with_expiry<T: DeserializeOwned>(&self, key: &str) -> Option<(T, i64)> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, i64)> = conn
            .query_row("SELECT data, expires FROM cache WHERE id = ?1", params![key], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()?;
        let (data, expires) = row?;
        match serde_json::from_str(&data) {
            Ok(v) => Some((v, expires)),
            Err(_) => {
                let _ = conn.execute("DELETE FROM cache WHERE id = ?1", params![key]);
                None
            }
        }
    }

    pub fn cache_put<T: Serialize + ?Sized>(&self, key: &str, value: &T, days: i64) {
        let Ok(data) = serde_json::to_string(value) else { return };
        let _ = self.conn.lock().unwrap().execute(
            "INSERT INTO cache (id, data, expires) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET data = ?2, expires = ?3",
            params![key, data, now() + days * 86_400],
        );
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let conn = self.conn.lock().unwrap();
        let data: String = conn.query_row("SELECT data FROM permanent WHERE id = ?1", params![key], |r| r.get(0)).optional().ok()??;
        serde_json::from_str(&data).ok()
    }

    pub fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) {
        let Ok(data) = serde_json::to_string(value) else { return };
        let _ = self
            .conn
            .lock()
            .unwrap()
            .execute("INSERT INTO permanent (id, data) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET data = ?2", params![key, data]);
    }
}
