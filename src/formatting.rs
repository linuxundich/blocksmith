//! Markdown formatting toolbar: cut/copy/paste plus the most-used Markdown
//! constructs, each just inserting/wrapping plain text in the source
//! buffer - no rich-text state to keep in sync, since the buffer IS the
//! Markdown source.

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

/// Keyboard shortcuts for the formatting actions that aren't already
/// covered by GtkSourceView's own bindings (cut/copy/paste are).
pub fn install_shortcuts(view: &sourceview5::View, buffer: &sourceview5::Buffer) {
    let controller = gtk4::EventControllerKey::new();
    let buffer = buffer.clone();
    controller.connect_key_pressed(move |_, key, _, state| {
        if !state.contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        match key {
            gdk::Key::b => {
                wrap_selection(&buffer, "**", "**");
                glib::Propagation::Stop
            }
            gdk::Key::i => {
                wrap_selection(&buffer, "*", "*");
                glib::Propagation::Stop
            }
            gdk::Key::k => {
                insert_link(&buffer);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });
    view.add_controller(controller);
}

pub fn build(view: &sourceview5::View, buffer: &sourceview5::Buffer) -> gtk4::Box {
    let toolbar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    toolbar.append(&group(&[
        icon_button("edit-cut-symbolic", "Ausschneiden (Strg+X)", view, |v| v.emit_by_name::<()>("cut-clipboard", &[])),
        icon_button("edit-copy-symbolic", "Kopieren (Strg+C)", view, |v| v.emit_by_name::<()>("copy-clipboard", &[])),
        icon_button("edit-paste-symbolic", "Einfügen (Strg+V)", view, |v| v.emit_by_name::<()>("paste-clipboard", &[])),
    ]));

    toolbar.append(&group(&[
        icon_button("format-text-bold-symbolic", "Fett (Strg+B)", buffer, |b| wrap_selection(b, "**", "**")),
        icon_button("format-text-italic-symbolic", "Kursiv (Strg+I)", buffer, |b| wrap_selection(b, "*", "*")),
        icon_button("format-text-strikethrough-symbolic", "Durchgestrichen", buffer, |b| wrap_selection(b, "~~", "~~")),
    ]));

    toolbar.append(&group(&[
        label_button("H2", "Überschrift", buffer, |b| insert_line_prefix(b, "## ")),
        label_button("”", "Zitat", buffer, |b| insert_line_prefix(b, "> ")),
        label_button("</>", "Code", buffer, |b| wrap_selection(b, "`", "`")),
        label_button("{ }", "Codeblock", buffer, |b| insert_code_block(b)),
    ]));

    toolbar.append(&group(&[
        label_button("•", "Liste", buffer, |b| insert_line_prefix(b, "- ")),
        label_button("1.", "Nummerierte Liste", buffer, |b| insert_line_prefix(b, "1. ")),
    ]));

    toolbar.append(&group(&[icon_button("insert-link-symbolic", "Link einfügen (Strg+K)", buffer, |b| insert_link(b))]));

    toolbar
}

/// Visually joins a cluster of related buttons into one segmented control
/// (GNOME's standard "linked" style), instead of a flat row of separately
/// framed buttons.
fn group(buttons: &[gtk4::Button]) -> gtk4::Box {
    let group_box = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).build();
    group_box.add_css_class("linked");
    for button in buttons {
        group_box.append(button);
    }
    group_box
}

fn icon_button<T: Clone + 'static>(icon_name: &str, tooltip: &str, target: &T, action: impl Fn(&T) + 'static) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    let target = target.clone();
    button.connect_clicked(move |_| action(&target));
    button
}

fn label_button<T: Clone + 'static>(label: &str, tooltip: &str, target: &T, action: impl Fn(&T) + 'static) -> gtk4::Button {
    let button = gtk4::Button::with_label(label);
    button.set_tooltip_text(Some(tooltip));
    let target = target.clone();
    button.connect_clicked(move |_| action(&target));
    button
}

/// Wraps the current selection in `prefix`...`suffix`; with no selection,
/// inserts an empty `prefix``suffix` pair with the cursor placed between them.
fn wrap_selection(buffer: &sourceview5::Buffer, prefix: &str, suffix: &str) {
    if let Some((mut start, mut end)) = buffer.selection_bounds() {
        let selected = buffer.text(&start, &end, false).to_string();
        buffer.delete(&mut start, &mut end);
        let pos = start.offset();
        buffer.insert(&mut start, &format!("{prefix}{selected}{suffix}"));
        let inner_start = buffer.iter_at_offset(pos + prefix.chars().count() as i32);
        let inner_end = buffer.iter_at_offset(pos + prefix.chars().count() as i32 + selected.chars().count() as i32);
        buffer.select_range(&inner_end, &inner_start);
    } else {
        let mut iter = buffer.iter_at_mark(&buffer.get_insert());
        let pos = iter.offset();
        buffer.insert(&mut iter, &format!("{prefix}{suffix}"));
        let cursor = buffer.iter_at_offset(pos + prefix.chars().count() as i32);
        buffer.place_cursor(&cursor);
    }
}

/// Inserts `prefix` at the start of the line the cursor is currently on.
fn insert_line_prefix(buffer: &sourceview5::Buffer, prefix: &str) {
    let mut iter = buffer.iter_at_mark(&buffer.get_insert());
    iter.set_line_offset(0);
    buffer.insert(&mut iter, prefix);
}

fn insert_code_block(buffer: &sourceview5::Buffer) {
    if let Some((mut start, mut end)) = buffer.selection_bounds() {
        let selected = buffer.text(&start, &end, false).to_string();
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, &format!("```\n{selected}\n```"));
    } else {
        let mut iter = buffer.iter_at_mark(&buffer.get_insert());
        let pos = iter.offset();
        buffer.insert(&mut iter, "```\n\n```");
        let cursor = buffer.iter_at_offset(pos + 4); // right after "```\n"
        buffer.place_cursor(&cursor);
    }
}

/// Inserts a Markdown link, selecting the placeholder text (existing
/// selection becomes the link text, or "text"/"url" placeholders otherwise)
/// so the user can immediately type to replace it.
fn insert_link(buffer: &sourceview5::Buffer) {
    if let Some((mut start, mut end)) = buffer.selection_bounds() {
        let selected = buffer.text(&start, &end, false).to_string();
        buffer.delete(&mut start, &mut end);
        let pos = start.offset();
        buffer.insert(&mut start, &format!("[{selected}](url)"));
        let url_start = pos + 1 + selected.chars().count() as i32 + 2;
        select(buffer, url_start, url_start + 3);
    } else {
        let mut iter = buffer.iter_at_mark(&buffer.get_insert());
        let pos = iter.offset();
        buffer.insert(&mut iter, "[text](url)");
        select(buffer, pos + 1, pos + 5);
    }
}

fn select(buffer: &sourceview5::Buffer, start_offset: i32, end_offset: i32) {
    let start = buffer.iter_at_offset(start_offset);
    let end = buffer.iter_at_offset(end_offset);
    buffer.select_range(&end, &start);
}
