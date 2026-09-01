//! "Bestehenden Artikel verlinken" dialog: pick one of the site's existing
//! posts by title and insert a real Markdown link to it. A scoped-down
//! alternative to live-typing autocomplete inside `[text](url)` (which would
//! need real `GtkSourceView` completion-provider integration to detect
//! "the cursor is inside a link destination" - a much bigger feature) that
//! still makes internal linking easy without copying a URL by hand first.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::{formatting, secrets, wpclient, wpsite};

pub fn open(parent: &adw::ApplicationWindow, buffer: &sourceview5::Buffer) {
    let site = wpsite::load();

    let status_label = gtk4::Label::new(None);
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Artikel suchen…"));

    let list_box = gtk4::ListBox::new();
    list_box.add_css_class("boxed-list");
    let posts: Rc<RefCell<Vec<wpclient::PostSummary>>> = Rc::new(RefCell::new(Vec::new()));

    let list_scroller = gtk4::ScrolledWindow::builder().child(&list_box).vexpand(true).min_content_height(360).build();

    let header = adw::HeaderBar::new();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content_box.append(&search_entry);
    content_box.append(&status_label);
    content_box.append(&list_scroller);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content_box));

    let dialog = adw::Dialog::builder()
        .title("Artikel verlinken")
        .content_width(480)
        .content_height(520)
        .child(&toolbar_view)
        .build();

    if site.url.is_empty() {
        status_label.set_label("Keine WordPress-Verbindung eingerichtet - bitte zuerst in den Einstellungen konfigurieren.");
        dialog.present(Some(parent));
        return;
    }

    // Filtering only ever hides rows, it never reorders or removes them, so
    // `row.index()` (used below to look the activated row back up in
    // `posts`) stays valid regardless of the current search text.
    //
    // `Adw.ActionRow` extends `Gtk.ListBoxRow` directly rather than sitting
    // inside a separate implicit wrapper row, so the row handed to the
    // filter function IS the action row itself (via its `ListBoxRow`
    // superclass) - downcast the row, not its child.
    {
        let search_entry_for_filter = search_entry.clone();
        list_box.set_filter_func(move |row| {
            let query = search_entry_for_filter.text().to_lowercase();
            if query.is_empty() {
                return true;
            }
            row.downcast_ref::<adw::ActionRow>()
                .map(|action_row| action_row.title().to_lowercase().contains(&query))
                .unwrap_or(true)
        });
    }
    {
        let list_box = list_box.clone();
        search_entry.connect_search_changed(move |_| list_box.invalidate_filter());
    }

    status_label.set_label("Lade Artikel …");
    let (tx, rx) = mpsc::channel::<Result<Vec<wpclient::PostSummary>, String>>();
    std::thread::spawn(move || {
        let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .map_err(|err| err.to_string())
            .and_then(|maybe_password| {
                maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
            })
            .and_then(|password| wpclient::Client::new(&site.url, &site.username, &password).list_posts().map_err(|err| err.to_string()));
        let _ = tx.send(outcome);
    });

    {
        let list_box = list_box.clone();
        let posts = posts.clone();
        let status_label = status_label.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok(fetched)) => {
                for post in &fetched {
                    let date = post.date.split('T').next().unwrap_or(&post.date);
                    let row = adw::ActionRow::builder()
                        .title(glib::markup_escape_text(&post.title).as_str())
                        .subtitle(date)
                        .activatable(true)
                        .build();
                    list_box.append(&row);
                }
                status_label.set_label(&format!("{} Artikel gefunden.", fetched.len()));
                *posts.borrow_mut() = fetched;
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                status_label.set_label(&format!("Fehler beim Laden: {err}"));
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                status_label.set_label("Interner Fehler: Lade-Thread hat kein Ergebnis geliefert.");
                glib::ControlFlow::Break
            }
        });
    }

    {
        let posts = posts.clone();
        let buffer = buffer.clone();
        let dialog_weak = dialog.downgrade();
        list_box.connect_row_activated(move |_list_box, row| {
            let Some(post) = posts.borrow().get(row.index() as usize).cloned() else {
                return;
            };
            formatting::insert_existing_link(&buffer, &post.title, &post.link);
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });
    }

    dialog.present(Some(parent));
}
