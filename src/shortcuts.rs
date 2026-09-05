//! "Tastenkürzel" - this app's keyboard shortcuts in a native
//! `Gtk.ShortcutsWindow`. Wired up via `Gtk.ApplicationWindow`'s standard
//! `help-overlay` property (`window.rs`), so GTK itself provides the
//! conventional `win.show-help-overlay` action - no bespoke action or
//! `open()` function needed here, just the window to hand it.

pub fn build() -> gtk4::ShortcutsWindow {
    let window = gtk4::ShortcutsWindow::builder().modal(true).build();

    let section = gtk4::ShortcutsSection::builder().section_name("main").build();
    section.add_group(&group(
        "Allgemein",
        &[
            ("Neuer Artikel", "<Ctrl>N"),
            ("Öffnen", "<Ctrl>O"),
            ("Von WordPress öffnen", "<Ctrl><Shift>O"),
            ("Speichern", "<Ctrl>S"),
            ("Artikel exportieren", "<Ctrl><Shift>P"),
            ("Medienverwaltung", "<Ctrl><Shift>M"),
            ("Einstellungen", "<Ctrl>comma"),
        ],
    ));
    section.add_group(&group(
        "Editor",
        &[
            ("Fett", "<Ctrl>B"),
            ("Kursiv", "<Ctrl>I"),
            ("Link einfügen", "<Ctrl>K"),
            ("Bild aus der Zwischenablage einfügen", "<Ctrl>V"),
            ("Suchen und Ersetzen", "<Ctrl>F"),
        ],
    ));
    window.add_section(&section);
    window
}

fn group(title: &str, shortcuts: &[(&str, &str)]) -> gtk4::ShortcutsGroup {
    let group = gtk4::ShortcutsGroup::builder().title(title).build();
    for (title, accelerator) in shortcuts {
        group.add_shortcut(&gtk4::ShortcutsShortcut::builder().title(*title).accelerator(*accelerator).build());
    }
    group
}
