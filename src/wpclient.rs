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

/// Only used to verify uploads in integration tests (`get_media`/
/// `delete_media` below) - the app itself only ever needs a
/// `media::WordPressMediaRef` (id + URL), read back from the locally saved
/// document, not a live re-fetch from the server.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct MediaDetail {
    pub id: u64,
    pub source_url: String,
    pub alt_text: String,
    pub caption: String,
}

#[derive(Debug, Clone)]
pub struct PostSummary {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub date: String,
    /// The post's public permalink - used e.g. by `linkpicker.rs` to insert
    /// a real, clickable link to it, not just its id/title.
    pub link: String,
}

#[derive(Debug, Clone)]
pub struct PostDetail {
    pub id: u64,
    pub title: String,
    /// Raw Gutenberg block-comment HTML (`content.raw`, which needs
    /// `context=edit` to get - `content.rendered` has been run through
    /// WordPress's display filters, which strip the `<!-- wp:... -->`
    /// comments a block editor needs to reconstruct the blocks).
    pub content: String,
    pub status: String,
    pub slug: String,
    pub categories: Vec<u64>,
    pub tags: Vec<u64>,
    /// `0` means no featured image is set.
    pub featured_media: u64,
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

    fn get_json(&self, url: &str) -> Result<Value> {
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
        serde_json::from_str(&body_text).map_err(|err| unreadable_response(status, err))
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

    /// Sets alt text and/or caption on an already-uploaded media item -
    /// `upload_media`'s `POST /media` only accepts the raw file bytes, so
    /// this is always a separate follow-up call, same endpoint (WordPress
    /// treats a `POST` to an existing attachment id as an update).
    pub fn update_media_metadata(&self, media_id: u64, alt_text: Option<&str>, caption: Option<&str>) -> Result<()> {
        let mut payload = serde_json::json!({});
        if let Some(alt) = alt_text {
            payload["alt_text"] = serde_json::json!(alt);
        }
        if let Some(caption) = caption {
            payload["caption"] = serde_json::json!(caption);
        }
        let mut response = self
            .agent
            .post(self.endpoint(&format!("media/{media_id}")))
            .header("Authorization", self.auth_header.as_str())
            .send_json(&payload)
            .map_err(network_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body_text = response.body_mut().read_to_string().unwrap_or_default();
            return Err(error_from_body(status, &body_text));
        }
        Ok(())
    }

    /// Fetches a media item's current alt text/caption back from the server
    /// (`context=edit` for the raw, untranslated `caption.raw` rather than
    /// the display-filtered `caption.rendered`) - used to verify an upload's
    /// metadata actually landed, not just that the `POST` returned 2xx.
    #[cfg(test)]
    pub fn get_media(&self, media_id: u64) -> Result<MediaDetail> {
        let url = format!("{}?context=edit", self.endpoint(&format!("media/{media_id}")));
        let value = self.get_json(&url)?;
        let id = value.get("id").and_then(Value::as_u64).unwrap_or(media_id);
        let source_url = value.get("source_url").and_then(Value::as_str).unwrap_or_default().to_string();
        let alt_text = value.get("alt_text").and_then(Value::as_str).unwrap_or_default().to_string();
        let caption = value
            .get("caption")
            .and_then(|c| c.get("raw"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Ok(MediaDetail { id, source_url, alt_text, caption })
    }

    /// Permanently deletes a media item (bypassing trash, which media
    /// attachments don't support anyway). Used both for integration-test
    /// cleanup and by `media::sync_uploads` to remove the now-superseded
    /// attachment after a changed local image is re-uploaded as a new one -
    /// WordPress's REST API has no way to replace an existing attachment's
    /// file in place.
    pub fn delete_media(&self, media_id: u64) -> Result<()> {
        let url = format!("{}?force=true", self.endpoint(&format!("media/{media_id}")));
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
        let value = self.get_json(&url)?;
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

    /// Resolves a taxonomy term id back to its name (the REST API gives
    /// posts' categories/tags as ids; opening an existing post for editing
    /// needs their names for the frontmatter).
    pub fn get_term_name(&self, taxonomy: &str, id: u64) -> Result<String> {
        let url = format!("{}?_fields=name", self.endpoint(&format!("{taxonomy}/{id}")));
        let value = self.get_json(&url)?;
        Ok(value.get("name").and_then(Value::as_str).unwrap_or_default().to_string())
    }

    /// Lists the most recent posts (any status the authenticated user can
    /// see), for the "Von WordPress öffnen" picker. Doesn't paginate beyond
    /// the first 50 - fine for finding a recent article to edit.
    pub fn list_posts(&self) -> Result<Vec<PostSummary>> {
        let url = format!(
            "{}?per_page=50&orderby=date&order=desc&context=edit&status=publish,future,draft,pending,private&_fields=id,title,status,date,link",
            self.endpoint("posts")
        );
        let value = self.get_json(&url)?;
        Ok(value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(PostSummary {
                            id: item.get("id")?.as_u64()?,
                            title: post_title(item),
                            status: item.get("status").and_then(Value::as_str).unwrap_or_default().to_string(),
                            date: item.get("date").and_then(Value::as_str).unwrap_or_default().to_string(),
                            link: item.get("link").and_then(Value::as_str).unwrap_or_default().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Fetches a post's full content (as raw Gutenberg block-comment HTML,
    /// via `context=edit` - see [`PostDetail::content`]) plus the metadata
    /// needed to populate the properties dialog after converting it back to
    /// Markdown.
    pub fn get_post(&self, id: u64) -> Result<PostDetail> {
        let url = format!(
            "{}?context=edit&_fields=id,title,content,status,slug,categories,tags,featured_media",
            self.endpoint(&format!("posts/{id}"))
        );
        let value = self.get_json(&url)?;
        let u64_array = |key: &str| -> Vec<u64> {
            value.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
        };
        Ok(PostDetail {
            id,
            title: post_title(&value),
            content: value.get("content").and_then(|c| c.get("raw")).and_then(Value::as_str).unwrap_or_default().to_string(),
            status: value.get("status").and_then(Value::as_str).unwrap_or("draft").to_string(),
            slug: value.get("slug").and_then(Value::as_str).unwrap_or_default().to_string(),
            categories: u64_array("categories"),
            tags: u64_array("tags"),
            featured_media: value.get("featured_media").and_then(Value::as_u64).unwrap_or(0),
        })
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

/// A post's `title` is `{"raw": "...", "rendered": "..."}` in `context=edit`
/// responses (and just `{"rendered": "..."}` otherwise) - prefer `raw`
/// since `rendered` may have HTML entities substituted in.
fn post_title(post: &Value) -> String {
    post.get("title")
        .and_then(|t| t.get("raw").or_else(|| t.get("rendered")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

    /// A 1x1 transparent PNG - real image bytes, not a text stand-in, so
    /// this exercises the same upload path a real screenshot would take.
    const ONE_PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
        0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60,
        0x82,
    ];

    /// Exercises the standalone per-image media workflow from
    /// `mediapanel.rs`: upload -> transmit alt text/caption -> read them
    /// back from the server (not just check the `POST` returned 2xx) ->
    /// clean up. Covers "WordPress upload", "alt text transmitted to
    /// WordPress", "caption transmitted to WordPress", and "WordPress media
    /// id saved" from the media-management spec.
    #[test]
    #[ignore]
    fn media_upload_with_alt_text_and_caption_round_trips_against_real_site() {
        let config = wpsite::load();
        assert!(!config.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&config.url, &config.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = Client::new(&config.url, &config.username, &password);

        let media = client.upload_media(ONE_PIXEL_PNG, "blocksmith-media-test.png", "image/png").expect("media upload failed");
        assert!(media.id > 0);
        assert!(!media.source_url.is_empty());

        client
            .update_media_metadata(media.id, Some("Ein einzelnes Testpixel"), Some("Testunterschrift"))
            .expect("update_media_metadata failed");

        let detail = client.get_media(media.id).expect("get_media failed");
        assert_eq!(detail.id, media.id);
        assert_eq!(detail.source_url, media.source_url);
        assert_eq!(detail.alt_text, "Ein einzelnes Testpixel");
        assert_eq!(detail.caption, "Testunterschrift");

        client.delete_media(media.id).expect("cleanup delete_media failed");
    }

    /// A deliberately empty alt text (decorative image) must reach WordPress
    /// as an empty string, not be skipped or rejected as an error - the same
    /// distinction `media::AltText::Empty` exists to preserve locally.
    #[test]
    #[ignore]
    fn media_deliberately_empty_alt_text_is_sent_as_an_empty_string() {
        let config = wpsite::load();
        assert!(!config.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&config.url, &config.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = Client::new(&config.url, &config.username, &password);

        let media = client.upload_media(ONE_PIXEL_PNG, "blocksmith-decorative-test.png", "image/png").expect("media upload failed");

        client.update_media_metadata(media.id, Some(""), None).expect("update_media_metadata with empty alt failed");

        let detail = client.get_media(media.id).expect("get_media failed");
        assert_eq!(detail.alt_text, "");

        client.delete_media(media.id).expect("cleanup delete_media failed");
    }

    /// A failed metadata update against a nonexistent attachment id must
    /// surface as a plain `Err`, never a panic - `mediapanel.rs` relies on
    /// exactly this to keep a failed upload from touching (let alone
    /// losing) the locally held article.
    #[test]
    #[ignore]
    fn update_media_metadata_on_an_unknown_id_fails_cleanly() {
        let config = wpsite::load();
        assert!(!config.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&config.url, &config.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = Client::new(&config.url, &config.username, &password);

        let result = client.update_media_metadata(u64::MAX, Some("does not matter"), None);
        assert!(result.is_err(), "expected updating a nonexistent media id to fail, not silently succeed");
    }
}
