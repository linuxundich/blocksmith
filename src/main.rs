mod autocomplete;
mod connection;
mod document;
mod editor;
mod export;
mod formatting;
mod importer;
mod preview;
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
        let win = window::build(app);
        win.present();
    });

    app.run()
}
