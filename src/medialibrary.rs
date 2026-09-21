//! "Aus Mediathek wählen…" dialog: browse the site's existing WordPress
//! media library and pick an already-uploaded image instead of uploading a
//! local file again. Same shape as `linkpicker.rs`'s "Artikel verlinken"
//! picker (search entry + list + open-time fetch), just over
//! `/wp-json/wp/v2/media` instead of `/posts` - the one real difference is
//! that a media library can run into the thousands of items, so the search
//! entry re-queries the server (`Client::list_media`'s own `search`
//! parameter) instead of just filtering the handful of already-fetched
//! rows client-side the way `linkpicker.rs` gets away with for a site's
//! (much shorter) post list.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::i18n::tr;
use crate::{secrets, wpclient, wpsite};

/// How long to wait after the last keystroke in the search box before
/// actually re-querying the server - longer than the live-preview
/// debounce (`window.rs::DEBOUNCE_MS`), since this is a real network round
/// trip, not a local re-render.
const SEARCH_DEBOUNCE_MS: u64 = 400;

/// Opens the picker; `on_selected` fires once, with the chosen item, when
/// the user activates a row - the caller decides what "picked" means
/// (insert into the editor body, or use as the featured image), the same
/// "caller decides where the result goes" shape `aialt::open`'s own
/// `on_apply` callback already uses.
pub fn open(parent: &gtk4::Window, on_selected: impl Fn(wpclient::WpMediaItem) + 'static) {
    let site = wpsite::load();

    let status_label = gtk4::Label::new(None);
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(&tr("Bilder suchen…")));

    let list_box = gtk4::ListBox::new();
    list_box.add_css_class("boxed-list");
    let items: Rc<RefCell<Vec<wpclient::WpMediaItem>>> = Rc::new(RefCell::new(Vec::new()));

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
        .title(tr("Aus Mediathek wählen"))
        .content_width(480)
        .content_height(520)
        .child(&toolbar_view)
        .build();

    if site.url.is_empty() {
        status_label.set_label(&tr("Keine WordPress-Verbindung eingerichtet - bitte zuerst in den Einstellungen konfigurieren."));
        dialog.present(Some(parent));
        return;
    }

    {
        let items = items.clone();
        let list_box = list_box.clone();
        let dialog_weak = dialog.downgrade();
        list_box.connect_row_activated(move |_list_box, row| {
            let Some(item) = items.borrow().get(row.index() as usize).cloned() else {
                return;
            };
            on_selected(item);
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });
    }

    spawn_fetch(site.clone(), None, list_box.clone(), items.clone(), status_label.clone());

    {
        let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let list_box = list_box.clone();
        let items = items.clone();
        let status_label = status_label.clone();
        search_entry.connect_search_changed(move |entry| {
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            let site = site.clone();
            let query = entry.text().to_string();
            let list_box = list_box.clone();
            let items = items.clone();
            let status_label = status_label.clone();
            let debounce_inner = debounce.clone();
            let id = glib::timeout_add_local(Duration::from_millis(SEARCH_DEBOUNCE_MS), move || {
                spawn_fetch(site.clone(), (!query.trim().is_empty()).then(|| query.clone()), list_box.clone(), items.clone(), status_label.clone());
                *debounce_inner.borrow_mut() = None;
                glib::ControlFlow::Break
            });
            *debounce.borrow_mut() = Some(id);
        });
    }

    dialog.present(Some(parent));
}

/// Fetches `search` (`None` for the initial, unfiltered load) and replaces
/// `list_box`'s rows with the result - shared by the dialog's initial load
/// and every debounced re-query the search entry triggers.
fn spawn_fetch(site: wpsite::SiteConfig, search: Option<String>, list_box: gtk4::ListBox, items: Rc<RefCell<Vec<wpclient::WpMediaItem>>>, status_label: gtk4::Label) {
    status_label.set_label(&tr("Lade Bilder …"));

    let (tx, rx) = mpsc::channel::<Result<Vec<wpclient::WpMediaItem>, String>>();
    std::thread::spawn(move || {
        let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .map_err(|err| err.to_string())
            .and_then(|maybe_password| maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden.")))
            .and_then(|password| wpclient::Client::new(&site.url, &site.username, &password).list_media(search.as_deref()).map_err(|err| err.to_string()));
        let _ = tx.send(outcome);
    });

    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(fetched)) => {
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }
            for item in &fetched {
                let title = if item.title.trim().is_empty() { item.source_url.clone() } else { item.title.clone() };
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&title).as_str())
                    .subtitle(glib::markup_escape_text(&item.source_url).as_str())
                    .activatable(true)
                    .build();
                list_box.append(&row);
            }
            status_label.set_label(&if fetched.is_empty() {
                tr("Keine Bilder gefunden.")
            } else {
                tr("{n} Bilder gefunden.").replace("{n}", &fetched.len().to_string())
            });
            *items.borrow_mut() = fetched;
            glib::ControlFlow::Break
        }
        Ok(Err(err)) => {
            status_label.set_label(&tr("Fehler beim Laden: {err}").replace("{err}", &err));
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            status_label.set_label(&tr("Interner Fehler: Lade-Thread hat kein Ergebnis geliefert."));
            glib::ControlFlow::Break
        }
    });
}
