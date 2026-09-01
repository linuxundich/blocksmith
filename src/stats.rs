//! Document statistics shown in the right pane's "Statistik" tab, computed
//! straight from the Markdown source text.

use gtk4::prelude::*;

#[derive(Clone, Copy)]
pub struct Stats {
    pub words: usize,
    pub chars_with_spaces: usize,
    pub chars_without_spaces: usize,
    pub paragraphs: usize,
    pub reading_minutes: usize,
}

pub fn compute(markdown: &str) -> Stats {
    let words = markdown.split_whitespace().count();
    let paragraphs = markdown.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).count();
    let reading_minutes = if words == 0 { 0 } else { ((words as f64 / 200.0).ceil() as usize).max(1) };
    Stats {
        words,
        chars_with_spaces: markdown.chars().count(),
        chars_without_spaces: markdown.chars().filter(|c| !c.is_whitespace()).count(),
        paragraphs,
        reading_minutes,
    }
}

pub struct StatsView {
    pub widget: gtk4::Widget,
    words: gtk4::Label,
    chars_with_spaces: gtk4::Label,
    chars_without_spaces: gtk4::Label,
    paragraphs: gtk4::Label,
    reading_minutes: gtk4::Label,
}

impl StatsView {
    pub fn new() -> Self {
        let words = value_label();
        let chars_with_spaces = value_label();
        let chars_without_spaces = value_label();
        let paragraphs = value_label();
        let reading_minutes = value_label();

        let list = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
        list.append(&row("Wörter", &words));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row("Zeichen (mit Leerzeichen)", &chars_with_spaces));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row("Zeichen (ohne Leerzeichen)", &chars_without_spaces));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row("Absätze", &paragraphs));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row("Geschätzte Lesezeit", &reading_minutes));
        list.add_css_class("boxed-list");

        let clamp = adw::Clamp::builder().maximum_size(420).child(&list).build();
        let scroller = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hexpand(true)
            .vexpand(true)
            .margin_top(18)
            .margin_start(12)
            .margin_end(12)
            .build();

        Self {
            widget: scroller.upcast(),
            words,
            chars_with_spaces,
            chars_without_spaces,
            paragraphs,
            reading_minutes,
        }
    }

    pub fn update(&self, markdown: &str) {
        let stats = compute(markdown);
        self.words.set_label(&stats.words.to_string());
        self.chars_with_spaces.set_label(&stats.chars_with_spaces.to_string());
        self.chars_without_spaces.set_label(&stats.chars_without_spaces.to_string());
        self.paragraphs.set_label(&stats.paragraphs.to_string());
        self.reading_minutes.set_label(&format!("{} min", stats.reading_minutes));
    }
}

fn value_label() -> gtk4::Label {
    let label = gtk4::Label::new(Some("0"));
    label.add_css_class("dim-label");
    label
}

fn row(title: &str, value: &gtk4::Label) -> gtk4::Box {
    let title_label = gtk4::Label::builder().label(title).xalign(0.0).hexpand(true).build();
    let row_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();
    row_box.append(&title_label);
    row_box.append(value);
    row_box
}
