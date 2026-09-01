//! The left-hand Markdown editing pane: a `GtkSourceView` with markdown
//! syntax highlighting inside a scrolled window, plus spell-checking via
//! `libspelling` (the GTK4-native successor to gspell, which never got a
//! GTK4 port).

use gtk4::prelude::*;
use libspelling as spelling;
use sourceview5::prelude::*;

use crate::appearance;

pub fn build() -> (gtk4::ScrolledWindow, sourceview5::View, sourceview5::Buffer) {
    let buffer = sourceview5::Buffer::new(None::<&gtk4::TextTagTable>);

    if let Some(lang) = sourceview5::LanguageManager::default().language("markdown") {
        buffer.set_language(Some(&lang));
    }
    buffer.set_highlight_syntax(true);

    if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(&appearance::load_source_scheme_id()) {
        buffer.set_style_scheme(Some(&scheme));
    }

    let view = sourceview5::View::with_buffer(&buffer);
    view.set_show_line_numbers(true);
    view.set_wrap_mode(gtk4::WrapMode::WordChar);
    view.set_monospace(true);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_hexpand(true);
    view.set_vexpand(true);

    let checker = spelling::Checker::default();
    let adapter = spelling::TextBufferAdapter::new(&buffer, &checker);
    adapter.set_enabled(true);
    // Both kept alive by the view: inserted action groups and the buffer's
    // own signal connections hold their own strong references.
    view.insert_action_group("spelling", Some(&adapter));
    view.set_extra_menu(Some(&adapter.menu_model()));

    let scroller = gtk4::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();

    (scroller, view, buffer)
}
