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
use crate::i18n::tr;
use crate::{linkcheck, media, mediapanel, notify, preview, secrets, wpclient, wpsite};

pub fn open(
    parent: &adw::ApplicationWindow,
    body: String,
    frontmatter: Rc<RefCell<Frontmatter>>,
    doc_dir: Option<PathBuf>,
    preview_pane: Rc<preview::PreviewPane>,
) {
    let site = wpsite::load();
    let current_fm = frontmatter.borrow().clone();

    let preview_label = gtk4::Label::builder().label(tr("Zu sendendes Gutenberg-HTML:")).xalign(0.0).build();

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

    // Same one-shot-scan-at-open-time approach as the media tab above: a
    // pre-publish sanity pass, not a live watcher.
    let links_page = linkcheck::build_content(&body);

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled_with_icon(&preview_page, Some("preview"), &tr("Vorschau"), "view-reveal-symbolic");
    view_stack.add_titled_with_icon(&media_page, Some("media"), &tr("Medien"), "image-x-generic-symbolic");
    view_stack.add_titled_with_icon(&links_page, Some("links"), &tr("Links"), "insert-link-symbolic");
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

    // Shows the published post's real permalink as a clickable link once a
    // publish/draft/schedule succeeds - kept as its own widget rather than
    // embedding an `<a href>` in `status_label` via Pango markup, since
    // that label's other messages are plain, unescaped, server-provided
    // error text that could otherwise break markup parsing.
    let link_button = gtk4::LinkButton::builder().visible(false).halign(gtk4::Align::Start).build();

    let publish_button = gtk4::Button::with_label(&if current_fm.wp_post_id.is_some() {
        tr("Aktualisieren")
    } else {
        tr("Veröffentlichen")
    });
    publish_button.add_css_class("suggested-action");
    publish_button.set_halign(gtk4::Align::End);

    let draft_button = gtk4::Button::with_label(&tr("Als Entwurf hochladen"));
    draft_button.set_halign(gtk4::Align::End);

    // Only shown when "Geplant" is actually selected in den Artikel-
    // Eigenschaften - the third status the export dialog's two buttons
    // above can't express, since each forces its own fixed status
    // (`wire_publish_button`'s `target_status`) - visible so scheduling is
    // reachable at all, but only when it means something.
    let schedule_button = gtk4::Button::with_label(&tr("Terminieren"));
    schedule_button.set_halign(gtk4::Align::End);
    schedule_button.set_visible(current_fm.status == PostStatus::Future);

    let delete_button = gtk4::Button::with_label(&tr("Von WordPress löschen"));
    delete_button.add_css_class("destructive-action");
    delete_button.set_halign(gtk4::Align::End);
    delete_button.set_visible(current_fm.wp_post_id.is_some());

    let button_row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(6).halign(gtk4::Align::End).build();
    button_row.append(&delete_button);
    button_row.append(&draft_button);
    button_row.append(&schedule_button);
    button_row.append(&publish_button);

    if site.url.is_empty() {
        status_label.set_label(&tr("Keine WordPress-Verbindung eingerichtet - bitte zuerst über den Verbindungs-Dialog konfigurieren."));
        publish_button.set_sensitive(false);
        draft_button.set_sensitive(false);
        schedule_button.set_sensitive(false);
        delete_button.set_sensitive(false);
    } else if current_fm.title.is_empty() {
        status_label.set_label(&tr("Bitte zuerst einen Titel in den Artikel-Eigenschaften setzen."));
        publish_button.set_sensitive(false);
        draft_button.set_sensitive(false);
        schedule_button.set_sensitive(false);
    }

    content_box.append(&view_stack);
    content_box.append(&status_label);
    content_box.append(&link_button);
    content_box.append(&button_row);
    toolbar_view.set_content(Some(&content_box));

    let dialog = adw::Dialog::builder()
        .title(tr("Artikel exportieren"))
        .content_width(680)
        .content_height(640)
        .child(&toolbar_view)
        .build();

    {
        let frontmatter = frontmatter.clone();
        let status_label = status_label.clone();
        let link_button = link_button.clone();
        let publish_button = publish_button.clone();
        let delete_button_for_click = delete_button.clone();
        let dialog_for_confirm = dialog.clone();
        delete_button.connect_clicked(move |_| {
            let Some(post_id) = frontmatter.borrow().wp_post_id else { return };
            let confirm = adw::AlertDialog::new(
                Some(&tr("Artikel wirklich löschen?")),
                Some(&tr("Der Artikel wird unwiderruflich von der WordPress-Seite gelöscht.")),
            );
            confirm.add_response("cancel", &tr("Abbrechen"));
            confirm.add_response("delete", &tr("Löschen"));
            confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");

            let frontmatter = frontmatter.clone();
            let status_label = status_label.clone();
            let link_button = link_button.clone();
            let publish_button = publish_button.clone();
            let delete_button = delete_button_for_click.clone();
            confirm.connect_response(None, move |_, response| {
                if response != "delete" {
                    return;
                }
                status_label.set_label(&tr("Wird gelöscht …"));
                link_button.set_visible(false);
                delete_button.set_sensitive(false);

                let site = wpsite::load();
                let (tx, rx) = mpsc::channel::<Result<(), String>>();
                std::thread::spawn(move || {
                    let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                        .map_err(|err| err.to_string())
                        .and_then(|maybe_password| {
                            maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden."))
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
                        status_label.set_label(&tr("Artikel wurde von WordPress gelöscht."));
                        publish_button.set_label(&tr("Veröffentlichen"));
                        delete_button.set_visible(false);
                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        status_label.set_label(&tr("Fehler beim Löschen: {err}").replace("{err}", &err));
                        delete_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status_label.set_label(&tr("Interner Fehler: Lösch-Thread hat kein Ergebnis geliefert."));
                        delete_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
            });
            confirm.present(Some(&dialog_for_confirm));
        });
    }

    let status = StatusWidgets { label: status_label.clone(), link: link_button.clone() };
    wire_publish_button(&publish_button, &[&draft_button, &schedule_button], PostStatus::Publish, &frontmatter, &body, &doc_dir, &status);
    wire_publish_button(&draft_button, &[&publish_button, &schedule_button], PostStatus::Draft, &frontmatter, &body, &doc_dir, &status);
    wire_publish_button(&schedule_button, &[&publish_button, &draft_button], PostStatus::Future, &frontmatter, &body, &doc_dir, &status);

    dialog.present(Some(parent));
}

/// The status line + its accompanying permalink `LinkButton` - bundled
/// purely to keep `wire_publish_button`'s parameter count down
/// (clippy::too_many_arguments), the same fix already used for
/// `DocContext`/`RecentFilesWidgets` in `window.rs`. Always shown/updated
/// together: a status message about what just happened, and (only once a
/// publish actually succeeds) a link to see it.
#[derive(Clone)]
struct StatusWidgets {
    label: gtk4::Label,
    link: gtk4::LinkButton,
}

/// Wires one of the three publish-flow buttons ("Veröffentlichen" /
/// "Als Entwurf hochladen" / "Terminieren") - `target_status` is sent
/// regardless of whatever `Frontmatter.status` happens to currently hold
/// (e.g. from the separate "Artikel-Eigenschaften" dialog), so clicking any
/// one button is an unambiguous, deterministic choice rather than depending
/// on a status set somewhere else first. `other_buttons` are disabled
/// alongside `button` while a request is in flight, so none of them can
/// race the same post/media at once; on success `Frontmatter.status` is
/// updated to match, so "Artikel-Eigenschaften" reflects what was actually
/// just sent.
fn wire_publish_button(
    button: &gtk4::Button,
    other_buttons: &[&gtk4::Button],
    target_status: PostStatus,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    body: &str,
    doc_dir: &Option<PathBuf>,
    status: &StatusWidgets,
) {
    let other_buttons: Vec<gtk4::Button> = other_buttons.iter().map(|b| (*b).clone()).collect();
    let frontmatter = frontmatter.clone();
    let body = body.to_string();
    let doc_dir = doc_dir.clone();
    let status_label = status.label.clone();
    let link_button = status.link.clone();

    let button_for_click = button.clone();
    button.connect_clicked(move |_| {
        button_for_click.set_sensitive(false);
        for b in &other_buttons {
            b.set_sensitive(false);
        }
        status_label.set_label(&tr("Wird gesendet …"));
        link_button.set_visible(false);

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
                    maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden."))
                })
                .and_then(|password| run_export(&site, &password, &mut current_fm, &body, doc_dir.as_deref()))
                .map(|post| (post, current_fm.media));
            let _ = tx.send(outcome);
        });

        let frontmatter = frontmatter.clone();
        let status_label = status_label.clone();
        let link_button = link_button.clone();
        let button = button_for_click.clone();
        let other_buttons = other_buttons.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok((post, media))) => {
                let title = {
                    let mut fm = frontmatter.borrow_mut();
                    fm.wp_post_id = Some(post.id);
                    fm.media = media;
                    fm.status = target_status;
                    fm.title.clone()
                };
                status_label.set_label(&tr("Erfolgreich gesendet:"));
                link_button.set_uri(&post.link);
                link_button.set_label(&post.link);
                link_button.set_visible(true);
                notify::send("export", &tr("Veröffentlicht"), &tr("„{title}“ wurde erfolgreich gesendet.").replace("{title}", &title));
                button.set_sensitive(true);
                for b in &other_buttons {
                    b.set_sensitive(true);
                }
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                status_label.set_label(&tr("Fehler: {err}").replace("{err}", &err));
                notify::send("export", &tr("Veröffentlichen fehlgeschlagen"), &err);
                button.set_sensitive(true);
                for b in &other_buttons {
                    b.set_sensitive(true);
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                status_label.set_label(&tr("Interner Fehler: Export-Thread hat kein Ergebnis geliefert."));
                button.set_sensitive(true);
                for b in &other_buttons {
                    b.set_sensitive(true);
                }
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
    // WordPress only actually schedules a post if its `date` is genuinely
    // in the future - given a missing or past date, it silently publishes
    // immediately instead of scheduling (see `PostStatus::Future`'s doc
    // comment), so this is checked up front rather than letting that
    // surprise happen after an otherwise-successful export.
    if frontmatter.status == PostStatus::Future && frontmatter.scheduled_at.is_none() {
        return Err(tr("Für den Status „Geplant“ muss ein gültiger Veröffentlichungstermin gesetzt sein."));
    }

    let client = wpclient::Client::new(&site.url, &site.username, password);

    frontmatter.media = media::reconcile(&frontmatter.media, body);
    let uploaded_urls = media::sync_uploads(&client, &mut frontmatter.media, doc_dir)?;

    let mut blocks = gutenberg::parse_markdown(body);
    apply_media_metadata(&mut blocks, &frontmatter.media);
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
    if let Some(excerpt) = &frontmatter.excerpt {
        payload["excerpt"] = serde_json::Value::String(excerpt.clone());
    }
    // RankMath registers these meta keys with `show_in_rest`, so they're
    // writable the same way as any other post meta; sent only when set, so
    // an unset field never overwrites a value already set directly in
    // RankMath's own editor. Harmless against a site without RankMath -
    // WordPress's REST API silently drops an unrecognized meta key rather
    // than erroring.
    let mut meta = serde_json::Map::new();
    if let Some(title) = &frontmatter.rank_math_title {
        meta.insert("rank_math_title".to_string(), serde_json::Value::String(title.clone()));
    }
    if let Some(description) = &frontmatter.rank_math_description {
        meta.insert("rank_math_description".to_string(), serde_json::Value::String(description.clone()));
    }
    if let Some(keyword) = &frontmatter.rank_math_focus_keyword {
        meta.insert("rank_math_focus_keyword".to_string(), serde_json::Value::String(keyword.clone()));
    }
    if !meta.is_empty() {
        payload["meta"] = serde_json::Value::Object(meta);
    }
    if frontmatter.status == PostStatus::Future {
        if let Some(scheduled_at) = &frontmatter.scheduled_at {
            payload["date"] = serde_json::Value::String(scheduled_at.clone());
        }
    }
    if let Some(path) = &frontmatter.featured_image {
        // Normally already uploaded via Medienverwaltung's own "Aufmacherbild"
        // row (`mediapanel::build_featured_image_row`) before publishing gets
        // this far, which clears `featured_image` in favor of
        // `featured_media_id` below. This is the fallback for a
        // `featured_image` set but never manually uploaded: upload it now and
        // use the resulting media id. Unlike body images, this always
        // re-uploads rather than going through `media::sync_uploads`'s hash
        // check - the featured image isn't a `MediaItem` at all (it's a
        // single Frontmatter field, never scanned from the Markdown body),
        // so it has no tracked content hash to compare against.
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

/// Overlays each image block's alt text/caption with the corresponding
/// `MediaItem`'s (matched by `source`, i.e. the block's still-original,
/// pre-`rewrite_image_urls` url - so this must run before that) - so an
/// edit made in Medienverwaltung actually reaches the published post's
/// HTML. Without this, `blocks` only ever carries whatever alt/title text
/// happens to be written literally in the Markdown source, since
/// Medienverwaltung's alt-text/caption editors (`mediapanel.rs`) only ever
/// update `Frontmatter.media`, never the body itself; the only place that
/// data otherwise reaches WordPress is `media::sync_uploads`'s
/// `update_media_metadata` call, which sets the *attachment's* alt
/// text/caption in the media library, not the `<img>`/`<figcaption>` baked
/// into this post's own content - and WordPress never re-reads an
/// attachment's current metadata into an already-published block.
/// `AltText::Undefined` (nothing decided yet) deliberately leaves the
/// parsed alt alone rather than blanking it.
fn apply_media_metadata(blocks: &mut [gutenberg::Block], media: &[media::MediaItem]) {
    for block in blocks.iter_mut() {
        match block {
            gutenberg::Block::Image { url, alt, title } => {
                if let Some(item) = media.iter().find(|item| &item.source == url) {
                    if let Some(text) = item.alt.as_wordpress_value() {
                        *alt = text.to_string();
                    }
                    *title = item.caption.clone();
                }
            }
            gutenberg::Block::BlockQuote { blocks } => apply_media_metadata(blocks, media),
            gutenberg::Block::List { items, .. } => {
                for item in items.iter_mut() {
                    apply_media_metadata(item, media);
                }
            }
            gutenberg::Block::Columns { columns } => {
                for column in columns.iter_mut() {
                    apply_media_metadata(column, media);
                }
            }
            _ => {}
        }
    }
}

/// Recursively substitutes `wp:image`/`wp:video`/`wp:audio` blocks' source
/// with the WordPress URL `media::sync_uploads` resolved for it, wherever
/// the block's current url is a key in `urls` - an already-remote url (not
/// tracked by `sync_uploads` at all, e.g. an embed) simply has no matching
/// key and is left as-is.
fn rewrite_image_urls(blocks: &mut [gutenberg::Block], urls: &std::collections::HashMap<String, String>) {
    for block in blocks.iter_mut() {
        match block {
            gutenberg::Block::Image { url, .. } | gutenberg::Block::Video { url } | gutenberg::Block::Audio { url } => {
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
            gutenberg::Block::Columns { columns } => {
                for column in columns.iter_mut() {
                    rewrite_image_urls(column, urls);
                }
            }
            gutenberg::Block::Gallery { images } => {
                for image in images.iter_mut() {
                    if let Some(new_url) = urls.get(&image.url) {
                        image.url = new_url.clone();
                    }
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
        let mut response = agent.get(source).call().map_err(|err| tr("Bild nicht abrufbar: {err}").replace("{err}", &err.to_string()))?;
        response
            .body_mut()
            .read_to_vec()
            .map_err(|err| tr("Bild nicht lesbar: {err}").replace("{err}", &err.to_string()))
    } else {
        let resolved = resolve_local_path(source, base_dir);
        std::fs::read(&resolved).map_err(|err| tr("Bild {path} nicht lesbar: {err}").replace("{path}", &resolved.display().to_string()).replace("{err}", &err.to_string()))
    }
}

pub(crate) fn upload_image_file(client: &wpclient::Client, path_str: &str, base_dir: Option<&Path>) -> wpclient::Result<wpclient::MediaResult> {
    let resolved = resolve_local_path(path_str, base_dir);
    let bytes = std::fs::read(&resolved).map_err(|err| wpclient::ApiError {
        status: 0,
        message: tr("Bild {path} nicht lesbar: {err}").replace("{path}", &resolved.display().to_string()).replace("{err}", &err.to_string()),
    })?;
    let filename = resolved
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let compressed = crate::imagecompress::maybe_compress(&bytes, &filename);
    client.upload_media(&compressed.bytes, &compressed.filename, compressed.mime_type)
}

pub(crate) fn mime_from_extension(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_image_urls_recurses_into_columns_and_gallery_blocks() {
        let urls: std::collections::HashMap<String, String> =
            [("local-a.png".to_string(), "https://example.com/a.png".to_string()), ("local-b.png".to_string(), "https://example.com/b.png".to_string())]
                .into_iter()
                .collect();
        let mut blocks = vec![
            gutenberg::Block::Columns {
                columns: vec![vec![gutenberg::Block::Image { url: "local-a.png".to_string(), alt: String::new(), title: None }]],
            },
            gutenberg::Block::Gallery {
                images: vec![gutenberg::GalleryImage { url: "local-b.png".to_string(), alt: String::new() }],
            },
        ];
        rewrite_image_urls(&mut blocks, &urls);
        let gutenberg::Block::Columns { columns } = &blocks[0] else { panic!("expected Columns") };
        let gutenberg::Block::Image { url, .. } = &columns[0][0] else { panic!("expected Image") };
        assert_eq!(url, "https://example.com/a.png");
        let gutenberg::Block::Gallery { images } = &blocks[1] else { panic!("expected Gallery") };
        assert_eq!(images[0].url, "https://example.com/b.png");
    }

    #[test]
    fn apply_media_metadata_overlays_alt_text_and_caption_from_the_matching_media_item() {
        let media = vec![media::MediaItem {
            id: "media-001".to_string(),
            filename: "cat.png".to_string(),
            source: "cat.png".to_string(),
            alt: media::AltText::Text("a red cat".to_string()),
            caption: Some("Our cat, sleeping".to_string()),
            wordpress: None,
        }];
        let mut blocks = vec![gutenberg::Block::Image {
            url: "cat.png".to_string(),
            alt: String::new(),
            title: None,
        }];
        apply_media_metadata(&mut blocks, &media);
        let gutenberg::Block::Image { alt, title, .. } = &blocks[0] else { panic!("expected Image") };
        assert_eq!(alt, "a red cat");
        assert_eq!(title.as_deref(), Some("Our cat, sleeping"));
    }

    #[test]
    fn apply_media_metadata_leaves_alt_untouched_while_undefined() {
        let media = vec![media::MediaItem {
            id: "media-001".to_string(),
            filename: "cat.png".to_string(),
            source: "cat.png".to_string(),
            alt: media::AltText::Undefined,
            caption: None,
            wordpress: None,
        }];
        let mut blocks = vec![gutenberg::Block::Image {
            url: "cat.png".to_string(),
            alt: "from the markdown source".to_string(),
            title: None,
        }];
        apply_media_metadata(&mut blocks, &media);
        let gutenberg::Block::Image { alt, .. } = &blocks[0] else { panic!("expected Image") };
        assert_eq!(alt, "from the markdown source");
    }

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
            scheduled_at: None,
            categories: Vec::new(),
            tags: Vec::new(),
            excerpt: None,
            rank_math_title: None,
            rank_math_description: None,
            rank_math_focus_keyword: None,
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
            scheduled_at: None,
            categories: vec!["Blocksmith Export Test".to_string()],
            tags: vec!["blocksmith-test".to_string()],
            excerpt: None,
            rank_math_title: None,
            rank_math_description: None,
            rank_math_focus_keyword: None,
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
            scheduled_at: None,
            categories: Vec::new(),
            tags: Vec::new(),
            excerpt: None,
            rank_math_title: None,
            rank_math_description: None,
            rank_math_focus_keyword: None,
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
