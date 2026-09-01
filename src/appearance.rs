//! "Erscheinungsbild" (Appearance) page in Einstellungen, modeled after
//! GNOME Builder's: an interface color-scheme picker (follow system/light/
//! dark) shown as mini window-mockup preview cards - the same visual
//! language Builder, Settings, and Text Editor all use for this choice -
//! plus a live syntax-highlighted code sample and a grid of GtkSourceView
//! style-scheme swatches for the editor, using GtkSourceView's own
//! `StyleSchemeChooserWidget` for the grid itself (the exact widget Builder
//! uses there, so no need to hand-roll that part).

use std::path::PathBuf;
use std::sync::Once;

use adw::prelude::*;
use gtk4::glib;
use sourceview5::prelude::*;

const DEFAULT_SOURCE_SCHEME_ID: &str = "Adwaita";

const PREVIEW_SAMPLE: &str = "\
// Welche Wörter welche Farbe bekommen
fn greet(name: &str) -> String {
    format!(\"Hallo, {name}!\")
}
";

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

static CSS_INSTALLED: Once = Once::new();

/// The theme-card visuals (mini window mockups, selection border) aren't
/// covered by any built-in libadwaita widget - Builder, Settings, and Text
/// Editor all hand-roll this exact look, usually from bundled thumbnail
/// images. We draw it from plain boxes + CSS instead, so it needs no
/// bundled assets and still reads clearly at this size.
fn install_theme_card_css() {
    CSS_INSTALLED.call_once(|| {
        let Some(display) = gtk4::gdk::Display::default() else {
            return;
        };
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(
            "
            .theme-card { padding: 6px; border-radius: 12px; }
            .theme-card-preview {
                border-radius: 8px;
                border: 1px solid alpha(currentColor, 0.15);
            }
            .theme-card:checked .theme-card-preview {
                border: 2px solid @accent_bg_color;
            }
            .theme-card-window-light { background-color: #ffffff; }
            .theme-card-window-dark { background-color: #241f31; }
            .theme-card-window-light .theme-card-header { background-color: alpha(#000000, 0.06); }
            .theme-card-window-dark .theme-card-header { background-color: alpha(#ffffff, 0.08); }
            .theme-card-window-light .theme-card-bar { background-color: alpha(#000000, 0.25); }
            .theme-card-window-dark .theme-card-bar { background-color: alpha(#ffffff, 0.3); }
            .theme-card-bar.accent { background-color: @accent_bg_color; }
            .theme-card-dot { border-radius: 9999px; background-color: #e01b24; }
            ",
        );
        gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    });
}

fn bar(width: i32, accent: bool) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    b.add_css_class("theme-card-bar");
    if accent {
        b.add_css_class("accent");
    }
    b.set_size_request(width, 4);
    b.set_halign(gtk4::Align::Start);
    b
}

/// One mini "window" mockup: a header strip with a corner dot, plus a
/// couple of bars standing in for a few lines of text - at `width` wide,
/// so the same builder can make a full-size card or one half of the
/// "follow system" split preview.
fn mini_window(variant: &str, width: i32) -> gtk4::Box {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header.add_css_class("theme-card-header");
    header.set_size_request(-1, 12);
    let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    dot.add_css_class("theme-card-dot");
    dot.set_size_request(5, 5);
    dot.set_valign(gtk4::Align::Center);
    dot.set_margin_start(5);
    header.append(&dot);

    let bars = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(4).margin_top(6).margin_start(6).margin_end(6).build();
    bars.append(&bar(width - 24, false));
    bars.append(&bar(width - 36, false));
    bars.append(&bar(width - 44, true));

    let window = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    window.add_css_class(&format!("theme-card-window-{variant}"));
    window.set_size_request(width, 72);
    window.append(&header);
    window.append(&bars);
    window
}

fn build_theme_card(label_text: &str, variant: &str, group: Option<&gtk4::ToggleButton>) -> gtk4::ToggleButton {
    let preview = if variant == "system" {
        let split = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        split.append(&mini_window("light", 48));
        split.append(&mini_window("dark", 48));
        split.add_css_class("theme-card-preview");
        split
    } else {
        let single = mini_window(variant, 96);
        single.add_css_class("theme-card-preview");
        single
    };

    let label = gtk4::Label::new(Some(label_text));

    let content = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(6).halign(gtk4::Align::Center).build();
    content.append(&preview);
    content.append(&label);

    let mut builder = gtk4::ToggleButton::builder().child(&content);
    if let Some(group) = group {
        builder = builder.group(group);
    }
    let button = builder.build();
    button.add_css_class("theme-card");
    button.add_css_class("flat");
    button
}

pub fn build_page(buffer: &sourceview5::Buffer) -> adw::PreferencesPage {
    install_theme_card_css();

    let interface_group = adw::PreferencesGroup::builder().title("Schnittstelle").build();

    let follow_button = build_theme_card("Dem System folgen", "system", None);
    let light_button = build_theme_card("Hell", "light", Some(&follow_button));
    let dark_button = build_theme_card("Dunkel", "dark", Some(&follow_button));

    match load_color_scheme() {
        adw::ColorScheme::ForceLight => light_button.set_active(true),
        adw::ColorScheme::ForceDark => dark_button.set_active(true),
        _ => follow_button.set_active(true),
    }

    let scheme_row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(12).halign(gtk4::Align::Center).margin_top(12).margin_bottom(12).build();
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

    let color_group = adw::PreferencesGroup::builder().title("Farbe").build();

    let initial_scheme = sourceview5::StyleSchemeManager::default().scheme(&load_source_scheme_id());

    // A small live sample, styled with whatever scheme is currently
    // selected - mirrors Builder's own "Farbe" page, where the code sample
    // sits above the swatch grid so a pick's effect is visible immediately
    // rather than only after closing the dialog.
    let sample_buffer = sourceview5::Buffer::new(None::<&gtk4::TextTagTable>);
    sample_buffer.set_text(PREVIEW_SAMPLE);
    sample_buffer.set_highlight_syntax(true);
    if let Some(lang) = sourceview5::LanguageManager::default().language("rust") {
        sample_buffer.set_language(Some(&lang));
    }
    if let Some(scheme) = &initial_scheme {
        sample_buffer.set_style_scheme(Some(scheme));
    }
    let sample_view = sourceview5::View::with_buffer(&sample_buffer);
    sample_view.set_editable(false);
    sample_view.set_cursor_visible(false);
    sample_view.set_monospace(true);
    sample_view.set_top_margin(8);
    sample_view.set_bottom_margin(8);
    sample_view.set_left_margin(10);
    sample_view.set_right_margin(10);
    let sample_scroller = gtk4::ScrolledWindow::builder()
        .child(&sample_view)
        .height_request(110)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    sample_scroller.add_css_class("card");
    color_group.add(&sample_scroller);

    let chooser = sourceview5::StyleSchemeChooserWidget::new();
    if let Some(scheme) = &initial_scheme {
        chooser.set_style_scheme(scheme);
    }
    chooser.set_vexpand(true);
    chooser.set_height_request(320);

    let buffer = buffer.clone();
    chooser.connect_style_scheme_notify(move |chooser| {
        let scheme = chooser.style_scheme();
        save_source_scheme_id(&scheme.id());
        buffer.set_style_scheme(Some(&scheme));
        sample_buffer.set_style_scheme(Some(&scheme));
    });

    color_group.add(&chooser);

    let page = adw::PreferencesPage::builder().title("Erscheinungsbild").icon_name("preferences-desktop-appearance-symbolic").build();
    page.add(&interface_group);
    page.add(&color_group);
    page
}
