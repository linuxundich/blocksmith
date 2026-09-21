//! Adds an "Alternativtext festlegen…" item to the editor's right-click
//! context menu - alt text (and caption) already exist as a full feature
//! via Medienverwaltung/the export dialog's "Medien" tab, but reaching
//! them meant leaving the editor entirely. This lets a right-click on the
//! specific line holding an image reference jump straight to that image's
//! fields, backed by the exact same `MediaItem`/`AltText` model.
//!
//! The menu section (`install`'s returned `gio::Menu` handle) starts empty
//! and is rebuilt on every secondary click from whether the clicked line
//! actually holds a media reference - mutating an already-installed
//! `gio::Menu` is reflected immediately in the popover showing it, the
//! same live-update trick `aimenu.rs` already uses for its custom-prompts
//! section - so the item only ever appears when it would actually do
//! something, instead of always showing and popping an explanation dialog
//! when clicked on a line with nothing to act on. "KI-Alternativtext
//! generieren…" additionally only appears for an actual image (not
//! video/audio), since vision-based generation makes no sense for those.
//! "Where the click landed" comes from the buffer's insertion mark at the
//! moment the menu item is built/activated - but a plain right-click does
//! NOT reposition that mark on its own (confirmed live: it stayed wherever
//! an earlier left-click/edit had left it), unlike e.g. a web browser's
//! text field. So a small `Gtk.GestureClick` explicitly moves the cursor
//! to the click point on every secondary-button press, before rebuilding
//! the menu from the now-accurate position.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::gio;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::document::{self, Frontmatter};
use crate::i18n::tr;
use crate::media::{self, AltText};
use crate::{aialt, aicaption, preview};

/// Shows/hides an alt-text-length warning icon and sets its tooltip from
/// `media::alt_text_length_warning` - shared by the initial state when a
/// dialog opens and every subsequent edit.
fn update_alt_length_warning(icon: &gtk4::Image, text: &str) {
    match media::alt_text_length_warning(text) {
        Some(message) => {
            icon.set_tooltip_text(Some(&message));
            icon.set_visible(true);
        }
        None => icon.set_visible(false),
    }
}

/// Rebuilds the context-menu section (see `install`) from whether `line`
/// actually holds a media reference - empty when it doesn't, so the
/// section contributes no items (and no stray separator) to the popover.
fn rebuild_menu_for_line(menu: &gio::Menu, buffer: &sourceview5::Buffer, line: i32) {
    menu.remove_all();
    let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
    let Some(source) = image_source_on_line(&body, line) else { return };
    menu.append(Some(&tr("Alternativtext festlegen…")), Some("imagealt.set"));
    if document::media_reference_kind(&source) == document::MediaReferenceKind::Image {
        menu.append(Some(&tr("KI-Alternativtext generieren…")), Some("imagealt.generate-ai"));
        menu.append(Some(&tr("KI-Bildunterschrift generieren…")), Some("imagealt.generate-caption"));
    }
}

