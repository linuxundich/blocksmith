//! The "WordPress" page of the Einstellungen (settings) dialog - site URL +
//! username (persisted plainly via `wpsite`) and the Application Password
//! (persisted via the Secret Service, see `secrets`). Kept separate from
//! `properties.rs` (per-article metadata) since this is app-wide connection
//! state, not document state. Composed into the settings shell by
//! `settings.rs`, which owns the actual `Adw.PreferencesDialog`.

use adw::prelude::*;
use gtk4::glib;

use crate::{secrets, wpsite};

pub fn build_page() -> adw::PreferencesPage {
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

    let save_button = gtk4::Button::from_icon_name("document-save-symbolic");
    save_button.set_tooltip_text(Some("Speichern"));
    save_button.add_css_class("flat");

    let group = adw::PreferencesGroup::builder().title("WordPress-Verbindung").build();
    group.set_description(Some(
        "Zugangsdaten werden im Schlüsselbund gespeichert, nicht als Klartext in dieser Datei.",
    ));
    group.set_header_suffix(Some(&save_button));
    group.add(&url_row);
    group.add(&username_row);
    group.add(&password_row);

    let status_label = gtk4::Label::builder().label("").xalign(0.0).build();
    status_label.add_css_class("dim-label");
    status_label.set_wrap(true);
    status_label.set_visible(false);
    let status_group = adw::PreferencesGroup::new();
    status_group.add(&status_label);

    let page = adw::PreferencesPage::builder()
        .title("WordPress")
        .icon_name("network-server-symbolic")
        .build();
    page.add(&group);
    page.add(&status_group);

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
                Err(err) => {
                    status_label.set_label(&format!("Passwort konnte nicht geladen werden: {err}"));
                    status_label.set_visible(true);
                }
            }
        });
    }

    save_button.connect_clicked(move |_| {
        let url = url_row.text().to_string();
        let username = username_row.text().to_string();
        let password = password_row.text().to_string();

        if let Err(err) = wpsite::save(&wpsite::SiteConfig {
            url: url.clone(),
            username: username.clone(),
        }) {
            status_label.set_label(&format!("Fehler beim Speichern: {err}"));
            status_label.set_visible(true);
            return;
        }

        let status_label = status_label.clone();
        glib::MainContext::default().spawn_local(async move {
            match secrets::store_app_password(&url, &username, &password).await {
                Ok(()) => {
                    status_label.set_label("Gespeichert.");
                    status_label.set_visible(true);
                }
                Err(err) => {
                    status_label.set_label(&format!("Fehler beim Speichern des Passworts: {err}"));
                    status_label.set_visible(true);
                }
            }
        });
    });

    page
}
