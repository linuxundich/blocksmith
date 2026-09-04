//! Per-image metadata (alt text, caption, WordPress upload state) that
//! Markdown's own `![alt](url)` syntax can't fully represent on its own -
//! namely the three-way distinction between "no alt text has been set yet"
//! and "an alt text of exactly nothing, on purpose" (decorative images),
//! and a caption that's independent of alt text rather than one standing
//! in for the other.
//!
//! This is the foundation for the planned `.bsm` project format (a
//! self-contained container embedding an article's media, still to come) -
//! the JSON shape here is designed to drop into that format's manifest
//! largely unchanged, and for now is persisted as one line in the existing
//! `.md` frontmatter block (`document.rs`) so none of this is lost before
//! `.bsm` exists.

use std::collections::HashMap;
use std::path::Path;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde_json::Value;
use sha2::Digest;

/// The three states an image's alt text can be in - collapsing "empty" and
/// "not yet decided" into one falsy value (as plain Markdown does) would
/// make every freshly-inserted image look identical to one a user
/// deliberately marked decorative, which is exactly the distinction this
/// module exists to preserve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AltText {
    /// Not yet reviewed - flagged by the accessibility check.
    Undefined,
    /// Reviewed and deliberately left blank (decorative image) - not an
    /// error.
    Empty,
    Text(String),
}

impl AltText {
    pub fn is_undefined(&self) -> bool {
        matches!(self, AltText::Undefined)
    }

    /// The text to send WordPress's `alt_text` field - `None` for
    /// `Undefined`, since nothing has been decided yet to send.
    pub fn as_wordpress_value(&self) -> Option<&str> {
        match self {
            AltText::Undefined => None,
            AltText::Empty => Some(""),
            AltText::Text(s) => Some(s),
        }
    }

    fn to_json_pair(&self) -> (bool, String) {
        match self {
            AltText::Undefined => (false, String::new()),
            AltText::Empty => (true, String::new()),
            AltText::Text(s) => (true, s.clone()),
        }
    }

    fn from_json_pair(defined: bool, text: &str) -> Self {
        if !defined {
            AltText::Undefined
        } else if text.is_empty() {
            AltText::Empty
        } else {
            AltText::Text(text.to_string())
        }
    }
}

/// A media file already uploaded to WordPress's media library - kept so
/// re-uploading the same image is recognized as unnecessary and never
/// creates a duplicate attachment.
#[derive(Debug, Clone, PartialEq)]
pub struct WordPressMediaRef {
    pub media_id: u64,
    pub url: String,
    /// SHA-256 (hex) of the local file's content at the moment it was
    /// uploaded - compared against the current file's hash by
    /// `sync_uploads` to tell "unchanged since upload" from "the user
    /// edited/replaced the local image" without depending on mtimes, which
    /// a plain copy/touch can bump without any real content change.
    pub content_hash: String,
}

/// The transient state of an in-progress upload - unlike `WordPressMediaRef`,
/// this is never persisted: `Uploading` only makes sense while the app is
/// open, and a `Failed` attempt should just look like `NotUploaded` again
/// the next time the document is opened, not carry a stale error forward.
#[derive(Debug, Clone, PartialEq)]
pub enum UploadStatus {
    NotUploaded,
    Uploading,
    Uploaded(WordPressMediaRef),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaItem {
    /// Stable id, independent of the image's position in the document -
    /// `crates/gutenberg`/`.bsm` can reference media by id without caring
    /// where in the text (or which of several) an image appears.
    pub id: String,
    pub filename: String,
    /// The path or URL exactly as it appears in the Markdown source right
    /// now - how this item is matched back up with its `![]()` reference
    /// on every re-scan (see `reconcile`).
    pub source: String,
    pub alt: AltText,
    /// Independent of `alt` - the app never derives one from the other.
    pub caption: Option<String>,
    pub wordpress: Option<WordPressMediaRef>,
}

impl MediaItem {
    pub fn upload_status(&self) -> UploadStatus {
        match &self.wordpress {
            Some(reference) => UploadStatus::Uploaded(reference.clone()),
            None => UploadStatus::NotUploaded,
        }
    }
}

/// Finds every `![alt](source "title")` image reference in `markdown`, in
/// document order, as `(source, alt_text_as_written, title_as_written)`
/// triples. The optional quoted `"title"` is the closest thing plain
/// Markdown has to a caption, so it seeds `MediaItem.caption` for images
/// seen for the first time (see `reconcile`) - Markdown itself still can't
/// distinguish "alt intentionally left blank" from "alt never set", so that
/// distinction is only made in `reconcile` too.
fn scan_images(markdown: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut in_image = false;
    let mut current_source = String::new();
    let mut current_title = String::new();
    let mut current_alt = String::new();

    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                in_image = true;
                current_source = dest_url.to_string();
                current_title = title.to_string();
                current_alt.clear();
            }
            Event::Text(text) if in_image => current_alt.push_str(&text),
            Event::End(TagEnd::Image) if in_image => {
                in_image = false;
                out.push((
                    std::mem::take(&mut current_source),
                    std::mem::take(&mut current_alt),
                    std::mem::take(&mut current_title),
                ));
            }
            _ => {}
        }
    }
    out
}