/// Moves the cursor to wherever a secondary click lands (see the module
/// docs for why this can't just rely on default `Gtk.TextView` behavior),
/// rebuilds the returned menu section from that now-accurate position, and
/// wires the `imagealt.set`/`imagealt.generate-ai` actions the section's
/// items invoke.
pub fn install(
    view: &sourceview5::View,
    buffer: &sourceview5::Buffer,
    frontmatter: Rc<RefCell<Frontmatter>>,
    current_path: Rc<RefCell<Option<PathBuf>>>,
    preview_pane: Rc<preview::PreviewPane>,
) -> gio::Menu {
    let menu = gio::Menu::new();

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3); // secondary/right button
    {
        let buffer = buffer.clone();
        let menu = menu.clone();
        let view_weak = view.downgrade();
        gesture.connect_pressed(move |_gesture, _n_press, x, y| {
            let Some(view) = view_weak.upgrade() else { return };
            let (buffer_x, buffer_y) = view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
            if let Some((iter, _trailing)) = view.iter_at_position(buffer_x, buffer_y) {
                buffer.place_cursor(&iter);
            }
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            rebuild_menu_for_line(&menu, &buffer, line);
        });
    }
    view.add_controller(gesture);

    let actions = gio::SimpleActionGroup::new();

    let set_action = gio::SimpleAction::new("set", None);
    {
        let buffer = buffer.clone();
        let frontmatter = frontmatter.clone();
        let preview_pane = preview_pane.clone();
        let view_weak = view.downgrade();
        set_action.connect_activate(move |_, _| {
            let Some(view) = view_weak.upgrade() else { return };
            let Some(window) = view.root().and_then(|root| root.downcast::<gtk4::Window>().ok()) else {
                return;
            };
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            open_for_line(&window, &buffer, &frontmatter, line, &preview_pane);
        });
    }
    actions.add_action(&set_action);

    let generate_ai_action = gio::SimpleAction::new("generate-ai", None);
    {
        let buffer = buffer.clone();
        let frontmatter = frontmatter.clone();
        let current_path = current_path.clone();
        let preview_pane = preview_pane.clone();
        let view_weak = view.downgrade();
        generate_ai_action.connect_activate(move |_, _| {
            let Some(view) = view_weak.upgrade() else { return };
            let Some(window) = view.root().and_then(|root| root.downcast::<gtk4::Window>().ok()) else {
                return;
            };
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            let doc_dir = current_path.borrow().as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            generate_ai_for_line(&window, &buffer, &frontmatter, line, doc_dir, &preview_pane);
        });
    }
    actions.add_action(&generate_ai_action);

    let generate_caption_action = gio::SimpleAction::new("generate-caption", None);
    {
        let buffer = buffer.clone();
        let frontmatter = frontmatter.clone();
        let current_path = current_path.clone();
        let preview_pane = preview_pane.clone();
        let view_weak = view.downgrade();
        generate_caption_action.connect_activate(move |_, _| {
            let Some(view) = view_weak.upgrade() else { return };
            let Some(window) = view.root().and_then(|root| root.downcast::<gtk4::Window>().ok()) else {
                return;
            };
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            let doc_dir = current_path.borrow().as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            generate_caption_for_line(&window, &buffer, &frontmatter, line, doc_dir, &preview_pane);
        });
    }
    actions.add_action(&generate_caption_action);

    view.insert_action_group("imagealt", Some(&actions));

    menu
}

/// Same image-on-line lookup + reconcile as `open_for_line`, but hands off
/// to `aialt::open`'s AI-generation review dialog instead of the plain
/// manual-entry one.
fn generate_ai_for_line(
    window: &gtk4::Window,
    buffer: &sourceview5::Buffer,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    line: i32,
    doc_dir: Option<PathBuf>,
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();

    // AI alt-text generation is vision-model-based and makes no sense for a
    // video/audio reference - `![]()` syntax is reused for all local media
    // (see `crates/gutenberg`'s `as_lone_media`), so a right-click here can
    // land on either.
    let is_image = |source: &String| document::media_reference_kind(source) == document::MediaReferenceKind::Image;
    let Some(source) = image_source_on_line(&body, line).filter(is_image) else {
        let alert = adw::AlertDialog::builder()
            .heading(tr("Keine Bildreferenz gefunden"))
            .body(tr("Für den KI-Alternativtext bitte mit der rechten Maustaste auf eine Zeile mit einem Bild (![Beschreibung](bild.png)) klicken."))
            .build();
        alert.add_response("ok", &tr("OK"));
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
    let title = frontmatter.borrow().media[index].filename.clone();

    let frontmatter = frontmatter.clone();
    let preview_pane = preview_pane.clone();
    aialt::open(window, title, source, doc_dir, move |text| {
        if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
            item.alt = if text.is_empty() { AltText::Empty } else { AltText::Text(text) };
        }
        preview_pane.refresh_media(&frontmatter.borrow().media);
    });
}

/// Same image-on-line lookup + reconcile as `generate_ai_for_line`, but
/// hands off to `aicaption::open` and writes the result into
/// `MediaItem.caption` instead of `.alt` - also, unlike alt text, pulls
/// `surrounding_context` from the body so the caption can be generated
/// with editorial awareness of the article, not the image in isolation.
fn generate_caption_for_line(
    window: &gtk4::Window,
    buffer: &sourceview5::Buffer,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    line: i32,
    doc_dir: Option<PathBuf>,
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();

    let is_image = |source: &String| document::media_reference_kind(source) == document::MediaReferenceKind::Image;
    let Some(source) = image_source_on_line(&body, line).filter(is_image) else {
        let alert = adw::AlertDialog::builder()
            .heading(tr("Keine Bildreferenz gefunden"))
            .body(tr("Für die KI-Bildunterschrift bitte mit der rechten Maustaste auf eine Zeile mit einem Bild (![Beschreibung](bild.png)) klicken."))
            .build();
        alert.add_response("ok", &tr("OK"));
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
    let title = frontmatter.borrow().media[index].filename.clone();
    let context = surrounding_context(&body, &source);

    let frontmatter = frontmatter.clone();
    let preview_pane = preview_pane.clone();
    aicaption::open(window, title, source, context, doc_dir, move |text| {
        if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
            item.caption = (!text.is_empty()).then_some(text);
        }
        preview_pane.refresh_media(&frontmatter.borrow().media);
    });
}

