//! "Artikel exportieren" dialog: shows the Gutenberg block HTML that's about
//! to be sent, then publishes/updates the post via `wpclient`.
//!
//! `wpclient` is blocking (see its module docs for why), so the actual HTTP
//! work runs on a spawned `std::thread`, not the GTK thread. GTK widgets are
//! `!Send`, so the result comes back over a plain `std::sync::mpsc` channel
//! (`Send`-safe because it only ever carries owned strings/structs) that the
//! main thread polls with `glib::timeout_add_local` - the widget-touching
//! code all runs there, never inside the background thread.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::document::{Frontmatter, PostStatus};
use crate::{media, mediapanel, preview, secrets, wpclient, wpsite};

pub fn open(
    parent: &adw::ApplicationWindow,
    body: String,
    frontmatter: Rc<RefCell<Frontmatter>>,
    doc_dir: Option<PathBuf>,
    preview_pane: Rc<preview::PreviewPane>,
) {
    let site = wpsite::load();
    let current_fm = frontmatter.borrow().clone();

    let preview_label = gtk4::Label::builder().label("Zu sendendes Gutenberg-HTML:").xalign(0.0).build();

    let preview_buffer = gtk4::TextBuffer::new(None::<&gtk4::TextTagTable>);
    preview_buffer.set_text(&gutenberg::markdown_to_gutenberg(&body));
    let preview_view = gtk4::TextView::builder()
        .buffer(&preview_buffer)
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    let preview_scroller = gtk4::ScrolledWindow::builder()
        .child(&preview_view)
        .vexpand(true)
        .min_content_height(240)
        .build();

    // Same spacing/margins as `mediapanel::build_content`'s own outer box,
    // so switching between "Vorschau" and "Medien" doesn't visibly shift
    // the content's inset within the dialog.
    let preview_page = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .vexpand(true)
        .build();
    preview_page.append(&preview_label);
    preview_page.append(&preview_scroller);

    // Embedding Medienverwaltung here (not just linking to the separate
    // Ctrl+Shift+M dialog) lets alt text/captions/uploads be checked and
    // fixed right before publishing, in the same dialog - `build_content`
    // does its own `media::reconcile`, so this tab is always in sync with
    // the current body even if Medienverwaltung was never opened before.
    let media_page = mediapanel::build_content(frontmatter.clone(), &body, doc_dir.clone(), preview_pane.clone());

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled_with_icon(&preview_page, Some("preview"), "Vorschau", "view-reveal-symbolic");
    view_stack.add_titled_with_icon(&media_page, Some("media"), "Medien", "image-x-generic-symbolic");
    view_stack.set_vexpand(true);

    let view_switcher = adw::InlineViewSwitcher::builder().stack(&view_stack).build();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&view_switcher));
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let status_label = gtk4::Label::new(None);
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);

    let publish_button = gtk4::Button::with_label(if current_fm.wp_post_id.is_some() {
        "Aktualisieren"
    } else {
        "Veröffentlichen"
    });
    publish_button.add_css_class("suggested-action");
    publish_button.set_halign(gtk4::Align::End);

    let draft_button = gtk4::Button::with_label("Als Entwurf hochladen");
    draft_button.set_halign(gtk4::Align::End);

    let delete_button = gtk4::Button::with_label("Von WordPress löschen");
    delete_button.add_css_class("destructive-action");
    delete_button.set_halign(gtk4::Align::End);
    delete_button.set_visible(current_fm.wp_post_id.is_some());

    let button_row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(6).halign(gtk4::Align::End).build();
    button_row.append(&delete_button);
    button_row.append(&draft_button);
    button_row.append(&publish_button);

    if site.url.is_empty() {
        status_label.set_label("Keine WordPress-Verbindung eingerichtet - bitte zuerst über den Verbindungs-Dialog konfigurieren.");
        publish_button.set_sensitive(false);
        draft_button.set_sensitive(false);
        delete_button.set_sensitive(false);
    } else if current_fm.title.is_empty() {
        status_label.set_label("Bitte zuerst einen Titel in den Artikel-Eigenschaften setzen.");
        publish_button.set_sensitive(false);
        draft_button.set_sensitive(false);
    }

    content_box.append(&view_stack);
    content_box.append(&status_label);
    content_box.append(&button_row);
    toolbar_view.set_content(Some(&content_box));

    let dialog = adw::Dialog::builder()
        .title("Artikel exportieren")
        .content_width(680)
        .content_height(640)
        .child(&toolbar_view)
        .build();

    {
        let frontmatter = frontmatter.clone();
        let status_label = status_label.clone();
        let publish_button = publish_button.clone();
        let delete_button_for_click = delete_button.clone();
        let dialog_for_confirm = dialog.clone();
        delete_button.connect_clicked(move |_| {
            let Some(post_id) = frontmatter.borrow().wp_post_id else { return };
            let confirm = adw::AlertDialog::new(
                Some("Artikel wirklich löschen?"),
                Some("Der Artikel wird unwiderruflich von der WordPress-Seite gelöscht."),
            );
            confirm.add_response("cancel", "Abbrechen");
            confirm.add_response("delete", "Löschen");
            confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");

            let frontmatter = frontmatter.clone();
            let status_label = status_label.clone();
            let publish_button = publish_button.clone();
            let delete_button = delete_button_for_click.clone();
            confirm.connect_response(None, move |_, response| {
                if response != "delete" {
                    return;
                }
                status_label.set_label("Wird gelöscht …");
                delete_button.set_sensitive(false);

                let site = wpsite::load();
                let (tx, rx) = mpsc::channel::<Result<(), String>>();
                std::thread::spawn(move || {
                    let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                        .map_err(|err| err.to_string())
                        .and_then(|maybe_password| {
                            maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
                        })
                        .and_then(|password| {
                            wpclient::Client::new(&site.url, &site.username, &password)
                                .delete_post(post_id)
                                .map_err(|err| err.to_string())
                        });
                    let _ = tx.send(outcome);
                });

                let frontmatter = frontmatter.clone();
                let status_label = status_label.clone();
                let publish_button = publish_button.clone();
                let delete_button = delete_button.clone();
                glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                    Ok(Ok(())) => {
                        frontmatter.borrow_mut().wp_post_id = None;
                        status_label.set_label("Artikel wurde von WordPress gelöscht.");
                        publish_button.set_label("Veröffentlichen");
                        delete_button.set_visible(false);
                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        status_label.set_label(&format!("Fehler beim Löschen: {err}"));
                        delete_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status_label.set_label("Interner Fehler: Lösch-Thread hat kein Ergebnis geliefert.");
                        delete_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
            });
            confirm.present(Some(&dialog_for_confirm));
        });
    }

    wire_publish_button(&publish_button, &draft_button, PostStatus::Publish, &frontmatter, &body, &doc_dir, &status_label);
    wire_publish_button(&draft_button, &publish_button, PostStatus::Draft, &frontmatter, &body, &doc_dir, &status_label);

    dialog.present(Some(parent));
}

