//! "Kategorien & Tags verwalten" dialog: lists every existing WordPress
//! category/tag with its real term id (`termcache`'s autocomplete cache only
//! ever holds names, which isn't enough to rename or delete) so an existing
//! term can be fixed or removed in place - until now, categories/tags could
//! only ever be *read* (autocomplete in `properties.rs`) or *created*
//! (automatically, on publish, for a name that doesn't exist yet).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::i18n::tr;
use crate::{secrets, termcache, wpclient, wpsite};

pub fn open(parent: &adw::ApplicationWindow, category_terms: Rc<RefCell<Vec<String>>>, tag_terms: Rc<RefCell<Vec<String>>>) {
    let status_label = gtk4::Label::new(Some(&tr("Lade Kategorien & Tags …")));
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);

    let categories_group = adw::PreferencesGroup::builder().title(tr("Kategorien")).build();
    categories_group.set_visible(false);
    let tags_group = adw::PreferencesGroup::builder().title(tr("Tags")).build();
    tags_group.set_visible(false);

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content_box.append(&status_label);
    content_box.append(&categories_group);
    content_box.append(&tags_group);

    let scroller = gtk4::ScrolledWindow::builder().child(&content_box).vexpand(true).build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title(tr("Kategorien & Tags verwalten"))
        .content_width(480)
        .content_height(560)
        .child(&toolbar_view)
        .build();
    dialog.present(Some(parent));

    let site = wpsite::load();
    if site.url.is_empty() {
        status_label.set_label(&tr("Keine WordPress-Seite konfiguriert."));
        return;
    }

    let (tx, rx) = mpsc::channel::<Result<(Vec<wpclient::Term>, Vec<wpclient::Term>), String>>();
    std::thread::spawn(move || {
        let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .map_err(|err| err.to_string())
            .and_then(|maybe_password| maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden.")))
            .and_then(|password| {
                let client = wpclient::Client::new(&site.url, &site.username, &password);
                let categories = client.list_terms("categories").map_err(|err| err.to_string())?;
                let tags = client.list_terms("tags").map_err(|err| err.to_string())?;
                Ok((categories, tags))
            });
        let _ = tx.send(outcome);
    });

    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok((categories, tags))) => {
            if categories.is_empty() && tags.is_empty() {
                status_label.set_label(&tr("Keine Kategorien oder Tags vorhanden."));
            } else {
                status_label.set_visible(false);
            }
            categories_group.set_visible(!categories.is_empty());
            for term in categories {
                categories_group.add(&build_row(term, "categories", &category_terms, &tag_terms, &dialog, &status_label, &categories_group));
            }
            tags_group.set_visible(!tags.is_empty());
            for term in tags {
                tags_group.add(&build_row(term, "tags", &category_terms, &tag_terms, &dialog, &status_label, &tags_group));
            }
            glib::ControlFlow::Break
        }
        Ok(Err(err)) => {
            status_label.set_label(&format!("{}: {err}", tr("Fehler")));
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            status_label.set_label(&tr("Interner Fehler: kein Ergebnis vom Ladevorgang."));
            glib::ControlFlow::Break
        }
    });
}

