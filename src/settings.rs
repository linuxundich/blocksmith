//! "Einstellungen" (app settings, as opposed to `properties.rs`'s per-article
//! metadata): a standard `Adw.PreferencesDialog` shell holding one page per
//! concern - appearance (`appearance::build_page`), WordPress connection
//! (`connection::build_page`), and the Gemini chat (`chatsettings::build_page`).
//! Further settings pages would just be additional `dialog.add(...)` calls
//! here.

use adw::prelude::*;

use crate::{appearance, chatsettings, connection};

pub fn open(parent: &adw::ApplicationWindow, buffer: &sourceview5::Buffer) {
    let dialog = adw::PreferencesDialog::builder().title("Einstellungen").build();
    dialog.add(&appearance::build_page(buffer));
    dialog.add(&connection::build_page());
    dialog.add(&chatsettings::build_page());
    dialog.present(Some(parent));
}
