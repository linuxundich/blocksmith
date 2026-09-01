mod document;
mod editor;
mod preview;
mod window;

use adw::prelude::*;
use gtk4::glib;

const APP_ID: &str = "de.christophlangner.Blocksmith";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.set_accels_for_action("win.new", &["<Ctrl>n"]);
    app.set_accels_for_action("win.open", &["<Ctrl>o"]);
    app.set_accels_for_action("win.save", &["<Ctrl>s"]);

    app.connect_activate(|app| {
        let win = window::build(app);
        win.present();
    });

    app.run()
}