/// Plain-text context for the AI caption prompt (`aicaption.rs`): the
/// article text immediately surrounding the image whose reference is
/// `source` - the top-level block before it and the one after it, in
/// document order, joined by a blank line - not the whole article, so the
/// prompt stays focused on what the photo is actually illustrating rather
/// than drowning in unrelated sections. Formatting markup is dropped (only
/// text/inline-code content is kept) since this feeds a text prompt, not
/// another render. Empty if the reference can't be found, or there's no
/// block on either side (e.g. an image right at the very start or end of
/// the article) - `aicaption.rs` treats that as "no extra context", not an
/// error.
pub fn surrounding_context(markdown: &str, source: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<(Event, std::ops::Range<usize>)> = Parser::new_ext(markdown, options).into_offset_iter().collect();

    let mut blocks: Vec<(usize, usize, bool)> = Vec::new();
    let mut i = 0;
    while i < events.len() {
        match &events[i].0 {
            Event::Start(tag) => {
                let end_marker = tag.to_end();
                let end = find_matching_end(&events, i, &end_marker);
                let has_image = events[i..=end]
                    .iter()
                    .any(|(event, _)| matches!(event, Event::Start(Tag::Image { dest_url, .. }) if dest_url.as_ref() == source));
                blocks.push((i, end, has_image));
                i = end + 1;
            }
            Event::Rule => {
                blocks.push((i, i, false));
                i += 1;
            }
            _ => i += 1,
        }
    }

    let Some(target) = blocks.iter().position(|(_, _, has_image)| *has_image) else {
        return String::new();
    };

    let mut parts = Vec::new();
    if target > 0 {
        let (start, end, _) = blocks[target - 1];
        let text = block_plain_text(&events[start..=end]);
        if !text.is_empty() {
            parts.push(text);
        }
    }
    if target + 1 < blocks.len() {
        let (start, end, _) = blocks[target + 1];
        let text = block_plain_text(&events[start..=end]);
        if !text.is_empty() {
            parts.push(text);
        }
    }
    parts.join("\n\n")
}

