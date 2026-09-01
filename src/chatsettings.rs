//! The "KI-Chat" page of the Einstellungen dialog: Gemini API key (a
//! secret, stored via `secrets.rs`), model id, and the editable/resettable
//! system prompt (persisted via `chatconfig.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::{chatconfig, secrets};

pub fn build_page() -> adw::PreferencesPage {
    let api_key_row = adw::PasswordEntryRow::builder().title("Gemini API-Key").build();
    let model_row = adw::EntryRow::builder().title("Modell").text(chatconfig::load_model().as_str()).build();

    let connection_save_button = gtk4::Button::from_icon_name("document-save-symbolic");
    connection_save_button.set_tooltip_text(Some("Speichern"));
    connection_save_button.add_css_class("flat");

    let connection_group = adw::PreferencesGroup::builder().title("Gemini-Verbindung").build();
    connection_group.set_description(Some("Der API-Key wird im Schlüsselbund gespeichert, nicht als Klartext."));
    connection_group.set_header_suffix(Some(&connection_save_button));
    connection_group.add(&api_key_row);
    connection_group.add(&model_row);

    let connection_status = gtk4::Label::builder().label("").xalign(0.0).build();
    connection_status.add_css_class("dim-label");
    connection_status.set_wrap(true);
    connection_status.set_visible(false);
    let connection_status_group = adw::PreferencesGroup::new();
    connection_status_group.add(&connection_status);

    {
        let api_key_row = api_key_row.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(Some(key)) = secrets::load_gemini_api_key().await {
                api_key_row.set_text(&key);
            }
        });
    }

    connection_save_button.connect_clicked(move |_| {
        if let Err(err) = chatconfig::save_model(&model_row.text()) {
            connection_status.set_label(&format!("Fehler beim Speichern des Modells: {err}"));
            connection_status.set_visible(true);
            return;
        }
        let key = api_key_row.text().to_string();
        let connection_status = connection_status.clone();
        glib::MainContext::default().spawn_local(async move {
            match secrets::store_gemini_api_key(&key).await {
                Ok(()) => {
                    connection_status.set_label("Gespeichert.");
                    connection_status.set_visible(true);
                }
                Err(err) => {
                    connection_status.set_label(&format!("Fehler beim Speichern des API-Keys: {err}"));
                    connection_status.set_visible(true);
                }
            }
        });
    });

    let prompt_buffer = gtk4::TextBuffer::new(None::<&gtk4::TextTagTable>);
    prompt_buffer.set_text(&chatconfig::load_system_prompt());
    let prompt_view = gtk4::TextView::builder()
        .buffer(&prompt_buffer)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    let prompt_scroller = gtk4::ScrolledWindow::builder().child(&prompt_view).vexpand(true).min_content_height(320).build();
    prompt_scroller.add_css_class("card");

    let reset_button = gtk4::Button::with_label("Auf Standard zurücksetzen");
    reset_button.set_sensitive(chatconfig::is_system_prompt_customized());

    let prompt_status = gtk4::Label::new(None);
    prompt_status.add_css_class("dim-label");
    prompt_status.set_xalign(0.0);
    prompt_status.set_visible(false);

    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    // Suppresses the auto-save handler for exactly one `changed` signal -
    // set right before a programmatic `set_text` (the reset button below),
    // so resetting doesn't immediately re-save the default text as a new
    // "customized" prompt.
    let suppress_autosave: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    {
        let debounce = debounce.clone();
        let prompt_status = prompt_status.clone();
        let reset_button = reset_button.clone();
        let suppress_autosave = suppress_autosave.clone();
        prompt_buffer.connect_changed(move |buf| {
            if suppress_autosave.replace(false) {
                return;
            }
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
            let debounce_inner = debounce.clone();
            let prompt_status = prompt_status.clone();
            let reset_button = reset_button.clone();
            let id = glib::timeout_add_local(Duration::from_millis(500), move || {
                match chatconfig::save_system_prompt(&text) {
                    Ok(()) => prompt_status.set_label("Gespeichert."),
                    Err(err) => prompt_status.set_label(&format!("Fehler beim Speichern: {err}")),
                }
                prompt_status.set_visible(true);
                reset_button.set_sensitive(chatconfig::is_system_prompt_customized());
                *debounce_inner.borrow_mut() = None;
                glib::ControlFlow::Break
            });
            *debounce.borrow_mut() = Some(id);
        });
    }

    {
        let prompt_buffer = prompt_buffer.clone();
        let prompt_status = prompt_status.clone();
        let reset_button_for_click = reset_button.clone();
        reset_button.connect_clicked(move |_| {
            if let Err(err) = chatconfig::reset_system_prompt() {
                prompt_status.set_label(&format!("Fehler beim Zurücksetzen: {err}"));
                prompt_status.set_visible(true);
                return;
            }
            *suppress_autosave.borrow_mut() = true;
            prompt_buffer.set_text(&chatconfig::load_system_prompt());
            prompt_status.set_label("Auf Standard zurückgesetzt.");
            prompt_status.set_visible(true);
            reset_button_for_click.set_sensitive(false);
        });
    }

    let prompt_label = gtk4::Label::builder().label("Systemprompt").xalign(0.0).hexpand(true).build();
    prompt_label.add_css_class("heading");

    let prompt_header_row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(6).build();
    prompt_header_row.append(&prompt_label);
    prompt_header_row.append(&reset_button);

    let prompt_box = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(8).margin_top(12).build();
    prompt_box.append(&prompt_header_row);
    prompt_box.append(&prompt_scroller);
    prompt_box.append(&prompt_status);

    let prompt_group = adw::PreferencesGroup::new();
    prompt_group.add(&prompt_box);

    let page = adw::PreferencesPage::builder().title("KI-Chat").icon_name("chat-symbolic").build();
    page.add(&connection_group);
    page.add(&connection_status_group);
    page.add(&prompt_group);
    page
}
