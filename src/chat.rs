//! The "Chat" tab: a Gemini-backed chat pane with message bubbles. Sends
//! run on a background thread (see `gemini.rs`'s module docs for why),
//! polled back via the same thread+mpsc-channel+`glib::timeout_add_local`
//! pattern used throughout this app's WordPress-facing dialogs.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::{chatconfig, gemini, secrets};

pub struct ChatView {
    pub widget: gtk4::Widget,
}

impl ChatView {
    pub fn new() -> Self {
        let messages_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let scroller = gtk4::ScrolledWindow::builder().child(&messages_box).vexpand(true).build();

        let entry = gtk4::Entry::builder().placeholder_text("Nachricht an Gemini …").hexpand(true).build();
        let send_button = gtk4::Button::from_icon_name("mail-send-symbolic");
        send_button.add_css_class("suggested-action");
        send_button.set_tooltip_text(Some("Senden (Enter)"));

        let status_label = gtk4::Label::new(None);
        status_label.add_css_class("dim-label");
        status_label.set_xalign(0.0);
        status_label.set_margin_start(6);
        status_label.set_visible(false);

        let input_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        input_row.append(&entry);
        input_row.append(&send_button);

        let root = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
        root.append(&scroller);
        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        root.append(&status_label);
        root.append(&input_row);

        let history: Rc<RefCell<Vec<gemini::ChatMessage>>> = Rc::new(RefCell::new(Vec::new()));

        let send: Rc<dyn Fn()> = {
            let entry = entry.clone();
            let send_button = send_button.clone();
            let messages_box = messages_box.clone();
            let scroller = scroller.clone();
            let status_label = status_label.clone();
            let history = history.clone();
            Rc::new(move || {
                let text = entry.text().trim().to_string();
                if text.is_empty() {
                    return;
                }
                entry.set_text("");
                append_bubble(&messages_box, &text, true);
                scroll_to_bottom(&scroller);
                history.borrow_mut().push(gemini::ChatMessage {
                    role: gemini::Role::User,
                    text,
                });

                entry.set_sensitive(false);
                send_button.set_sensitive(false);
                status_label.set_label("Gemini denkt nach …");
                status_label.set_visible(true);

                let model = chatconfig::load_model();
                let system_prompt = chatconfig::load_system_prompt();
                let history_snapshot = history.borrow().clone();

                let (tx, rx) = mpsc::channel::<Result<String, String>>();
                std::thread::spawn(move || {
                    let outcome = futures_lite::future::block_on(secrets::load_gemini_api_key())
                        .map_err(|err| err.to_string())
                        .and_then(|maybe_key| {
                            maybe_key.ok_or_else(|| "Kein Gemini API-Key in den Einstellungen hinterlegt.".to_string())
                        })
                        .and_then(|key| {
                            gemini::Client::new(&key, &model).send(&system_prompt, &history_snapshot).map_err(|err| err.to_string())
                        });
                    let _ = tx.send(outcome);
                });

                let entry = entry.clone();
                let send_button = send_button.clone();
                let messages_box = messages_box.clone();
                let scroller = scroller.clone();
                let status_label = status_label.clone();
                let history = history.clone();
                glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                    Ok(Ok(reply)) => {
                        append_bubble(&messages_box, &reply, false);
                        scroll_to_bottom(&scroller);
                        history.borrow_mut().push(gemini::ChatMessage {
                            role: gemini::Role::Model,
                            text: reply,
                        });
                        status_label.set_visible(false);
                        entry.set_sensitive(true);
                        send_button.set_sensitive(true);
                        entry.grab_focus();
                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        status_label.set_label(&format!("Fehler: {err}"));
                        entry.set_sensitive(true);
                        send_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status_label.set_label("Interner Fehler: Chat-Thread hat kein Ergebnis geliefert.");
                        entry.set_sensitive(true);
                        send_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
            })
        };

        {
            let send = send.clone();
            entry.connect_activate(move |_| send());
        }
        {
            let send = send.clone();
            send_button.connect_clicked(move |_| send());
        }

        Self { widget: root.upcast() }
    }
}

fn append_bubble(messages_box: &gtk4::Box, text: &str, is_user: bool) {
    let label = gtk4::Label::builder().label(text).wrap(true).xalign(0.0).selectable(true).max_width_chars(48).build();

    let bubble = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    bubble.append(&label);
    bubble.add_css_class("chat-bubble");
    bubble.add_css_class(if is_user { "chat-bubble-user" } else { "chat-bubble-model" });

    let row = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).build();
    row.set_halign(if is_user { gtk4::Align::End } else { gtk4::Align::Start });
    row.append(&bubble);

    messages_box.append(&row);
}

fn scroll_to_bottom(scroller: &gtk4::ScrolledWindow) {
    let adjustment = scroller.vadjustment();
    glib::idle_add_local_once(move || {
        adjustment.set_value(adjustment.upper());
    });
}