/// Rebuilds the media list from the document's current text, preserving
/// every existing item's metadata (alt/caption/WordPress link) as long as
/// its image is still referenced - matched by `source`, not position, so
/// reordering images in the text doesn't lose their metadata. An image no
/// longer present is dropped; a newly-added one starts as `Undefined`
/// unless the Markdown itself already carries a non-empty alt text (e.g.
/// pasted from elsewhere), which is taken as an initial `Text` value, and
/// likewise seeds its caption from the Markdown title (`![alt](src "title")`)
/// if one is present.
pub fn reconcile(existing: &[MediaItem], markdown: &str) -> Vec<MediaItem> {
    let mut next_serial = existing
        .iter()
        .filter_map(|item| item.id.strip_prefix("media-").and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0)
        + 1;

    scan_images(markdown)
        .into_iter()
        .map(|(source, markdown_alt, markdown_title)| {
            if let Some(found) = existing.iter().find(|item| item.source == source) {
                found.clone()
            } else {
                let filename = source.rsplit(['/', '\\']).next().unwrap_or(&source).to_string();
                let alt = if markdown_alt.is_empty() { AltText::Undefined } else { AltText::Text(markdown_alt) };
                let caption = (!markdown_title.is_empty()).then_some(markdown_title);
                let id = format!("media-{next_serial:03}");
                next_serial += 1;
                MediaItem { id, filename, source, alt, caption, wordpress: None }
            }
        })
        .collect()
}

fn hash_bytes(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Resolves `source` against `doc_dir` (see `export::resolve_local_path`),
/// reads it, and hashes its content - the same hash `sync_uploads` compares
/// against, exposed separately for callers (like `mediapanel.rs`'s manual
/// upload button) that already know which single item they just uploaded
/// and only need to record its hash, not decide whether to upload at all.
pub fn hash_local_file(source: &str, doc_dir: Option<&Path>) -> std::io::Result<String> {
    let resolved = crate::export::resolve_local_path(source, doc_dir);
    let bytes = std::fs::read(resolved)?;
    Ok(hash_bytes(&bytes))
}

/// Ensures every local image in `items` is uploaded to WordPress with its
/// current filename/alt text/caption, updating each item's `wordpress` ref
/// in place, and returns a `source -> URL` map the caller uses to rewrite
/// `wp:image` blocks before publishing. An image already uploaded with a
/// content hash matching its current file is left untouched entirely - "bei
/// Bedarf hochladen", not on every single export. An already-remote source
/// (`http(s)://`, e.g. from a WordPress-imported article) is left alone
/// too, silently, since there's nothing local to upload; a LOCAL path that
/// fails to read is a real error and is surfaced, not skipped, since it
/// would otherwise publish a broken image reference with no explanation.
///
/// WordPress's REST API has no way to replace an existing attachment's file
/// in place, so a changed image is uploaded as a brand new attachment, and
/// the superseded one is then deleted so the media library doesn't
/// accumulate orphaned duplicates every time an image is edited and
/// republished. That delete is best-effort: it runs after the new upload
/// has already succeeded, so a failure to delete the old one is not itself
/// treated as an export failure.
pub fn sync_uploads(
    client: &crate::wpclient::Client,
    items: &mut [MediaItem],
    doc_dir: Option<&Path>,
) -> Result<HashMap<String, String>, String> {
    let mut urls = HashMap::new();

    for item in items.iter_mut() {
        if item.source.starts_with("http://") || item.source.starts_with("https://") {
            continue;
        }

        let resolved = crate::export::resolve_local_path(&item.source, doc_dir);
        let bytes = std::fs::read(&resolved).map_err(|err| format!("Bild {} nicht lesbar: {err}", resolved.display()))?;
        let current_hash = hash_bytes(&bytes);

        if let Some(existing) = &item.wordpress {
            if existing.content_hash == current_hash {
                urls.insert(item.source.clone(), existing.url.clone());
                continue;
            }
        }

        let previous = item.wordpress.take();
        let mime = crate::export::mime_from_extension(&item.filename);
        let media = client.upload_media(&bytes, &item.filename, mime).map_err(|err| err.to_string())?;
        client
            .update_media_metadata(media.id, item.alt.as_wordpress_value(), item.caption.as_deref())
            .map_err(|err| err.to_string())?;

        if let Some(previous) = previous {
            let _ = client.delete_media(previous.media_id);
        }

        urls.insert(item.source.clone(), media.source_url.clone());
        item.wordpress = Some(WordPressMediaRef { media_id: media.id, url: media.source_url, content_hash: current_hash });
    }

    Ok(urls)
}

pub fn to_json(items: &[MediaItem]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                let (alt_defined, alt_text) = item.alt.to_json_pair();
                let mut object = serde_json::json!({
                    "id": item.id,
                    "filename": item.filename,
                    "source": item.source,
                    "alt": alt_text,
                    "altDefined": alt_defined,
                });
                if let Some(caption) = &item.caption {
                    object["caption"] = Value::String(caption.clone());
                }
                if let Some(wp) = &item.wordpress {
                    object["wordpress"] = serde_json::json!({ "mediaId": wp.media_id, "url": wp.url, "contentHash": wp.content_hash });
                }
                object
            })
            .collect(),
    )
}

