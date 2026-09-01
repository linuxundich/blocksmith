//! "WordPress-Verbindung" dialog: site URL + username (persisted plainly via
//! `wpsite`) and the Application Password (persisted via the Secret Service,
//! see `secrets`). Kept separate from `properties.rs` (per-article
//! metadata) since this is app-wide connection state, not document state.

use adw::prelude::*;
use gtk4::glib;

use crate::{secrets, wpsite};

pub fn open(parent: &adw::ApplicationWindow) {
    let config = wpsite::load();

    let url_row = adw::EntryRow::builder()
        .title("Website-URL")
        .text(config.url.as_str())
        .build();
    let username_row = adw::EntryRow::builder()
        .title("Benutzername")
        .text(config.username.as_str())
        .build();
    let password_row = adw::PasswordEntryRow::builder().title("Application Password").build();

    let group = adw::PreferencesGroup::builder().title("WordPress-Verbindung").build();
    group.set_description(Some(
        "Zugangsdaten werden im Schlüsselbund gespeichert, nicht als Klartext in dieser Datei.",
    ));
    group.add(&url_row);
    group.add(&username_row);
    group.add(&password_row);

    let status_label = gtk4::Label::builder().label("").margin_top(6).build();
    status_label.set_wrap(true);

    let save_button = gtk4::Button::builder()
        .label("Speichern")
        .halign(gtk4::Align::End)
        .margin_top(12)
        .build();
    save_button.add_css_class("suggested-action");

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content_box.append(&group);
    content_box.append(&status_label);
    content_box.append(&save_button);

    let clamp = adw::Clamp::builder().maximum_size(480).child(&content_box).build();
    let scroller = gtk4::ScrolledWindow::builder().child(&clamp).vexpand(true).build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title("WordPress-Verbindung")
        .content_width(480)
        .content_height(420)
        .child(&toolbar_view)
        .build();

    // Prefill the password field from the keyring, if a matching entry exists.
    if !config.url.is_empty() && !config.username.is_empty() {
        let password_row = password_row.clone();
        let status_label = status_label.clone();
        let url = config.url.clone();
        let username = config.username.clone();
        glib::MainContext::default().spawn_local(async move {
            match secrets::load_app_password(&url, &username).await {
                Ok(Some(password)) => password_row.set_text(&password),
                Ok(None) => {}
                Err(err) => status_label.set_label(&format!("Passwort konnte nicht geladen werden: {err}")),
            }
        });
    }

    let dialog_weak = dialog.downgrade();
    save_button.connect_clicked(move |_| {
        let url = url_row.text().to_string();
        let username = username_row.text().to_string();
        let password = password_row.text().to_string();

        if let Err(err) = wpsite::save(&wpsite::SiteConfig {
            url: url.clone(),
            username: username.clone(),
        }) {
            status_label.set_label(&format!("Fehler beim Speichern: {err}"));
            return;
        }

        let status_label = status_label.clone();
        let dialog_weak = dialog_weak.clone();
        glib::MainContext::default().spawn_local(async move {
            match secrets::store_app_password(&url, &username, &password).await {
                Ok(()) => {
                    if let Some(dialog) = dialog_weak.upgrade() {
                        dialog.close();
                    }
                }
                Err(err) => status_label.set_label(&format!("Fehler beim Speichern des Passworts: {err}")),
            }
        });
    });

    dialog.present(Some(parent));
}
