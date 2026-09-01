//! On-disk + in-memory cache of WordPress categories/tags, so the
//! properties dialog's autocomplete (`autocomplete.rs`) has something to
//! show immediately - at app startup, before any network round trip
//! completes - rather than starting empty every time the dialog opens.
//! Refreshable automatically at startup and on demand (a button in the
//! properties dialog calls `spawn_refresh` again).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk4::glib;
use serde_json::Value;

use crate::{secrets, wpclient, wpsite};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TermCache {
    pub categories: Vec<String>,
    pub tags: Vec<String>,
}

fn cache_path() -> PathBuf {
    let mut dir = glib::user_cache_dir();
    dir.push("blocksmith");
    dir.push("terms.json");
    dir
}

pub fn load() -> TermCache {
    match std::fs::read_to_string(cache_path()) {
        Ok(contents) => parse(&contents),
        Err(_) => TermCache::default(),
    }
}

fn save(cache: &TermCache) -> std::io::Result<()> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serialize(cache))
}

fn parse(s: &str) -> TermCache {
    let Ok(value) = serde_json::from_str::<Value>(s) else {
        return TermCache::default();
    };
    let string_list = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    TermCache {
        categories: string_list("categories"),
        tags: string_list("tags"),
    }
}

fn serialize(cache: &TermCache) -> String {
    serde_json::json!({ "categories": cache.categories, "tags": cache.tags }).to_string()
}

/// Refreshes the cache from the configured WordPress site on a background
/// thread (see `wpclient`'s module docs for why it's blocking), updating
/// the shared in-memory lists and the on-disk cache once done. A no-op if
/// no site is configured; leaves the existing cache untouched on failure.
pub fn spawn_refresh(categories: Rc<RefCell<Vec<String>>>, tags: Rc<RefCell<Vec<String>>>) {
    let site = wpsite::load();
    if site.url.is_empty() {
        return;
    }

    let (tx, rx) = mpsc::channel::<Option<TermCache>>();
    std::thread::spawn(move || {
        let result = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .ok()
            .flatten()
            .map(|password| wpclient::Client::new(&site.url, &site.username, &password))
            .and_then(|client| {
                let categories = client.list_term_names("categories").ok()?;
                let tags = client.list_term_names("tags").ok()?;
                Some(TermCache { categories, tags })
            });
        let _ = tx.send(result);
    });

    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Some(cache)) => {
            *categories.borrow_mut() = cache.categories.clone();
            *tags.borrow_mut() = cache.tags.clone();
            let _ = save(&cache);
            glib::ControlFlow::Break
        }
        Ok(None) => glib::ControlFlow::Break,
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_parse_and_serialize() {
        let cache = TermCache {
            categories: vec!["GNU/Linux".to_string(), "Android".to_string()],
            tags: vec!["arch".to_string()],
        };
        assert_eq!(parse(&serialize(&cache)), cache);
    }

    #[test]
    fn missing_or_corrupt_file_yields_default() {
        assert_eq!(parse(""), TermCache::default());
        assert_eq!(parse("not json"), TermCache::default());
    }
}
