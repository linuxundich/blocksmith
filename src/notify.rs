//! Desktop notifications (`Gio::Notification`) for a background action -
//! publishing, an image upload, a link check - finishing while nobody is
//! looking at the window. Works under Flatpak via the notification portal
//! with no extra `finish-args` permission needed, since it goes through
//! `Gio::Application::send_notification` rather than talking to D-Bus
//! directly.
//!
//! Deliberately narrow: every one of these actions already shows its
//! result in an in-dialog status label/toast for someone watching, so a
//! system notification on top of that would just be noise. `send` only
//! actually notifies when the whole app has no focused window at all
//! (checked via `Gtk.Application::active_window()`), i.e. the user has
//! switched away entirely - not merely looking at a different tab of the
//! same dialog, which isn't worth the same distinction here.

use adw::prelude::*;
use gtk4::gio;

/// `id` lets a second notification for the same kind of event (e.g.
/// re-publishing) replace the previous one instead of piling up, per
/// `Gio::Application::send_notification`'s own semantics.
pub fn send(id: &str, title: &str, body: &str) {
    let Some(app) = gio::Application::default() else { return };
    if let Some(gtk_app) = app.downcast_ref::<gtk4::Application>() {
        if gtk_app.active_window().is_some() {
            return;
        }
    }
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    app.send_notification(Some(id), &notification);
}
