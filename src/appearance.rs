//! "Erscheinungsbild" (Appearance) page in Einstellungen, modeled after
//! GNOME Builder's: an interface color-scheme picker (follow system/light/
//! dark) plus a grid of GtkSourceView style-scheme swatches for the editor,
//! using GtkSourceView's own `StyleSchemeChooserWidget` - the exact widget
//! Builder itself uses for that swatch grid, so no need to hand-roll it.

use std::path::PathBuf;

use adw::prelude::*;
use gtk4::glib;
use sourceview5::prelude::*;

const DEFAULT_SOURCE_SCHEME_ID: &str = "Adwaita";

fn config_dir() -> PathBuf {
    let mut dir = glib::user_config_dir();
    dir.push("blocksmith");
    dir
}

fn color_scheme_path() -> PathBuf {
    let mut path = config_dir();
    path.push("color_scheme.txt");
    path
}

fn source_scheme_path() -> PathBuf {
    let mut path = config_dir();
    path.push("source_scheme.txt");
    path
}

pub fn load_color_scheme() -> adw::ColorScheme {
    match std::fs::read_to_string(color_scheme_path()).ok().as_deref().map(str::trim) {
        Some("light") => adw::ColorScheme::ForceLight,
        Some("dark") => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    }
}

fn save_color_scheme(scheme: adw::ColorScheme) {
    let value = match scheme {
        adw::ColorScheme::ForceLight => "light",
        adw::ColorScheme::ForceDark => "dark",
        _ => "system",
    };
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(color_scheme_path(), value);
}

pub fn load_source_scheme_id() -> String {
    std::fs::read_to_string(source_scheme_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SOURCE_SCHEME_ID.to_string())
}

fn save_source_scheme_id(id: &str) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(source_scheme_path(), id);
}

/// Applies the saved color-scheme preference - call once at startup so the
/// app's chrome starts in the right scheme immediately rather than
/// flashing the default first.
pub fn apply_saved_color_scheme() {
    adw::StyleManager::default().set_color_scheme(load_color_scheme());
}

pub fn build_page(buffer: &sourceview5::Buffer) -> adw::PreferencesPage {
    let interface_group = adw::PreferencesGroup::builder().title("Schnittstelle").build();

    let follow_button = gtk4::ToggleButton::builder().label("Dem System folgen").build();
    let light_button = gtk4::ToggleButton::builder().label("Hell").group(&follow_button).build();
    let dark_button = gtk4::ToggleButton::builder().label("Dunkel").group(&follow_button).build();

    match load_color_scheme() {
        adw::ColorScheme::ForceLight => light_button.set_active(true),
        adw::ColorScheme::ForceDark => dark_button.set_active(true),
        _ => follow_button.set_active(true),
    }

    let scheme_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .halign(gtk4::Align::Center)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    scheme_row.add_css_class("linked");
    scheme_row.append(&follow_button);
    scheme_row.append(&light_button);
    scheme_row.append(&dark_button);

    follow_button.connect_toggled(|button| {
        if button.is_active() {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
            save_color_scheme(adw::ColorScheme::Default);
        }
    });
    light_button.connect_toggled(|button| {
        if button.is_active() {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
            save_color_scheme(adw::ColorScheme::ForceLight);
        }
    });
    dark_button.connect_toggled(|button| {
        if button.is_active() {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
            save_color_scheme(adw::ColorScheme::ForceDark);
        }
    });

    interface_group.add(&scheme_row);

    let color_group = adw::PreferencesGroup::builder().title("Editor-Farbschema").build();
    let chooser = sourceview5::StyleSchemeChooserWidget::new();
    if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(&load_source_scheme_id()) {
        chooser.set_style_scheme(&scheme);
    }
    chooser.set_vexpand(true);
    chooser.set_height_request(320);

    let buffer = buffer.clone();
    chooser.connect_style_scheme_notify(move |chooser| {
        let scheme = chooser.style_scheme();
        save_source_scheme_id(&scheme.id());
        buffer.set_style_scheme(Some(&scheme));
    });

    color_group.add(&chooser);

    let page = adw::PreferencesPage::builder().title("Erscheinungsbild").icon_name("preferences-desktop-appearance-symbolic").build();
    page.add(&interface_group);
    page.add(&color_group);
    page
}
