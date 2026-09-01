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

use crate::document::Frontmatter;
use crate::{secrets, wpclient, wpsite};

pub fn open(
    parent: &adw::ApplicationWindow,
    body: String,
    frontmatter: Rc<RefCell<Frontmatter>>,
    doc_dir: Option<PathBuf>,
) {
    let site = wpsite::load();
    let current_fm = frontmatter.borrow().clone();

    let header = adw::HeaderBar::new();
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

    let delete_button = gtk4::Button::with_label("Von WordPress löschen");
    delete_button.add_css_class("destructive-action");
    delete_button.set_halign(gtk4::Align::End);
    delete_button.set_visible(current_fm.wp_post_id.is_some());

    let button_row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(6).halign(gtk4::Align::End).build();
    button_row.append(&delete_button);
    button_row.append(&publish_button);

    if site.url.is_empty() {
        status_label.set_label("Keine WordPress-Verbindung eingerichtet - bitte zuerst über den Verbindungs-Dialog konfigurieren.");
        publish_button.set_sensitive(false);
        delete_button.set_sensitive(false);
    } else if current_fm.title.is_empty() {
        status_label.set_label("Bitte zuerst einen Titel in den Artikel-Eigenschaften setzen.");
        publish_button.set_sensitive(false);
    }

    content_box.append(&preview_label);
    content_box.append(&preview_scroller);
    content_box.append(&status_label);
    content_box.append(&button_row);
    toolbar_view.set_content(Some(&content_box));

    let dialog = adw::Dialog::builder()
        .title("Artikel exportieren")
        .content_width(640)
        .content_height(560)
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

    let publish_button_for_click = publish_button.clone();
    publish_button.connect_clicked(move |_| {
        publish_button_for_click.set_sensitive(false);
        status_label.set_label("Wird gesendet …");

        let site = wpsite::load();
        let current_fm = frontmatter.borrow().clone();
        let body = body.clone();
        let doc_dir = doc_dir.clone();

        let (tx, rx) = mpsc::channel::<Result<wpclient::PostResult, String>>();
        std::thread::spawn(move || {
            let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                .map_err(|err| err.to_string())
                .and_then(|maybe_password| {
                    maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
                })
                .and_then(|password| run_export(&site, &password, &current_fm, &body, doc_dir.as_deref()));
            let _ = tx.send(outcome);
        });

        let frontmatter = frontmatter.clone();
        let status_label = status_label.clone();
        let publish_button = publish_button_for_click.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok(post)) => {
                frontmatter.borrow_mut().wp_post_id = Some(post.id);
                status_label.set_label(&format!("Erfolgreich gesendet: {}", post.link));
                publish_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                status_label.set_label(&format!("Fehler: {err}"));
                publish_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                status_label.set_label("Interner Fehler: Export-Thread hat kein Ergebnis geliefert.");
                publish_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
        });
    });

    dialog.present(Some(parent));
}

fn run_export(
    site: &wpsite::SiteConfig,
    password: &str,
    frontmatter: &Frontmatter,
    body: &str,
    doc_dir: Option<&Path>,
) -> Result<wpclient::PostResult, String> {
    let client = wpclient::Client::new(&site.url, &site.username, password);

    let mut blocks = gutenberg::parse_markdown(body);
    upload_local_images(&client, &mut blocks, doc_dir).map_err(|err| err.to_string())?;
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
        // the resulting media id.
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

/// Recursively rewrites `wp:image` blocks whose source is a local file path
/// (not `http(s)://`) to the URL WordPress serves after uploading it, since
/// a Gutenberg image block needs a URL the reader's browser can reach.
fn upload_local_images(client: &wpclient::Client, blocks: &mut [gutenberg::Block], base_dir: Option<&Path>) -> wpclient::Result<()> {
    for block in blocks.iter_mut() {
        match block {
            gutenberg::Block::Image { url, .. } => {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    let media = upload_image_file(client, url, base_dir)?;
                    *url = media.source_url;
                }
            }
            gutenberg::Block::BlockQuote { blocks } => upload_local_images(client, blocks, base_dir)?,
            gutenberg::Block::List { items, .. } => {
                for item in items.iter_mut() {
                    upload_local_images(client, item, base_dir)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn upload_image_file(client: &wpclient::Client, path_str: &str, base_dir: Option<&Path>) -> wpclient::Result<wpclient::MediaResult> {
    let path = Path::new(path_str);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.map(|dir| dir.join(path)).unwrap_or_else(|| path.to_path_buf())
    };
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

fn mime_from_extension(filename: &str) -> &'static str {
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

        let frontmatter = Frontmatter {
            title: "Blocksmith export test post".to_string(),
            slug: String::new(),
            status: crate::document::PostStatus::Draft,
            categories: vec!["Blocksmith Export Test".to_string()],
            tags: vec!["blocksmith-test".to_string()],
            featured_image: None,
            wp_post_id: None,
            featured_media_id: None,
        };

        let created = run_export(&site, &password, &frontmatter, body, Some(&doc_dir)).expect("run_export failed");
        assert!(created.id > 0);

        // Cleanup: the post, and the category/tag terms `run_export` created.
        let client = wpclient::Client::new(&site.url, &site.username, &password);
        client.delete_post(created.id).expect("cleanup delete_post failed");
    }
}