/// Wires one of the two publish-flow buttons ("Veröffentlichen" /
/// "Als Entwurf hochladen") - `target_status` is sent regardless of
/// whatever `Frontmatter.status` happens to currently hold (e.g. from the
/// separate "Artikel-Eigenschaften" dialog), so clicking either button is
/// an unambiguous, deterministic choice rather than depending on a status
/// set somewhere else first. `other_button` is disabled alongside `button`
/// while a request is in flight, so both can't race the same post/media at
/// once; on success `Frontmatter.status` is updated to match, so
/// "Artikel-Eigenschaften" reflects what was actually just sent.
fn wire_publish_button(
    button: &gtk4::Button,
    other_button: &gtk4::Button,
    target_status: PostStatus,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    body: &str,
    doc_dir: &Option<PathBuf>,
    status_label: &gtk4::Label,
) {
    let other_button = other_button.clone();
    let frontmatter = frontmatter.clone();
    let body = body.to_string();
    let doc_dir = doc_dir.clone();
    let status_label = status_label.clone();

    let button_for_click = button.clone();
    button.connect_clicked(move |_| {
        button_for_click.set_sensitive(false);
        other_button.set_sensitive(false);
        status_label.set_label("Wird gesendet …");

        let site = wpsite::load();
        let mut current_fm = frontmatter.borrow().clone();
        current_fm.status = target_status;
        let body = body.clone();
        let doc_dir = doc_dir.clone();

        let (tx, rx) = mpsc::channel::<Result<(wpclient::PostResult, Vec<media::MediaItem>), String>>();
        std::thread::spawn(move || {
            let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                .map_err(|err| err.to_string())
                .and_then(|maybe_password| {
                    maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
                })
                .and_then(|password| run_export(&site, &password, &mut current_fm, &body, doc_dir.as_deref()))
                .map(|post| (post, current_fm.media));
            let _ = tx.send(outcome);
        });

        let frontmatter = frontmatter.clone();
        let status_label = status_label.clone();
        let button = button_for_click.clone();
        let other_button = other_button.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok((post, media))) => {
                {
                    let mut fm = frontmatter.borrow_mut();
                    fm.wp_post_id = Some(post.id);
                    fm.media = media;
                    fm.status = target_status;
                }
                status_label.set_label(&format!("Erfolgreich gesendet: {}", post.link));
                button.set_sensitive(true);
                other_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                status_label.set_label(&format!("Fehler: {err}"));
                button.set_sensitive(true);
                other_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                status_label.set_label("Interner Fehler: Export-Thread hat kein Ergebnis geliefert.");
                button.set_sensitive(true);
                other_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
        });
    });
}

