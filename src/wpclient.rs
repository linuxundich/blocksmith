//! Blocking WordPress REST API client (`/wp-json/wp/v2/...`).
//!
//! Deliberately blocking (`ureq`, not `reqwest`): `reqwest` needs a `tokio`
//! runtime to drive its I/O, but `oo7` (see `secrets.rs`) already commits
//! this app to `async-std`'s reactor so it composes with GLib's main loop
//! without pulling in a second async runtime. REST calls here are rare,
//! user-initiated (publish/update button), and fast enough that running
//! them on a spawned thread - not the UI thread - is simpler than reconciling
//! two executors. Callers are expected to invoke these from a background
//! thread and hand the result back to the GTK thread themselves.

use std::time::Duration;

use base64::Engine;
use serde_json::Value;

pub struct Client {
    agent: ureq::Agent,
    base_url: String,
    auth_header: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

impl std::error::Error for ApiError {}

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone)]
pub struct PostResult {
    pub id: u64,
    pub link: String,
}

#[derive(Debug, Clone)]
pub struct MediaResult {
    pub id: u64,
    pub source_url: String,
}

fn network_error(err: ureq::Error) -> ApiError {
    ApiError {
        status: 0,
        message: err.to_string(),
    }
}

fn error_from_body(status: u16, body_text: &str) -> ApiError {
    let message = serde_json::from_str::<Value>(body_text)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_else(|| body_text.to_string());
    ApiError { status, message }
}

fn unreadable_response(status: u16, err: impl std::fmt::Display) -> ApiError {
    ApiError {
        status,
        message: format!("Antwort nicht lesbar: {err}"),
    }
}

impl Client {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        let credentials = format!("{username}:{password}");
        let auth_header = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes())
        );
        Self {
            agent: ureq::Agent::new_with_config(config),
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header,
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/wp-json/wp/v2/{path}", self.base_url)
    }

    /// Uploads a local file to the media library, returning its id and the
    /// URL WordPress will actually serve it from (used to rewrite `wp:image`
    /// blocks whose source was a local path before export).
    pub fn upload_media(&self, bytes: &[u8], filename: &str, mime_type: &str) -> Result<MediaResult> {
        let mut response = self
            .agent
            .post(self.endpoint("media"))
            .header("Authorization", self.auth_header.as_str())
            .header("Content-Type", mime_type)
            .header("Content-Disposition", format!("attachment; filename=\"{filename}\"").as_str())
            .send(bytes)
            .map_err(network_error)?;

        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text));
        }
        let value: Value = serde_json::from_str(&body_text).map_err(|err| unreadable_response(status, err))?;
        let id = value
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ApiError { status, message: "Keine Medien-ID in der Antwort".into() })?;
        let source_url = value.get("source_url").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(MediaResult { id, source_url })
    }

    pub fn create_post(&self, payload: &Value) -> Result<PostResult> {
        self.send_post_payload(self.endpoint("posts"), payload)
    }

    pub fn update_post(&self, post_id: u64, payload: &Value) -> Result<PostResult> {
        self.send_post_payload(self.endpoint(&format!("posts/{post_id}")), payload)
    }

    /// Permanently deletes a post (bypassing trash). Mainly useful for
    /// cleaning up after integration tests against a real site.
    pub fn delete_post(&self, post_id: u64) -> Result<()> {
        let url = format!("{}?force=true", self.endpoint(&format!("posts/{post_id}")));
        let mut response = self
            .agent
            .delete(url)
            .header("Authorization", self.auth_header.as_str())
            .call()
            .map_err(network_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body_text = response.body_mut().read_to_string().unwrap_or_default();
            return Err(error_from_body(status, &body_text));
        }
        Ok(())
    }

    /// Lists up to 100 existing term names for a taxonomy (`"categories"` or
    /// `"tags"`), for autocomplete suggestions. Doesn't paginate beyond that
    /// - fine for a personal blog's category/tag list.
    pub fn list_term_names(&self, taxonomy: &str) -> Result<Vec<String>> {
        let url = format!("{}?per_page=100&_fields=name", self.endpoint(taxonomy));
        let mut response = self
            .agent
            .get(url)
            .header("Authorization", self.auth_header.as_str())
            .call()
            .map_err(network_error)?;
        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text));
        }
        let value: Value = serde_json::from_str(&body_text).map_err(|err| unreadable_response(status, err))?;
        Ok(value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn send_post_payload(&self, url: String, payload: &Value) -> Result<PostResult> {
        let mut response = self
            .agent
            .post(url)
            .header("Authorization", self.auth_header.as_str())
            .send_json(payload)
            .map_err(network_error)?;

        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text));
        }
        let value: Value = serde_json::from_str(&body_text).map_err(|err| unreadable_response(status, err))?;
        let id = value
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ApiError { status, message: "Keine Post-ID in der Antwort".into() })?;
        let link = value.get("link").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(PostResult { id, link })
    }

    /// Resolves a category/tag name to its term id, creating the term if no
    /// exact match exists yet. WordPress's REST API wants term ids, not
    /// names, in a post's `categories`/`tags` arrays.
    pub fn resolve_or_create_term(&self, taxonomy: &str, name: &str) -> Result<u64> {
        let search_url = format!("{}?search={}", self.endpoint(taxonomy), percent_encode(name));
        let mut response = self
            .agent
            .get(&search_url)
            .header("Authorization", self.auth_header.as_str())
            .call()
            .map_err(network_error)?;
        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if (200..300).contains(&status) {
            if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&body_text) {
                let existing = items
                    .iter()
                    .find(|item| item.get("name").and_then(Value::as_str) == Some(name))
                    .and_then(|item| item.get("id").and_then(Value::as_u64));
                if let Some(id) = existing {
                    return Ok(id);
                }
            }
        }

        let mut response = self
            .agent
            .post(self.endpoint(taxonomy))
            .header("Authorization", self.auth_header.as_str())
            .send_json(serde_json::json!({ "name": name }))
            .map_err(network_error)?;
        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text));
        }
        let value: Value = serde_json::from_str(&body_text).map_err(|err| unreadable_response(status, err))?;
        value
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ApiError { status, message: format!("Kein Term-ID für \"{name}\" erhalten") })
    }
}

