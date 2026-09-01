mod aimenu;
mod aiprompts;
mod appearance;
mod autocomplete;
mod chat;
mod chatconfig;
mod chatsettings;
mod codeview;
mod connection;
mod default_prompt;
mod document;
mod editor;
mod export;
mod formatting;
mod importer;
mod llm;
mod mdpango;
mod preview;
mod promptsettings;
mod properties;
mod secrets;
mod settings;
mod stats;
mod termcache;
mod window;
mod wpclient;
mod wpsite;

use adw::prelude::*;
use gtk4::glib;

const APP_ID: &str = "de.christophlangner.Blocksmith";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.set_accels_for_action("win.new", &["<Ctrl>n"]);
    app.set_accels_for_action("win.open", &["<Ctrl>o"]);
    app.set_accels_for_action("win.open-from-wordpress", &["<Ctrl><Shift>o"]);
    app.set_accels_for_action("win.save", &["<Ctrl>s"]);
    app.set_accels_for_action("win.settings", &["<Ctrl>comma"]);
    app.set_accels_for_action("win.publish", &["<Ctrl><Shift>p"]);

    app.connect_activate(|app| {
        appearance::apply_saved_color_scheme();
        load_chat_bubble_css();
        let win = window::build(app);
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