fn run_export(
    site: &wpsite::SiteConfig,
    password: &str,
    frontmatter: &mut Frontmatter,
    body: &str,
    doc_dir: Option<&Path>,
) -> Result<wpclient::PostResult, String> {
    let client = wpclient::Client::new(&site.url, &site.username, password);

    frontmatter.media = media::reconcile(&frontmatter.media, body);
    let uploaded_urls = media::sync_uploads(&client, &mut frontmatter.media, doc_dir)?;

    let mut blocks = gutenberg::parse_markdown(body);
    rewrite_image_urls(&mut blocks, &uploaded_urls);
    let content = gutenberg::render_blocks(&blocks);

    let mut category_ids = Vec::new();
    for name in &frontmatter.categories {
        category_ids.push(client.resolve_or_create_term("categories", name).map_err(|err| err.to_string())?);
    }
    let mut tag_ids = Vec::new();
    for name in &frontmatter.tags {
        tag_ids.push(client.resolve_or_create_term("tags", name).map_err(|err| err.to_string())?);
    }

    let mut payload = serde_json::json!({
        "title": frontmatter.title,
        "content": content,
        "status": frontmatter.status.as_str(),
        "categories": category_ids,
        "tags": tag_ids,
    });
    if !frontmatter.slug.is_empty() {
        payload["slug"] = serde_json::Value::String(frontmatter.slug.clone());
    }
    if let Some(path) = &frontmatter.featured_image {
        // A newly set local image always takes priority: upload it and use
        // the resulting media id. Unlike body images, this always re-
        // uploads on every export rather than going through
        // `media::sync_uploads`'s hash check - the featured image isn't a
        // `MediaItem` at all (it's a single Frontmatter field, never
        // scanned from the Markdown body), so it has no tracked content
        // hash to compare against. Out of scope for now since the request
        // this was built for was specifically about body images with
        // alt-text/caption; worth unifying later if duplicate featured-
        // image uploads become a real nuisance.
        let media = upload_image_file(&client, path, doc_dir).map_err(|err| err.to_string())?;
        payload["featured_media"] = serde_json::json!(media.id);
    } else if let Some(id) = frontmatter.featured_media_id {
        // Nothing new was set, but the document carries an existing
        // featured image from importing this post (see `importer.rs`) -
        // keep it rather than silently clearing it on re-export.
        payload["featured_media"] = serde_json::json!(id);
    }

    let result = match frontmatter.wp_post_id {
        Some(id) => client.update_post(id, &payload),
        None => client.create_post(&payload),
    };
    result.map_err(|err| err.to_string())
}

/// Recursively substitutes `wp:image` blocks' source with the WordPress URL
/// `media::sync_uploads` resolved for it, wherever the block's current url
/// is a key in `urls` - an already-remote url (not tracked by
/// `sync_uploads` at all) simply has no matching key and is left as-is.
fn rewrite_image_urls(blocks: &mut [gutenberg::Block], urls: &std::collections::HashMap<String, String>) {
    for block in blocks.iter_mut() {
        match block {
            gutenberg::Block::Image { url, .. } => {
                if let Some(new_url) = urls.get(url) {
                    *url = new_url.clone();
                }
            }
            gutenberg::Block::BlockQuote { blocks } => rewrite_image_urls(blocks, urls),
            gutenberg::Block::List { items, .. } => {
                for item in items.iter_mut() {
                    rewrite_image_urls(item, urls);
                }
            }
            _ => {}
        }
    }
}