/// Minimal percent-encoding for a query parameter value - not a general URL
/// encoder, just enough for category/tag names in a `?search=` query.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{secrets, wpsite};

    /// Backs the categories/tags autocomplete in `properties.rs` - checks
    /// it actually gets real term names back, not just a 200 with an empty
    /// or malformed body.
    #[test]
    #[ignore]
    fn list_term_names_against_real_site() {
        let config = wpsite::load();
        assert!(!config.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&config.url, &config.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = Client::new(&config.url, &config.username, &password);

        let categories = client.list_term_names("categories").expect("list_term_names(categories) failed");
        assert!(!categories.is_empty(), "expected at least one existing category on the real site");
        assert!(categories.iter().any(|c| c == "Allgemein"), "expected the real site's known 'Allgemein' category, got {categories:?}");
    }

    /// Exercises create -> resolve/create term -> media upload -> update ->
    /// delete against the real, already-configured WordPress site (see
    /// `wpsite::load()`/`secrets::load_app_password`) rather than a mock.
    /// Ignored by default since it needs live credentials and a reachable
    /// site; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn full_round_trip_against_real_site() {
        let config = wpsite::load();
        assert!(!config.url.is_empty(), "no WordPress site configured (run the connection dialog first)");

        let password = futures_lite::future::block_on(secrets::load_app_password(&config.url, &config.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");

        let client = Client::new(&config.url, &config.username, &password);

        let category_id = client
            .resolve_or_create_term("categories", "Blocksmith Test")
            .expect("category resolve/create failed");

        let media = client
            .upload_media(b"not a real png, just bytes for the upload test", "blocksmith-test.txt", "text/plain")
            .expect("media upload failed");
        assert!(media.id > 0);

        let created = client
            .create_post(&serde_json::json!({
                "title": "Blocksmith integration test post",
                "content": "<!-- wp:paragraph -->\n<p>Created by an automated test, safe to delete.</p>\n<!-- /wp:paragraph -->",
                "status": "draft",
                "categories": [category_id],
            }))
            .expect("create_post failed");
        assert!(created.id > 0);

        let updated = client
            .update_post(
                created.id,
                &serde_json::json!({
                    "title": "Blocksmith integration test post (updated)",
                }),
            )
            .expect("update_post failed");
        assert_eq!(updated.id, created.id);

        client.delete_post(created.id).expect("cleanup delete_post failed");
    }
}
