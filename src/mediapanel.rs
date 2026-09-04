//! "Medienverwaltung" dialog: per-image alt text, caption, and WordPress
//! upload state for every image referenced in the current article - see
//! `media.rs` for the data model and why alt text needs three states
//! instead of Markdown's plain on/off.
//!
//! Uploads reuse `export.rs`'s local-image-resolution logic and run on a
//! spawned thread (`wpclient` is blocking), polled via `mpsc` +
//! `glib::timeout_add_local`, same pattern as `export.rs`'s publish/delete
//! flows - a failed or slow upload must never freeze the dialog or the rest
//! of the app, and never touches the locally-held article until it
//! succeeds.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::document::Frontmatter;
use crate::media::{self, AltText, UploadStatus};
use crate::{export, secrets, wpclient, wpsite};

pub fn open(parent: &adw::ApplicationWindow, body: String, frontmatter: Rc<RefCell<Frontmatter>>, doc_dir: Option<PathBuf>) {
    let content = build_content(frontmatter, &body, doc_dir);

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));

    let dialog = adw::Dialog::builder()
        .title("Medienverwaltung")
        .content_width(560)
        .content_height(600)
        .child(&toolbar_view)
        .build();
    dialog.present(Some(parent));
}

/// Builds the Medienverwaltung's actual content (status line + per-image
/// list), independent of the dialog chrome around it - reused both by the
/// standalone `open()` above (still reachable via Ctrl+Shift+M) and by
/// `export.rs`, which embeds this same content as a tab in the publish
/// dialog so alt text/captions/uploads can be checked right before
/// publishing, not just from a separate dialog.
pub fn build_content(frontmatter: Rc<RefCell<Frontmatter>>, body: &str, doc_dir: Option<PathBuf>) -> gtk4::Widget {
    // Re-scan so images added/removed since the last reconcile (a save, or
    // opening this panel before) are reflected immediately - metadata for
    // still-referenced images is preserved (see `media::reconcile`).
    {
        let mut fm = frontmatter.borrow_mut();
        fm.media = media::reconcile(&fm.media, body);
    }
    let item_count = frontmatter.borrow().media.len();

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
    status_label.set_label(&summary_text(&frontmatter));

    let list_box = gtk4::ListBox::new();
    list_box.add_css_class("boxed-list");
    for index in 0..item_count {
        let row = build_row(index, frontmatter.clone(), doc_dir.clone(), status_label.clone());
        list_box.append(&row);
    }

    let scroller = gtk4::ScrolledWindow::builder().child(&list_box).vexpand(true).min_content_height(360).build();

    content_box.append(&status_label);
    content_box.append(&scroller);
    content_box.upcast()
}

fn summary_text(frontmatter: &Rc<RefCell<Frontmatter>>) -> String {
    let fm = frontmatter.borrow();
    if fm.media.is_empty() {
        return "Dieser Artikel enthält aktuell keine Bilder.".to_string();
    }
    let missing_alt = fm.media.iter().filter(|item| item.alt.is_undefined()).count();
    if missing_alt > 0 {
        format!(
            "{missing_alt} von {} Bild{} {} noch keinen Alternativtext.",
            fm.media.len(),
            if fm.media.len() == 1 { "" } else { "ern" },
            if missing_alt == 1 { "hat" } else { "haben" }
        )
    } else {
        format!("{} Bild{} in diesem Artikel.", fm.media.len(), if fm.media.len() == 1 { "" } else { "er" })
    }
}

fn upload_status_text(status: &UploadStatus) -> String {
    match status {
        UploadStatus::NotUploaded => "Noch nicht zu WordPress hochgeladen".to_string(),
        UploadStatus::Uploading => "Wird hochgeladen …".to_string(),
        UploadStatus::Uploaded(reference) => format!("Bereits hochgeladen (Medien-ID {})", reference.media_id),
        UploadStatus::Failed(err) => format!("Fehler beim letzten Upload: {err}"),
    }
}