/// Concatenates the visible text of a slice of events, dropping all markup -
/// `Event::Text`/`Event::Code` content only, with soft/hard breaks folded to
/// a single space, so multi-line source wraps into one flowing sentence.
fn block_plain_text(events: &[(Event, std::ops::Range<usize>)]) -> String {
    let mut out = String::new();
    for (event, _) in events {
        match event {
            Event::Text(text) | Event::Code(text) => out.push_str(text),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Same `Tag::to_end()` depth-counting trick `preview.rs`/`crates/gutenberg`
/// both already use - duplicated rather than shared, matching how those two
/// already keep their own private copies (see `preview.rs::find_matching_end`'s
/// doc comment).
fn find_matching_end(events: &[(Event, std::ops::Range<usize>)], start: usize, end_marker: &TagEnd) -> usize {
    let mut depth = 0usize;
    let mut j = start;
    while j < events.len() {
        match &events[j].0 {
            Event::Start(t) if &t.to_end() == end_marker => depth += 1,
            Event::End(e) if e == end_marker => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
        j += 1;
    }
    events.len().saturating_sub(1)
}

/// Finds the `![alt](source)` reference starting on `line` (0-indexed,
/// matching `Gtk.TextIter::line()`), reconciles the tracked media list
/// against the current body so the reference is guaranteed to have a
/// `MediaItem`, then delegates to `open_dialog_for_index` for that image's
/// own alt text and caption.
fn open_for_line(window: &gtk4::Window, buffer: &sourceview5::Buffer, frontmatter: &Rc<RefCell<Frontmatter>>, line: i32, preview_pane: &Rc<preview::PreviewPane>) {
    let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();

    let Some(source) = image_source_on_line(&body, line) else {
        let alert = adw::AlertDialog::builder()
            .heading(tr("Keine Bildreferenz gefunden"))
            .body(tr("Für den Alternativtext bitte mit der rechten Maustaste auf eine Zeile mit einem Bild (![Beschreibung](bild.png)) klicken."))
            .build();
        alert.add_response("ok", &tr("OK"));
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
    open_dialog_for_index(window, frontmatter, index, preview_pane);
}

/// Opens the manual alt-text/caption dialog for one already-known
/// `MediaItem` index - shared by `open_for_line` above (the editor
/// context-menu path, which first has to look up the index from a clicked
/// line) and the preview pane's own "Alternativtext bearbeiten…" context
/// menu item (`preview.rs`, which already knows the index from the
/// clicked image). Same fields and wiring as `mediapanel.rs`'s row, minus
/// the upload button, which isn't part of what a quick shortcut needs.
pub fn open_dialog_for_index(window: &gtk4::Window, frontmatter: &Rc<RefCell<Frontmatter>>, index: usize, preview_pane: &Rc<preview::PreviewPane>) {
    let Some(item) = frontmatter.borrow().media.get(index).cloned() else {
        return;
    };

    let alt_switch_row = adw::SwitchRow::builder()
        .title(tr("Alternativtext definieren"))
        .subtitle(tr("Aus lassen für rein dekorative Bilder - das ist kein Fehler"))
        .active(!item.alt.is_undefined())
        .build();

    let alt_entry_row = adw::EntryRow::builder().title(tr("Alternativtext")).build();
    if let AltText::Text(text) = &item.alt {
        alt_entry_row.set_text(text);
    }
    alt_entry_row.set_visible(alt_switch_row.is_active());

    // Non-blocking hint for an unusually long alt text (see
    // `media::alt_text_length_warning`) - only ever a suffix icon with a
    // tooltip, never something that stops the dialog from being closed, since
    // some images genuinely need a longer description.
    let alt_length_warning_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    alt_length_warning_icon.add_css_class("warning");
    alt_length_warning_icon.set_visible(false);
    alt_entry_row.add_suffix(&alt_length_warning_icon);

    {
        let frontmatter = frontmatter.clone();
        let alt_entry_row = alt_entry_row.clone();
        let alt_length_warning_icon = alt_length_warning_icon.clone();
        let preview_pane = preview_pane.clone();
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
            update_alt_length_warning(&alt_length_warning_icon, &alt_entry_row.text());
            preview_pane.refresh_media(&frontmatter.borrow().media);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let alt_switch_row = alt_switch_row.clone();
        let alt_length_warning_icon = alt_length_warning_icon.clone();
        let preview_pane = preview_pane.clone();
        alt_entry_row.connect_changed(move |row| {
            if !alt_switch_row.is_active() {
                return;
            }
            let text = row.text().to_string();
            update_alt_length_warning(&alt_length_warning_icon, &text);
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if text.is_empty() { AltText::Empty } else { AltText::Text(text) };
            }
            preview_pane.refresh_media(&frontmatter.borrow().media);
        });
    }
    update_alt_length_warning(&alt_length_warning_icon, &alt_entry_row.text());

    let caption_row = adw::EntryRow::builder().title(tr("Bildunterschrift")).text(item.caption.as_deref().unwrap_or("")).build();
    {
        let frontmatter = frontmatter.clone();
        let preview_pane = preview_pane.clone();
        caption_row.connect_changed(move |row| {
            let text = row.text().to_string();
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.caption = (!text.is_empty()).then_some(text);
            }
            preview_pane.refresh_media(&frontmatter.borrow().media);
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

    let dialog = adw::Dialog::builder().title(tr("Alternativtext")).content_width(420).content_height(340).child(&toolbar_view).build();
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

    #[test]
    fn surrounding_context_returns_the_paragraphs_before_and_after() {
        let markdown = "Intro text before the photo.\n\n![a cat](cat.png)\n\nOutro text after the photo.\n";
        assert_eq!(surrounding_context(markdown, "cat.png"), "Intro text before the photo.\n\nOutro text after the photo.");
    }

    #[test]
    fn surrounding_context_handles_an_image_with_no_block_before_it() {
        let markdown = "![a cat](cat.png)\n\nOutro text after the photo.\n";
        assert_eq!(surrounding_context(markdown, "cat.png"), "Outro text after the photo.");
    }

    #[test]
    fn surrounding_context_handles_an_image_with_no_block_after_it() {
        let markdown = "Intro text before the photo.\n\n![a cat](cat.png)\n";
        assert_eq!(surrounding_context(markdown, "cat.png"), "Intro text before the photo.");
    }

    #[test]
    fn surrounding_context_is_empty_when_the_source_is_not_found() {
        let markdown = "Intro.\n\n![a cat](cat.png)\n\nOutro.\n";
        assert_eq!(surrounding_context(markdown, "dog.png"), "");
    }

    #[test]
    fn surrounding_context_uses_a_heading_neighbor_as_context_too() {
        let markdown = "## Der Ausflug\n\n![a cat](cat.png)\n\nOutro text.\n";
        assert_eq!(surrounding_context(markdown, "cat.png"), "Der Ausflug\n\nOutro text.");
    }
}
