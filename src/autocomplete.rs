//! Attaches suggestion-popover autocomplete to a comma-separated `EntryRow`
//! (used for the categories/tags fields in `properties.rs`), suggesting
//! from a shared, asynchronously-populated term list fetched from
//! WordPress (see `spawn_term_fetch` below).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::{secrets, wpclient, wpsite};

const MAX_SUGGESTIONS: usize = 8;

/// Suggests terms matching the text after the last comma, replacing that
/// segment (keeping everything before it intact) when a suggestion is
/// picked - so categories/tags stay comma-separated as the user builds up
/// the list.
pub fn attach(entry: &adw::EntryRow, terms: Rc<RefCell<Vec<String>>>) {
    let popover = gtk4::Popover::builder().autohide(false).has_arrow(true).build();
    popover.set_parent(entry);

    let list_box = gtk4::ListBox::new();
    list_box.add_css_class("boxed-list");
    popover.set_child(Some(&list_box));

    let entry_for_activate = entry.clone();
    let popover_for_activate = popover.clone();
    list_box.connect_row_activated(move |_, row| {
        let Some(label) = row.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let suggestion = label.text().to_string();
        let current = entry_for_activate.text().to_string();
        let prefix = current.rfind(',').map(|idx| format!("{} ", &current[..=idx])).unwrap_or_default();
        let new_text = format!("{prefix}{suggestion}, ");
        entry_for_activate.set_text(&new_text);
        entry_for_activate.set_position(new_text.chars().count() as i32);
        popover_for_activate.popdown();
    });

    let list_box_for_changed = list_box.clone();
    let popover_for_changed = popover.clone();
    entry.connect_changed(move |row| {
        let text = row.text().to_string();
        let current_segment = text.rsplit(',').next().unwrap_or("").trim().to_lowercase();

        while let Some(child) = list_box_for_changed.first_child() {
            list_box_for_changed.remove(&child);
        }

        if current_segment.is_empty() {
            popover_for_changed.popdown();
            return;
        }

        let matches: Vec<String> = terms
            .borrow()
            .iter()
            .filter(|t| t.to_lowercase().contains(&current_segment) && t.to_lowercase() != current_segment)
            .take(MAX_SUGGESTIONS)
            .cloned()
            .collect();

        if matches.is_empty() {
            popover_for_changed.popdown();
            return;
        }

        for term in &matches {
            let label = gtk4::Label::builder()
                .label(term)
                .xalign(0.0)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(10)
                .margin_end(10)
                .build();
            let list_row = gtk4::ListBoxRow::new();
            list_row.set_child(Some(&label));
            list_box_for_changed.append(&list_row);
        }
        popover_for_changed.popup();
    });
}

/// Fetches existing term names for a taxonomy (`"categories"`/`"tags"`) from
/// the configured WordPress site on a background thread (see `wpclient`'s
/// module docs for why it's blocking), filling `terms` once done. A no-op if
/// no site is configured or the password isn't in the keyring - autocomplete
/// then just stays empty rather than erroring.
pub fn spawn_term_fetch(taxonomy: &'static str, terms: Rc<RefCell<Vec<String>>>) {
    let site = wpsite::load();
    if site.url.is_empty() {
        return;
    }

    let (tx, rx) = mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        let names = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .ok()
            .flatten()
            .and_then(|password| {
                let client = wpclient::Client::new(&site.url, &site.username, &password);
                client.list_term_names(taxonomy).ok()
            })
            .unwrap_or_default();
        let _ = tx.send(names);
    });

    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(names) => {
            *terms.borrow_mut() = names;
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });
}
