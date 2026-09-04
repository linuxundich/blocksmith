//! Adds an "Alternativtext festlegen…" item to the editor's right-click
//! context menu - alt text (and caption) already exist as a full feature
//! via Medienverwaltung/the export dialog's "Medien" tab, but reaching
//! them meant leaving the editor entirely. This lets a right-click on the
//! specific line holding an image reference jump straight to that image's
//! fields, backed by the exact same `MediaItem`/`AltText` model.
//!
//! A plain `Gio.Menu`-based `extra-menu` (what `aimenu.rs` already builds)
//! can't conditionally hide an item depending on where the click landed -
//! so the item is always present, and clicking it when the line has no
//! image reference shows a short explanation instead of silently doing
//! nothing. "Where the click landed" comes from the buffer's insertion
//! mark at the moment the menu item is activated - but a plain right-click
//! does NOT reposition that mark on its own (confirmed live: it stayed
//! wherever an earlier left-click/edit had left it), unlike e.g. a web
//! browser's text field. So a small `Gtk.GestureClick` explicitly moves
//! the cursor to the click point on every secondary-button press, purely
//! so the existing cursor-based lookup has an accurate position to read.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::gio;

use crate::document::Frontmatter;
use crate::media::{self, AltText};

/// The context-menu section to merge into the editor's combined
/// `extra-menu` (see `aimenu.rs`, the sole caller of `view.set_extra_menu`).
pub fn menu_section() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Alternativtext festlegen…"), Some("imagealt.set"));
    menu
}

/// Moves the cursor to wherever a secondary click lands (see the module
/// docs for why this can't just rely on default `Gtk.TextView` behavior),
/// and wires the `imagealt.set` action activated by the menu item
/// `menu_section` returns, which then reads the (now-accurate) cursor line.
pub fn install(view: &sourceview5::View, buffer: &sourceview5::Buffer, frontmatter: Rc<RefCell<Frontmatter>>) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3); // secondary/right button
    {
        let buffer = buffer.clone();
        let view_weak = view.downgrade();
        gesture.connect_pressed(move |_gesture, _n_press, x, y| {
            let Some(view) = view_weak.upgrade() else { return };
            let (buffer_x, buffer_y) = view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
            if let Some((iter, _trailing)) = view.iter_at_position(buffer_x, buffer_y) {
                buffer.place_cursor(&iter);
            }
        });
    }
    view.add_controller(gesture);

    let actions = gio::SimpleActionGroup::new();
    let set_action = gio::SimpleAction::new("set", None);
    {
        let buffer = buffer.clone();
        let view_weak = view.downgrade();
        set_action.connect_activate(move |_, _| {
            let Some(view) = view_weak.upgrade() else { return };
            let Some(window) = view.root().and_then(|root| root.downcast::<gtk4::Window>().ok()) else {
                return;
            };
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            open_for_line(&window, &buffer, &frontmatter, line);
        });
    }
    actions.add_action(&set_action);
    view.insert_action_group("imagealt", Some(&actions));
}

/// Finds the `![alt](source)` reference starting on `line` (0-indexed,
/// matching `Gtk.TextIter::line()`), reconciles the tracked media list
/// against the current body so the reference is guaranteed to have a
/// `MediaItem`, then opens a small dialog for just that one image's alt
/// text and caption - the same fields and wiring as `mediapanel.rs`'s row,
/// minus the upload button, which isn't part of what a quick in-editor
/// shortcut needs.
fn open_for_line(window: &gtk4::Window, buffer: &sourceview5::Buffer, frontmatter: &Rc<RefCell<Frontmatter>>, line: i32) {
    let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();

    let Some(source) = image_source_on_line(&body, line) else {
        let alert = adw::AlertDialog::builder()
            .heading("Keine Bildreferenz gefunden")
            .body("Für den Alternativtext bitte mit der rechten Maustaste auf eine Zeile mit einem Bild (![Beschreibung](bild.png)) klicken.")
            .build();
        alert.add_response("ok", "OK");
        alert.present(Some(window));
        return;
    };

    {
        let mut fm = frontmatter.borrow_mut();
        fm.media = media::reconcile(&fm.media, &body);
    }
    let Some(index) = frontmatter.borrow().media.iter().position(|item| item.source == source) else {
        return;
    };
    let item = frontmatter.borrow().media[index].clone();

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
        alt_switch_row.connect_active_notify(move |row| {
            let active = row.is_active();
            alt_entry_row.set_visible(active);
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if active {
                    let text = alt_entry_row.text().to_string();
                    if text.is_empty() { AltText::Empty } else { AltText::Text(text) }
                } else {
                    AltText::Undefined
                };
            }
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let alt_switch_row = alt_switch_row.clone();
        alt_entry_row.connect_changed(move |row| {
            if !alt_switch_row.is_active() {
                return;
            }
            let text = row.text().to_string();
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if text.is_empty() { AltText::Empty } else { AltText::Text(text) };
            }
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

    let group = adw::PreferencesGroup::builder().title(&item.filename).build();
    group.add(&alt_switch_row);
    group.add(&alt_entry_row);
    group.add(&caption_row);

    let clamp = adw::Clamp::builder().maximum_size(420).child(&group).build();
    let content = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    content.append(&clamp);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&adw::HeaderBar::new());
    toolbar_view.set_content(Some(&content));

    let dialog = adw::Dialog::builder().title("Alternativtext").content_width(420).content_height(340).child(&toolbar_view).build();
    dialog.present(Some(window));
}

/// Scans `markdown` for an `![alt](source)` reference whose opening `![`
/// starts on `line` (0-indexed) - deliberately line-based rather than
/// matching by click column, since the whole point is "the image this
/// line is about", not needing the click to land exactly on the syntax.
fn image_source_on_line(markdown: &str, line: i32) -> Option<String> {
    for (event, range) in pulldown_cmark::Parser::new(markdown).into_offset_iter() {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event {
            let start_line = markdown[..range.start].matches('\n').count() as i32;
            if start_line == line {
                return Some(dest_url.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_image_source_on_the_given_line() {
        let markdown = "Intro.\n\n![a cat](cat.png)\n\nOutro.\n";
        assert_eq!(image_source_on_line(markdown, 2), Some("cat.png".to_string()));
    }

    #[test]
    fn returns_none_for_a_line_without_an_image() {
        let markdown = "Intro.\n\n![a cat](cat.png)\n\nOutro.\n";
        assert_eq!(image_source_on_line(markdown, 0), None);
        assert_eq!(image_source_on_line(markdown, 4), None);
    }
}
