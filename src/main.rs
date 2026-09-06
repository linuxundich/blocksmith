mod about;
mod aialt;
mod aimenu;
mod aiprompts;
mod appearance;
mod autocomplete;
mod autosave;
mod changelog;
mod chat;
mod chatconfig;
mod chatsettings;
mod codeview;
mod connection;
mod default_prompt;
mod document;
mod editor;
mod export;
mod fontutil;
mod formatting;
mod i18n;
mod imagealt;
mod imagecompress;
mod importer;
mod linkcheck;
mod linkpicker;
mod llm;
mod mdpango;
mod media;
mod mediapanel;
mod notify;
mod preview;
mod promptsettings;
mod properties;
mod recentfiles;
mod searchbar;
mod secrets;
mod settings;
mod shortcuts;
mod stats;
mod statusbar;
mod taxonomy;
mod termcache;
mod window;
mod windowstate;
mod wpclient;
mod wpsite;

use adw::prelude::*;
use gtk4::{gio, glib};

const APP_ID: &str = "de.christophlangner.Blocksmith";

fn main() -> glib::ExitCode {
    // Must run before anything else - `setlocale` (which this calls) isn't
    // thread-safe against locale-dependent calls running concurrently on
    // other threads, and nothing here has spawned any yet.
    i18n::init();

    // HANDLES_OPEN: the `.desktop` file declares `MimeType=text/markdown;`,
    // so double-clicking a `.md` file (or "Open With" → Blocksmith) in
    // Nautilus launches with a file argument - without this flag GTK
    // refuses that outright ("This application can not open files").
    let app = adw::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_OPEN).build();

    app.set_accels_for_action("win.new", &["<Ctrl>n"]);
    app.set_accels_for_action("win.open", &["<Ctrl>o"]);
    app.set_accels_for_action("win.open-from-wordpress", &["<Ctrl><Shift>o"]);
    app.set_accels_for_action("win.save", &["<Ctrl>s"]);
    app.set_accels_for_action("win.settings", &["<Ctrl>comma"]);
    app.set_accels_for_action("win.publish", &["<Ctrl><Shift>p"]);
    app.set_accels_for_action("win.media-manager", &["<Ctrl><Shift>m"]);
    app.set_accels_for_action("win.find", &["<Ctrl>f"]);
    app.set_accels_for_action("win.toggle-focus-mode", &["<Ctrl><Shift>f"]);
    app.set_accels_for_action("win.show-help-overlay", &["<Ctrl>question"]);

    app.connect_activate(|app| {
        appearance::apply_saved_color_scheme();
        load_chat_bubble_css();
        let win = window::build(app, None);
        win.present();
    });

    // Single-window app (see ROADMAP.md's "Deliberately not recommended" -
    // multi-window is a poor fit here), so a file opened while an instance
    // is already running loads into that same window via the `open-path`
    // action (`window.rs::wire_open_path_action`) rather than spawning a
    // second one; only the very first launch-with-a-file builds the window
    // itself with the path preloaded.
    app.connect_open(|app, files, _hint| {
        let Some(path) = files.first().and_then(gtk4::gio::File::path) else {
            return;
        };
        if let Some(win) = app.windows().into_iter().next() {
            win.activate_action("win.open-path", Some(&path.display().to_string().to_variant())).ok();
            win.present();
            return;
        }
        appearance::apply_saved_color_scheme();
        load_chat_bubble_css();
        let win = window::build(app, Some(path));
        win.present();
    });

    app.run()
}

/// Chat bubble colors use libadwaita's named theme colors so they adapt to
/// light/dark mode automatically, rather than hardcoding colors that would
/// only look right in one theme.
fn load_chat_bubble_css() {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(
        "
        .chat-bubble { padding: 8px 12px; border-radius: 12px; }
        .chat-bubble-user { background-color: @accent_bg_color; color: @accent_fg_color; }
        .chat-bubble-model { background-color: alpha(currentColor, 0.08); }
        ",
    );
    gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
}