/// One editable term row: renaming re-sends the whole current text as the
/// new name (the "Umbenennen" button only lights up once the text actually
/// differs from what's on the server), deleting asks for confirmation first
/// since it's unrecoverable. Both mutations re-run `termcache::spawn_refresh`
/// on success so the properties dialog's autocomplete reflects the change
/// immediately, without needing its own separate WordPress round trip here.
fn build_row(
    term: wpclient::Term,
    taxonomy: &'static str,
    category_terms: &Rc<RefCell<Vec<String>>>,
    tag_terms: &Rc<RefCell<Vec<String>>>,
    dialog: &adw::Dialog,
    status_label: &gtk4::Label,
    group: &adw::PreferencesGroup,
) -> adw::EntryRow {
    let term_id = term.id;
    let row = adw::EntryRow::builder().title(tr("Name")).text(term.name.as_str()).build();
    let original_name = Rc::new(RefCell::new(term.name));

    let save_button = gtk4::Button::from_icon_name("document-save-symbolic");
    save_button.set_tooltip_text(Some(&tr("Umbenennen")));
    save_button.set_valign(gtk4::Align::Center);
    save_button.add_css_class("flat");
    save_button.set_sensitive(false);
    row.add_suffix(&save_button);

    let delete_button = gtk4::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some(&tr("Löschen")));
    delete_button.set_valign(gtk4::Align::Center);
    delete_button.add_css_class("flat");
    row.add_suffix(&delete_button);

    {
        let save_button = save_button.clone();
        let original_name = original_name.clone();
        row.connect_changed(move |row| {
            let text = row.text();
            save_button.set_sensitive(!text.trim().is_empty() && text.as_str() != original_name.borrow().as_str());
        });
    }

    {
        let row = row.clone();
        let original_name = original_name.clone();
        let category_terms = category_terms.clone();
        let tag_terms = tag_terms.clone();
        let status_label = status_label.clone();
        let save_button_for_click = save_button.clone();
        let delete_button_for_click = delete_button.clone();
        save_button.connect_clicked(move |_| {
            let new_name = row.text().to_string();
            save_button_for_click.set_sensitive(false);
            delete_button_for_click.set_sensitive(false);
            row.set_sensitive(false);
            status_label.set_visible(false);

            let site = wpsite::load();
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let new_name_for_thread = new_name.clone();
            std::thread::spawn(move || {
                let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                    .map_err(|err| err.to_string())
                    .and_then(|maybe_password| maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden.")))
                    .and_then(|password| {
                        wpclient::Client::new(&site.url, &site.username, &password)
                            .rename_term(taxonomy, term_id, &new_name_for_thread)
                            .map_err(|err| err.to_string())
                    });
                let _ = tx.send(outcome);
            });

            let row = row.clone();
            let original_name = original_name.clone();
            let category_terms = category_terms.clone();
            let tag_terms = tag_terms.clone();
            let status_label = status_label.clone();
            let save_button = save_button_for_click.clone();
            let delete_button = delete_button_for_click.clone();
            glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                Ok(Ok(())) => {
                    *original_name.borrow_mut() = new_name.clone();
                    row.set_sensitive(true);
                    delete_button.set_sensitive(true);
                    save_button.set_sensitive(false);
                    termcache::spawn_refresh(category_terms.clone(), tag_terms.clone());
                    glib::ControlFlow::Break
                }
                Ok(Err(err)) => {
                    row.set_sensitive(true);
                    delete_button.set_sensitive(true);
                    save_button.set_sensitive(true);
                    status_label.set_label(&format!("{}: {err}", tr("Fehler beim Umbenennen")));
                    status_label.set_visible(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    row.set_sensitive(true);
                    delete_button.set_sensitive(true);
                    save_button.set_sensitive(true);
                    status_label.set_label(&tr("Interner Fehler: kein Ergebnis vom Umbenennen-Thread."));
                    status_label.set_visible(true);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    {
        let dialog = dialog.clone();
        let row = row.clone();
        let category_terms = category_terms.clone();
        let tag_terms = tag_terms.clone();
        let status_label = status_label.clone();
        let group = group.clone();
        delete_button.connect_clicked(move |delete_button| {
            let confirm = adw::AlertDialog::new(
                Some(&tr("Term wirklich löschen?")),
                Some(&tr("Der Begriff wird unwiderruflich von der WordPress-Seite gelöscht.")),
            );
            confirm.add_response("cancel", &tr("Abbrechen"));
            confirm.add_response("delete", &tr("Löschen"));
            confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");

            let row = row.clone();
            let category_terms = category_terms.clone();
            let tag_terms = tag_terms.clone();
            let status_label = status_label.clone();
            let delete_button = delete_button.clone();
            let group = group.clone();
            confirm.connect_response(None, move |_, response| {
                if response != "delete" {
                    return;
                }
                row.set_sensitive(false);
                delete_button.set_sensitive(false);
                status_label.set_visible(false);

                let site = wpsite::load();
                let (tx, rx) = mpsc::channel::<Result<(), String>>();
                std::thread::spawn(move || {
                    let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                        .map_err(|err| err.to_string())
                        .and_then(|maybe_password| maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden.")))
                        .and_then(|password| {
                            wpclient::Client::new(&site.url, &site.username, &password)
                                .delete_term(taxonomy, term_id)
                                .map_err(|err| err.to_string())
                        });
                    let _ = tx.send(outcome);
                });

                let row = row.clone();
                let category_terms = category_terms.clone();
                let tag_terms = tag_terms.clone();
                let status_label = status_label.clone();
                let delete_button = delete_button.clone();
                let group = group.clone();
                glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                    Ok(Ok(())) => {
                        group.remove(&row);
                        termcache::spawn_refresh(category_terms.clone(), tag_terms.clone());
                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        row.set_sensitive(true);
                        delete_button.set_sensitive(true);
                        status_label.set_label(&format!("{}: {err}", tr("Fehler beim Löschen")));
                        status_label.set_visible(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        row.set_sensitive(true);
                        delete_button.set_sensitive(true);
                        status_label.set_label(&tr("Interner Fehler: kein Ergebnis vom Lösch-Thread."));
                        status_label.set_visible(true);
                        glib::ControlFlow::Break
                    }
                });
            });
            confirm.present(Some(&dialog));
        });
    }

    row
}
