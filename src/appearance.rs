//! "Erscheinungsbild" (Appearance) page in Einstellungen, adopted directly
//! from GNOME Builder's own implementation (`gnome-builder.git`,
//! `src/plugins/platformui/gbp-platformui-tweaks-addin.c` and
//! `src/plugins/editorui/gbp-editorui-scheme-selector.c`):
//!
//! - The interface style picker (follow-system/light/dark) uses Builder's
//!   own bundled preview illustrations (`data/icons/appearance-preview/`,
//!   see `ATTRIBUTION.md` there) inside a `GtkPicture`, exactly like
//!   `IdeStyleVariantPreview` does - not a hand-drawn approximation.
//! - The color-scheme grid uses GtkSourceView's own `StyleSchemePreview`
//!   widget (the same one Builder's `GbpEditoruiSchemeSelector` uses) laid
//!   out in a `GtkFlowBox`, filtered to the schemes matching the current
//!   light/dark mode (Builder's `update_style_schemes`/`is_dark` logic,
//!   ported to Rust below) rather than showing every scheme at once.

use std::path::PathBuf;

use adw::prelude::*;
use gtk4::glib;
use sourceview5::prelude::*;

const DEFAULT_SOURCE_SCHEME_ID: &str = "Adwaita";

const PREVIEW_LIGHT_SVG: &[u8] = include_bytes!("../data/icons/appearance-preview/preview-light.svg");
const PREVIEW_DARK_SVG: &[u8] = include_bytes!("../data/icons/appearance-preview/preview-dark.svg");
const PREVIEW_SYSTEM_SVG: &[u8] = include_bytes!("../data/icons/appearance-preview/preview-system.svg");

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

/// Port of Builder's `ide_source_style_scheme_is_dark()`
/// (`src/libide/sourceview/ide-source-style-scheme.c`): prefer the
/// scheme's own "variant" metadata or an "-dark" id suffix, and fall back
/// to the perceived brightness (HSP) of its "text" style's background.
fn scheme_is_dark(scheme: &sourceview5::StyleScheme) -> bool {
    match scheme.metadata("variant").as_deref() {
        Some("light") => return false,
        Some("dark") => return true,
        _ => {}
    }
    if scheme.id().contains("-dark") {
        return true;
    }
    if let Some(style) = scheme.style("text") {
        if style.is_background_set() {
            if let Some(bg) = style.background() {
                if let Ok(rgba) = gtk4::gdk::RGBA::parse(&bg) {
                    let (r, g, b) = (f64::from(rgba.red()) * 255.0, f64::from(rgba.green()) * 255.0, f64::from(rgba.blue()) * 255.0);
                    let hsp = (0.299 * r * r + 0.587 * g * g + 0.114 * b * b).sqrt();
                    return hsp <= 127.5;
                }
            }
        }
    }
    false
}

static CSS_INSTALLED: std::sync::Once = std::sync::Once::new();

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
            ",
        );
        gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    });
}

fn picture_from_svg_bytes(bytes: &'static [u8]) -> gtk4::Picture {
    let texture = gtk4::gdk::Texture::from_bytes(&glib::Bytes::from_static(bytes)).expect("bundled preview SVG should always parse");
    let picture = gtk4::Picture::for_paintable(&texture);
    picture.set_content_fit(gtk4::ContentFit::Fill);
    picture.set_can_shrink(true);
    picture.set_size_request(148, 81); // matches the SVGs' native 164:90 aspect ratio
    picture
}

fn build_theme_card(label_text: &str, svg_bytes: &'static [u8], group: Option<&gtk4::ToggleButton>) -> gtk4::ToggleButton {
    let picture = picture_from_svg_bytes(svg_bytes);
    let preview = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    preview.add_css_class("theme-card-preview");
    preview.set_overflow(gtk4::Overflow::Hidden);
    preview.append(&picture);

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

/// Rebuilds the scheme `flow_box`'s children from the schemes matching
/// `is_dark` - Builder's `update_style_schemes()`, minus the light/dark
/// "alternate variant" bookkeeping (Blocksmith doesn't offer per-scheme
/// variant swapping, just the filtered list itself).
fn populate_scheme_flow_box(flow_box: &gtk4::FlowBox, buffer: &sourceview5::Buffer) {
    while let Some(child) = flow_box.first_child() {
        flow_box.remove(&child);
    }

    let manager = sourceview5::StyleSchemeManager::default();
    let is_dark = adw::StyleManager::default().is_dark();
    let current_id = load_source_scheme_id();

    let mut schemes: Vec<sourceview5::StyleScheme> = manager
        .scheme_ids()
        .iter()
        .filter(|id| id.as_str() != "printing")
        .filter_map(|id| manager.scheme(id))
        .collect();
    schemes.sort_by_key(|s| s.name().to_string());

    for scheme in schemes {
        if scheme_is_dark(&scheme) != is_dark && scheme.id() != current_id {
            continue;
        }

        let preview = sourceview5::StyleSchemePreview::new(&scheme);
        preview.set_selected(scheme.id() == current_id);

        let buffer = buffer.clone();
        let flow_box_weak = flow_box.downgrade();
        preview.connect_activate(move |activated| {
            let scheme = activated.scheme();
            save_source_scheme_id(&scheme.id());
            buffer.set_style_scheme(Some(&scheme));
            if let Some(flow_box) = flow_box_weak.upgrade() {
                let mut child = flow_box.first_child();
                while let Some(c) = child {
                    if let Some(flow_child) = c.downcast_ref::<gtk4::FlowBoxChild>() {
                        if let Some(p) = flow_child.child().and_then(|w| w.downcast::<sourceview5::StyleSchemePreview>().ok()) {
                            p.set_selected(p.scheme().id() == scheme.id());
                        }
                    }
                    child = c.next_sibling();
                }
            }
        });

        flow_box.insert(&preview, -1);
    }
}

pub fn build_page(buffer: &sourceview5::Buffer) -> adw::PreferencesPage {
    install_theme_card_css();

    let interface_group = adw::PreferencesGroup::builder().title("Schnittstelle").build();

    let follow_button = build_theme_card("Dem System folgen", PREVIEW_SYSTEM_SVG, None);
    let light_button = build_theme_card("Hell", PREVIEW_LIGHT_SVG, Some(&follow_button));
    let dark_button = build_theme_card("Dunkel", PREVIEW_DARK_SVG, Some(&follow_button));

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

    let flow_box = gtk4::FlowBox::builder().column_spacing(12).row_spacing(12).max_children_per_line(4).selection_mode(gtk4::SelectionMode::None).homogeneous(true).build();
    flow_box.add_css_class("style-schemes");
    populate_scheme_flow_box(&flow_box, buffer);

    {
        let flow_box = flow_box.clone();
        let buffer = buffer.clone();
        adw::StyleManager::default().connect_dark_notify(move |_| {
            populate_scheme_flow_box(&flow_box, &buffer);
        });
    }

    color_group.add(&flow_box);

    let page = adw::PreferencesPage::builder().title("Erscheinungsbild").icon_name("preferences-desktop-appearance-symbolic").build();
    page.add(&interface_group);
    page.add(&color_group);
    page
}