pub fn from_json(value: &Value) -> Vec<MediaItem> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.to_string();
                    let filename = item.get("filename").and_then(Value::as_str).unwrap_or_default().to_string();
                    let source = item.get("source")?.as_str()?.to_string();
                    let alt_defined = item.get("altDefined").and_then(Value::as_bool).unwrap_or(false);
                    let alt_text = item.get("alt").and_then(Value::as_str).unwrap_or_default();
                    let caption = item.get("caption").and_then(Value::as_str).map(str::to_string);
                    let wordpress = item.get("wordpress").and_then(|wp| {
                        Some(WordPressMediaRef {
                            media_id: wp.get("mediaId")?.as_u64()?,
                            url: wp.get("url").and_then(Value::as_str).unwrap_or_default().to_string(),
                            // Missing on a document saved before content-hash
                            // tracking existed - an empty hash never matches
                            // a real file's hash, so `sync_uploads` treats
                            // it as "changed" and re-uploads once on the
                            // next export, safely picking up hash tracking
                            // from then on. Not data-destructive, just one
                            // redundant upload for pre-existing images.
                            content_hash: wp.get("contentHash").and_then(Value::as_str).unwrap_or_default().to_string(),
                        })
                    });
                    Some(MediaItem {
                        id,
                        filename,
                        source,
                        alt: AltText::from_json_pair(alt_defined, alt_text),
                        caption,
                        wordpress,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn to_json_string(items: &[MediaItem]) -> String {
    to_json(items).to_string()
}

pub fn from_json_str(s: &str) -> Vec<MediaItem> {
    serde_json::from_str::<Value>(s).map(|value| from_json(&value)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_images_in_order_with_their_alt_text_and_title() {
        let markdown = "Text.\n\n![first alt](a.png \"first title\")\n\nMore.\n\n![](b.png)\n";
        let found = scan_images(markdown);
        assert_eq!(
            found,
            vec![
                ("a.png".to_string(), "first alt".to_string(), "first title".to_string()),
                ("b.png".to_string(), String::new(), String::new()),
            ]
        );
    }

    #[test]
    fn reconcile_creates_new_items_with_undefined_alt_for_blank_markdown_alt() {
        let items = reconcile(&[], "![](photo.jpg)\n");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].alt, AltText::Undefined);
        assert_eq!(items[0].filename, "photo.jpg");
        assert_eq!(items[0].caption, None);
        assert_eq!(items[0].wordpress, None);
    }

    #[test]
    fn reconcile_seeds_text_alt_from_existing_markdown_alt() {
        let items = reconcile(&[], "![a red barn](barn.jpg)\n");
        assert_eq!(items[0].alt, AltText::Text("a red barn".to_string()));
    }

    #[test]
    fn reconcile_seeds_caption_from_markdown_title() {
        let items = reconcile(&[], "![a red barn](barn.jpg \"A red barn at dusk\")\n");
        assert_eq!(items[0].caption, Some("A red barn at dusk".to_string()));
    }

    #[test]
    fn reconcile_does_not_overwrite_an_existing_item_caption_from_markdown_title() {
        let existing = vec![MediaItem {
            id: "media-001".to_string(),
            filename: "barn.jpg".to_string(),
            source: "barn.jpg".to_string(),
            alt: AltText::Text("a red barn".to_string()),
            caption: Some("User-edited caption".to_string()),
            wordpress: None,
        }];
        let updated = reconcile(&existing, "![a red barn](barn.jpg \"a different markdown title\")\n");
        assert_eq!(updated[0].caption, Some("User-edited caption".to_string()));
    }

    #[test]
    fn reconcile_preserves_metadata_for_still_referenced_images() {
        let existing = vec![MediaItem {
            id: "media-001".to_string(),
            filename: "cat.png".to_string(),
            source: "cat.png".to_string(),
            alt: AltText::Text("A cat".to_string()),
            caption: Some("My cat".to_string()),
            wordpress: Some(WordPressMediaRef {
                media_id: 42,
                url: "https://example.com/cat.png".to_string(),
                content_hash: "deadbeef".to_string(),
            }),
        }];
        let updated = reconcile(&existing, "![something else entirely](cat.png)\n");
        assert_eq!(updated, existing, "metadata for an image matched by source must survive re-scanning, even if the markdown alt text differs");
    }

    #[test]
    fn reconcile_drops_items_no_longer_referenced() {
        let existing = vec![MediaItem {
            id: "media-001".to_string(),
            filename: "gone.png".to_string(),
            source: "gone.png".to_string(),
            alt: AltText::Empty,
            caption: None,
            wordpress: None,
        }];
        let updated = reconcile(&existing, "No images here anymore.\n");
        assert!(updated.is_empty());
    }

    #[test]
    fn reconcile_assigns_increasing_ids_and_never_reuses_one() {
        let existing = vec![MediaItem {
            id: "media-005".to_string(),
            filename: "a.png".to_string(),
            source: "a.png".to_string(),
            alt: AltText::Undefined,
            caption: None,
            wordpress: None,
        }];
        let updated = reconcile(&existing, "![](a.png)\n\n![](b.png)\n");
        assert_eq!(updated[0].id, "media-005");
        assert_eq!(updated[1].id, "media-006");
    }

    #[test]
    fn json_round_trips_all_three_alt_states() {
        let items = vec![
            MediaItem { id: "media-001".into(), filename: "a.png".into(), source: "a.png".into(), alt: AltText::Undefined, caption: None, wordpress: None },
            MediaItem { id: "media-002".into(), filename: "b.png".into(), source: "b.png".into(), alt: AltText::Empty, caption: None, wordpress: None },
            MediaItem {
                id: "media-003".into(),
                filename: "c.png".into(),
                source: "c.png".into(),
                alt: AltText::Text("A description".into()),
                caption: Some("A caption".into()),
                wordpress: Some(WordPressMediaRef { media_id: 7, url: "https://example.com/c.png".into(), content_hash: "abc123".into() }),
            },
        ];
        let round_tripped = from_json_str(&to_json_string(&items));
        assert_eq!(round_tripped, items);
    }

    #[test]
    fn deliberately_empty_alt_is_distinct_from_undefined_after_a_round_trip() {
        let items = vec![
            MediaItem { id: "media-001".into(), filename: "a.png".into(), source: "a.png".into(), alt: AltText::Undefined, caption: None, wordpress: None },
            MediaItem { id: "media-002".into(), filename: "b.png".into(), source: "b.png".into(), alt: AltText::Empty, caption: None, wordpress: None },
        ];
        let round_tripped = from_json_str(&to_json_string(&items));
        assert!(round_tripped[0].alt.is_undefined());
        assert!(!round_tripped[1].alt.is_undefined());
        assert_eq!(round_tripped[1].alt.as_wordpress_value(), Some(""));
    }

    #[test]
    fn corrupt_or_missing_json_yields_an_empty_list_rather_than_panicking() {
        assert_eq!(from_json_str("not json at all"), Vec::new());
        assert_eq!(from_json_str(""), Vec::new());
    }
}