/// Resolves a `MediaItem.source`/Markdown image path against the
/// document's own directory - shared by every place that needs the actual
/// file behind a local reference (`upload_image_file` here, and
/// `media::sync_uploads`/`hash_local_file`).
pub(crate) fn resolve_local_path(source: &str, base_dir: Option<&Path>) -> PathBuf {
    let path = Path::new(source);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.map(|dir| dir.join(path)).unwrap_or_else(|| path.to_path_buf())
    }
}

/// Reads the actual bytes behind a `MediaItem.source`/Markdown image
/// destination - a local path resolved against the document's own
/// directory (see `resolve_local_path`), or fetched over plain HTTP for an
/// already-remote `http(s)://` source (e.g. an image in an article opened
/// via "Von WordPress öffnen", which was never local to begin with). Used
/// by `aialt.rs`, which needs the real image bytes to send to a vision-
/// capable LLM - unlike `sync_uploads`, which only ever needs to *upload*
/// a changed local file and can skip a remote source entirely.
pub(crate) fn read_image_bytes(source: &str, base_dir: Option<&Path>) -> Result<Vec<u8>, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent.get(source).call().map_err(|err| format!("Bild nicht abrufbar: {err}"))?;
        response.body_mut().read_to_vec().map_err(|err| format!("Bild nicht lesbar: {err}"))
    } else {
        let resolved = resolve_local_path(source, base_dir);
        std::fs::read(&resolved).map_err(|err| format!("Bild {} nicht lesbar: {err}", resolved.display()))
    }
}

pub(crate) fn upload_image_file(client: &wpclient::Client, path_str: &str, base_dir: Option<&Path>) -> wpclient::Result<wpclient::MediaResult> {
    let resolved = resolve_local_path(path_str, base_dir);
    let bytes = std::fs::read(&resolved).map_err(|err| wpclient::ApiError {
        status: 0,
        message: format!("Bild {} nicht lesbar: {err}", resolved.display()),
    })?;
    let filename = resolved
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    client.upload_media(&bytes, &filename, mime_from_extension(&filename))
}

