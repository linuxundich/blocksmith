//! The window's bottom bar: a compact, always-visible line of word-count
//! and reading-time statistics for the whole article, plus - whenever the
//! editor has an active selection - the same two numbers for just the
//! selected text. The full breakdown (character counts, paragraphs, ...)
//! stays in the "Statistik" tab (`stats.rs`); this is the at-a-glance
//! summary, always on screen regardless of which right-pane tab is open.

use adw::prelude::*;

use crate::stats;

pub struct StatusBar {
    pub widget: gtk4::Widget,
    label: gtk4::Label,
    document_stats: std::cell::Cell<stats::Stats>,
    selection_stats: std::cell::Cell<Option<stats::Stats>>,
}

impl StatusBar {
    pub fn new() -> Self {
        let label = gtk4::Label::builder().xalign(0.0).build();
        label.add_css_class("dim-label");
        label.add_css_class("caption");

        let widget = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).margin_top(4).margin_bottom(4).margin_start(12).margin_end(12).build();
        widget.append(&label);

        let bar = Self {
            widget: widget.upcast(),
            label,
            document_stats: std::cell::Cell::new(stats::compute("")),
            selection_stats: std::cell::Cell::new(None),
        };
        bar.render();
        bar
    }

    pub fn update_document(&self, markdown: &str) {
        self.document_stats.set(stats::compute(markdown));
        self.render();
    }

    pub fn update_selection(&self, selected_text: Option<&str>) {
        let non_empty = selected_text.map(str::trim).filter(|s| !s.is_empty());
        self.selection_stats.set(non_empty.map(stats::compute));
        self.render();
    }

    fn render(&self) {
        let doc = self.document_stats.get();
        let mut text = format!("{} Wörter · ≈ {} Min. Lesezeit", format_de(doc.words), doc.reading_minutes);
        if let Some(selection) = self.selection_stats.get() {
            text.push_str(&format!("  —  Auswahl: {} Wörter · ≈ {} Min.", format_de(selection.words), selection.reading_minutes));
        }
        self.label.set_label(&text);
    }
}

/// Formats a count with German-style `.` thousands separators (e.g.
/// `12345` -> `"12.345"`), matching the rest of the app's German UI text.
fn format_de(n: usize) -> String {
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_numbers_are_unformatted() {
        assert_eq!(format_de(0), "0");
        assert_eq!(format_de(42), "42");
        assert_eq!(format_de(999), "999");
    }

    #[test]
    fn thousands_separators_are_inserted() {
        assert_eq!(format_de(1234), "1.234");
        assert_eq!(format_de(12345), "12.345");
        assert_eq!(format_de(1234567), "1.234.567");
    }
}