fn build_row(
    index: usize,
    frontmatter: Rc<RefCell<Frontmatter>>,
    doc_dir: Option<PathBuf>,
    status_label: gtk4::Label,
) -> adw::ExpanderRow {
    let item = frontmatter.borrow().media[index].clone();

    let expander = adw::ExpanderRow::builder().title(item.filename.clone()).use_markup(false).build();
    expander.set_subtitle(&upload_status_text(&item.upload_status()));

    let alt_switch_row = adw::SwitchRow::builder()
        .title("Alternativtext definieren")
        .subtitle("Aus lassen für rein dekorative Bilder - das ist kein Fehler")
        .active(!item.alt.is_undefined())
        .build();

    let alt_entry_row = adw::EntryRow::builder().title("Alternativtext").build();
    if let AltText::Text(text) = &item.alt {
        alt_entry_row.set_text(text);
    }
    alt_entry_row.set_visible(alt_switch_row.is_active());

    {
        let frontmatter = frontmatter.clone();
        let alt_entry_row = alt_entry_row.clone();
        let status_label = status_label.clone();
        alt_switch_row.connect_active_notify(move |row| {
            let active = row.is_active();
            alt_entry_row.set_visible(active);
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if active {
                    let text = alt_entry_row.text().to_string();
                    if text.is_empty() {
                        AltText::Empty
                    } else {
                        AltText::Text(text)
                    }
                } else {
                    AltText::Undefined
                };
            }
            status_label.set_label(&summary_text(&frontmatter));
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let alt_switch_row = alt_switch_row.clone();
        let status_label = status_label.clone();
        alt_entry_row.connect_changed(move |row| {
            if !alt_switch_row.is_active() {
                return;
            }
            let text = row.text().to_string();
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if text.is_empty() { AltText::Empty } else { AltText::Text(text) };
            }
            status_label.set_label(&summary_text(&frontmatter));
        });
    }

    let caption_row = adw::EntryRow::builder().title("Bildunterschrift").text(item.caption.as_deref().unwrap_or("")).build();
    {
        let frontmatter = frontmatter.clone();
        caption_row.connect_changed(move |row| {
            let text = row.text().to_string();
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.caption = (!text.is_empty()).then_some(text);
            }
        });
    }

    let upload_button = gtk4::Button::with_label(if item.wordpress.is_some() { "Erneut hochladen" } else { "Zu WordPress hochladen" });
    let upload_status_label = gtk4::Label::new(None);
    upload_status_label.set_xalign(0.0);
    upload_status_label.set_wrap(true);

    let upload_row_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    upload_row_box.append(&upload_button);
    upload_row_box.append(&upload_status_label);
    let upload_row = gtk4::ListBoxRow::builder().child(&upload_row_box).activatable(false).selectable(false).build();

    {
        let frontmatter = frontmatter.clone();
        let expander = expander.clone();
        let upload_button_for_click = upload_button.clone();
        let upload_status_label = upload_status_label.clone();
        upload_button.connect_clicked(move |_| {
            let (source, alt_for_upload, caption_for_upload, previous) = {
                let fm = frontmatter.borrow();
                let Some(item) = fm.media.get(index) else { return };
                (item.source.clone(), item.alt.as_wordpress_value().map(str::to_string), item.caption.clone(), item.wordpress.clone())
            };

            upload_button_for_click.set_sensitive(false);
            upload_status_label.set_label("Wird hochgeladen …");
            expander.set_subtitle(&upload_status_text(&UploadStatus::Uploading));

            let site = wpsite::load();
            let doc_dir = doc_dir.clone();
            let (tx, rx) = mpsc::channel::<Result<(wpclient::MediaResult, String), String>>();
            std::thread::spawn(move || {
                let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                    .map_err(|err| err.to_string())
                    .and_then(|maybe_password| {
                        maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
                    })
                    .and_then(|password| {
                        let client = wpclient::Client::new(&site.url, &site.username, &password);
                        let uploaded = export::upload_image_file(&client, &source, doc_dir.as_deref()).map_err(|err| err.to_string())?;
                        client
                            .update_media_metadata(uploaded.id, alt_for_upload.as_deref(), caption_for_upload.as_deref())
                            .map_err(|err| err.to_string())?;
                        // WordPress can't replace an existing attachment's file
                        // in place, so a re-upload (the "Erneut hochladen" case)
                        // always creates a new one - clean up the superseded
                        // attachment so the media library doesn't accumulate
                        // orphaned duplicates. Best-effort: the new upload
                        // already succeeded, a failed cleanup shouldn't fail this.
                        if let Some(previous) = previous {
                            let _ = client.delete_media(previous.media_id);
                        }
                        let content_hash = media::hash_local_file(&source, doc_dir.as_deref()).unwrap_or_default();
                        Ok((uploaded, content_hash))
                    });
                let _ = tx.send(outcome);
            });

            let frontmatter = frontmatter.clone();
            let expander = expander.clone();
            let upload_button = upload_button_for_click.clone();
            let upload_status_label = upload_status_label.clone();
            glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                Ok(Ok((media_result, content_hash))) => {
                    let reference = media::WordPressMediaRef { media_id: media_result.id, url: media_result.source_url.clone(), content_hash };
                    if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                        item.wordpress = Some(reference.clone());
                    }
                    expander.set_subtitle(&upload_status_text(&UploadStatus::Uploaded(reference)));
                    upload_status_label.set_label("Erfolgreich hochgeladen.");
                    upload_button.set_label("Erneut hochladen");
                    upload_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Ok(Err(err)) => {
                    expander.set_subtitle(&upload_status_text(&UploadStatus::Failed(err.clone())));
                    upload_status_label.set_label(&format!("Fehler: {err}"));
                    upload_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    upload_status_label.set_label("Interner Fehler: Upload-Thread hat kein Ergebnis geliefert.");
                    upload_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    expander.add_row(&alt_switch_row);
    expander.add_row(&alt_entry_row);
    expander.add_row(&caption_row);
    expander.add_row(&upload_row);
    expander
}
