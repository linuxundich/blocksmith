//! "Einstellungen" (app settings, as opposed to `properties.rs`'s per-article
//! metadata): a standard `Adw.PreferencesDialog` shell. Currently holds a
//! single WordPress-connection page (`connection::build_page`); further
//! settings pages would just be additional `dialog.add(...)` calls here.

use adw::prelude::*;

use crate::connection;

pub fn open(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::builder().title("Einstellungen").build();
    dialog.add(&connection::build_page());
    dialog.present(Some(parent));
}