pub(crate) fn mime_from_extension(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the "Als Entwurf hochladen" vs "Veröffentlichen" choice
    /// directly: `run_export` must send whatever `frontmatter.status` holds
    /// at the time of the call (the two export-dialog buttons each force
    /// this to a specific value before calling it - see
    /// `wire_publish_button`), and a later call with a different status on
    /// the same `wp_post_id` must update it in place, not create a second
    /// post.
    #[test]
    #[ignore]
    fn run_export_respects_the_requested_post_status() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = wpclient::Client::new(&site.url, &site.username, &password);

        let body = "Ein Testartikel für Entwurf/Veröffentlichen.\n";
        let mut frontmatter = Frontmatter {
            title: "Blocksmith draft/publish status test".to_string(),
            slug: String::new(),
            status: crate::document::PostStatus::Draft,
            categories: Vec::new(),
            tags: Vec::new(),
            featured_image: None,
            wp_post_id: None,
            featured_media_id: None,
            media: Vec::new(),
        };

        let created = run_export(&site, &password, &mut frontmatter, body, None).expect("draft export failed");
        assert_eq!(client.get_post(created.id).expect("get_post failed").status, "draft");

        frontmatter.wp_post_id = Some(created.id);
        frontmatter.status = crate::document::PostStatus::Publish;
        let updated = run_export(&site, &password, &mut frontmatter, body, None).expect("publish export failed");
        assert_eq!(updated.id, created.id, "updating status must reuse the same post, not create a new one");
        assert_eq!(client.get_post(updated.id).expect("get_post failed").status, "publish");

        client.delete_post(created.id).expect("cleanup delete_post failed");
    }

    /// Exercises `run_export`'s own composition (local image path
    /// resolution + upload, category/tag name resolution, frontmatter ->
    /// REST payload mapping) against the real, already-configured
    /// WordPress site - not just `wpclient`'s lower-level calls, which
    /// `wpclient::tests` already covers. Ignored by default; run explicitly
    /// with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn run_export_with_local_image_and_terms_against_real_site() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");

        let doc_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let body = "# Blocksmith export test\n\nSome **text** with a local image below.\n\n![a red pixel](pixel.png)\n";

        let mut frontmatter = Frontmatter {
            title: "Blocksmith export test post".to_string(),
            slug: String::new(),
            status: crate::document::PostStatus::Draft,
            categories: vec!["Blocksmith Export Test".to_string()],
            tags: vec!["blocksmith-test".to_string()],
            featured_image: None,
            wp_post_id: None,
            featured_media_id: None,
            media: Vec::new(),
        };

        let created = run_export(&site, &password, &mut frontmatter, body, Some(&doc_dir)).expect("run_export failed");
        assert!(created.id > 0);

        // `run_export` reconciles + uploads media as a side effect - confirm
        // it actually tracked and uploaded the one local image, not just
        // that the post itself was created.
        assert_eq!(frontmatter.media.len(), 1);
        let uploaded = frontmatter.media[0].wordpress.clone().expect("expected the local image to have been uploaded");
        assert!(uploaded.media_id > 0);
        assert!(!uploaded.content_hash.is_empty());

        // Cleanup: the post, the uploaded media item, and the category/tag
        // terms `run_export` created.
        let client = wpclient::Client::new(&site.url, &site.username, &password);
        client.delete_post(created.id).expect("cleanup delete_post failed");
        client.delete_media(uploaded.media_id).expect("cleanup delete_media failed");
    }

    /// Exercises the duplicate-upload fix directly: exporting the same
    /// unchanged local image twice must reuse the same WordPress media id
    /// both times, not create a second attachment.
    #[test]
    #[ignore]
    fn run_export_does_not_reupload_an_unchanged_local_image() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");

        let doc_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let body = "# Blocksmith re-export test\n\n![a red pixel](pixel.png)\n";

        let mut frontmatter = Frontmatter {
            title: "Blocksmith re-export test post".to_string(),
            slug: String::new(),
            status: crate::document::PostStatus::Draft,
            categories: Vec::new(),
            tags: Vec::new(),
            featured_image: None,
            wp_post_id: None,
            featured_media_id: None,
            media: Vec::new(),
        };

        let first = run_export(&site, &password, &mut frontmatter, body, Some(&doc_dir)).expect("first run_export failed");
        let first_media_id = frontmatter.media[0].wordpress.clone().expect("expected an upload on the first export").media_id;

        // Re-export the identical body/frontmatter (as an update, since
        // `wp_post_id` now carries over) - the image content hasn't
        // changed, so this must NOT create a second media attachment.
        frontmatter.wp_post_id = Some(first.id);
        let _second = run_export(&site, &password, &mut frontmatter, body, Some(&doc_dir)).expect("second run_export failed");
        let second_media_id = frontmatter.media[0].wordpress.clone().expect("expected the ref to survive re-export").media_id;

        assert_eq!(first_media_id, second_media_id, "re-exporting an unchanged local image must reuse the same WordPress media id");

        let client = wpclient::Client::new(&site.url, &site.username, &password);
        client.delete_post(first.id).expect("cleanup delete_post failed");
        client.delete_media(first_media_id).expect("cleanup delete_media failed");
    }

    #[test]
    fn read_image_bytes_reads_a_local_file_relative_to_the_doc_dir() {
        let dir = std::env::temp_dir().join(format!("blocksmith-read-image-bytes-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("photo.png"), b"not a real png, just test bytes").unwrap();

        let bytes = read_image_bytes("photo.png", Some(&dir)).expect("expected the local file to be readable");
        assert_eq!(bytes, b"not a real png, just test bytes");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_image_bytes_reports_a_readable_error_for_a_missing_local_file() {
        let dir = std::env::temp_dir().join(format!("blocksmith-read-image-bytes-missing-test-{}", std::process::id()));
        let err = read_image_bytes("nope.png", Some(&dir)).expect_err("expected a missing file to be an error");
        assert!(err.contains("nope.png"), "{err}");
    }
}
