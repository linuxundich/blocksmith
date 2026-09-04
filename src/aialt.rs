//! AI-generated alt text: right-click an image - its `![alt](src)` line in
//! the editor (see `imagealt.rs`) or the rendered image itself in the
//! Vorschau pane (see `preview.rs`) - to have the active KI-Chat provider
//! look at the real image bytes and propose an accessible alt text, at one
//! of three levels of detail. The result is shown for review/correction
//! before anything is written, and applying it goes straight into the same
//! `MediaItem.alt` field the manual editor and Medienverwaltung already
//! use - so it's what the WordPress media upload sends, no separate step.
//!
//! Deliberately no attempt to hide this behind "is an LLM actually
//! configured" - matching `imagealt.rs`'s own choice to always show its
//! menu item and explain rather than silently do nothing: a missing/invalid
//! API key surfaces as a normal inline error from the same generation call
//! chat.rs already makes, not as a pre-check.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::document::Frontmatter;
use crate::media::AltText;
use crate::{chatconfig, export, llm, secrets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailLevel {
    Standard,
    Detailed,
    Precise,
}

impl DetailLevel {
    const ALL: [DetailLevel; 3] = [DetailLevel::Standard, DetailLevel::Detailed, DetailLevel::Precise];

    fn label(&self) -> &'static str {
        match self {
            DetailLevel::Standard => "Standard (kurz & bündig)",
            DetailLevel::Detailed => "Ausführlich",
            DetailLevel::Precise => "Hohe Genauigkeit",
        }
    }

    fn prompt(&self) -> &'static str {
        match self {
            DetailLevel::Standard => {
                "Du erstellst einen barrierefreien Alternativtext (alt-Text) für ein Bild in einem \
                 Blogartikel. Beschreibe in maximal einem kurzen Satz (ca. 8-12 Wörter), was auf dem \
                 Bild zu sehen ist, sodass eine Person mit Bildschirmleser den Inhalt versteht. Keine \
                 Einleitung wie \"Bild von\" oder \"Ein Foto zeigt\", keine Anführungszeichen, keine \
                 Interpretation - nur der reine Alternativtext."
            }
            DetailLevel::Detailed => {
                "Du erstellst einen barrierefreien Alternativtext (alt-Text) für ein Bild in einem \
                 Blogartikel. Beschreibe in 2-3 vollständigen Sätzen, was auf dem Bild zu sehen ist - \
                 Hauptmotiv, relevanter Kontext und Umgebung, soweit erkennbar - sodass sich eine \
                 Person mit Bildschirmleser ein klares Bild machen kann. Keine Einleitung wie \"Bild \
                 von\" oder \"Ein Foto zeigt\", keine Anführungszeichen, keine Spekulation über nicht \
                 erkennbare Dinge - nur der reine Alternativtext."
            }
            DetailLevel::Precise => {
                "Du erstellst einen barrierefreien Alternativtext (alt-Text) für ein Bild in einem \
                 Blogartikel. Beschreibe möglichst genau und sachlich alle relevanten visuellen \
                 Details: Objekte, ihre Anordnung, Farben, sichtbaren Text (wortwörtlich, falls \
                 vorhanden), sowie Zahlen/Werte, falls es sich um eine Grafik oder ein Diagramm \
                 handelt. Beschreibe Personen nur anhand äußerlich erkennbarer Merkmale, ohne \
                 Vermutungen zu Identität anzustellen. Keine Einleitung wie \"Bild von\" oder \"Ein \
                 Foto zeigt\", keine Anführungszeichen - nur der reine Alternativtext."
            }
        }
    }
}

/// Opens the review dialog for `frontmatter.media[index]` - the caller is
/// responsible for having already reconciled the media list against the
/// current document body (see `imagealt.rs`/`preview.rs`), so `index` is
/// guaranteed valid at the moment this is called.
pub fn open(window: &gtk4::Window, frontmatter: Rc<RefCell<Frontmatter>>, index: usize, doc_dir: Option<PathBuf>) {
    let Some(item) = frontmatter.borrow().media.get(index).cloned() else { return };

    let level_labels: Vec<&str> = DetailLevel::ALL.iter().map(DetailLevel::label).collect();
    let level_row = adw::ComboRow::builder().title("Detailgrad").model(&gtk4::StringList::new(&level_labels)).build();

    let generate_button = gtk4::Button::with_label("Text generieren");
    generate_button.add_css_class("suggested-action");

    let status_label = gtk4::Label::builder().wrap(true).xalign(0.0).build();
    status_label.set_visible(false);

    let text_view = gtk4::TextView::builder()
        .wrap_mode(gtk4::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    let text_buffer = text_view.buffer();
    text_buffer.set_text("Noch kein Text generiert - auf „Text generieren“ klicken.");

    let frame = gtk4::Frame::new(None);
    frame.set_child(Some(&text_view));
    let text_scroller = gtk4::ScrolledWindow::builder().child(&frame).min_content_height(120).vexpand(true).build();

    let generate_row_box = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(8).build();
    generate_row_box.append(&generate_button);

    let group = adw::PreferencesGroup::builder().title(&item.filename).build();
    group.add(&level_row);

    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(&group);
    content.append(&generate_row_box);
    content.append(&status_label);
    content.append(&text_scroller);

    let apply_button = gtk4::Button::with_label("Übernehmen");
    apply_button.add_css_class("suggested-action");

    let header = adw::HeaderBar::new();
    header.pack_end(&apply_button);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));

    let dialog = adw::Dialog::builder().title("KI-Alternativtext").content_width(460).content_height(420).child(&toolbar_view).build();

    {
        let source = item.source.clone();
        let filename = item.filename.clone();
        let doc_dir = doc_dir.clone();
        let level_row = level_row.clone();
        let generate_button_for_click = generate_button.clone();
        let status_label = status_label.clone();
        let text_buffer = text_buffer.clone();
        generate_button.connect_clicked(move |_| {
            let level = DetailLevel::ALL[level_row.selected() as usize];
            run_generation(level, &source, &filename, doc_dir.clone(), &generate_button_for_click, &status_label, &text_buffer);
        });
    }

    {
        let frontmatter = frontmatter.clone();
        let text_buffer = text_buffer.clone();
        let dialog = dialog.clone();
        apply_button.connect_clicked(move |_| {
            let text = text_buffer.text(&text_buffer.start_iter(), &text_buffer.end_iter(), false).to_string();
            let text = text.trim().to_string();
            if let Some(item) = frontmatter.borrow_mut().media.get_mut(index) {
                item.alt = if text.is_empty() { AltText::Empty } else { AltText::Text(text) };
            }
            dialog.close();
        });
    }

    dialog.present(Some(window));

    // Generate an initial Standard-level suggestion right away, so the
    // reviewer's first action after opening is reading/correcting, not
    // clicking a button that just produces the obvious next step.
    run_generation(DetailLevel::Standard, &item.source, &item.filename, doc_dir, &generate_button, &status_label, &text_buffer);
}

fn run_generation(
    level: DetailLevel,
    source: &str,
    filename: &str,
    doc_dir: Option<PathBuf>,
    generate_button: &gtk4::Button,
    status_label: &gtk4::Label,
    text_buffer: &gtk4::TextBuffer,
) {
    generate_button.set_sensitive(false);
    status_label.set_label("Wird generiert …");
    status_label.set_visible(true);

    let source = source.to_string();
    let mime_type = export::mime_from_extension(filename);
    let prompt = level.prompt().to_string();
    let config = chatconfig::load_provider_config();
    let provider = config.active;
    let model = config.model_for(provider).to_string();
    let base_url = config.ollama_base_url.clone();

    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    std::thread::spawn(move || {
        let outcome = export::read_image_bytes(&source, doc_dir.as_deref()).and_then(|bytes| {
            let client = if provider.needs_api_key() {
                let key = futures_lite::future::block_on(secrets::load_llm_api_key(provider.id()))
                    .map_err(|err| err.to_string())?
                    .ok_or_else(|| format!("Kein {}-API-Key in den Einstellungen hinterlegt.", provider.label()))?;
                llm::Client::new(provider, &key, &model, &base_url)
            } else {
                llm::Client::new(provider, "", &model, &base_url)
            };
            client.describe_image(&prompt, &bytes, mime_type).map_err(|err| err.to_string())
        });
        let _ = tx.send(outcome);
    });

    let generate_button = generate_button.clone();
    let status_label = status_label.clone();
    let text_buffer = text_buffer.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(text)) => {
            text_buffer.set_text(text.trim());
            status_label.set_visible(false);
            generate_button.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Ok(Err(err)) => {
            status_label.set_label(&format!("Fehler: {err}"));
            generate_button.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            status_label.set_label("Interner Fehler: Generierungs-Thread hat kein Ergebnis geliefert.");
            generate_button.set_sensitive(true);
            glib::ControlFlow::Break
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_detail_level_has_a_distinct_prompt() {
        let prompts: Vec<&str> = DetailLevel::ALL.iter().map(DetailLevel::prompt).collect();
        assert_ne!(prompts[0], prompts[1]);
        assert_ne!(prompts[1], prompts[2]);
        assert_ne!(prompts[0], prompts[2]);
    }

    #[test]
    fn every_detail_level_has_a_non_empty_label() {
        for level in DetailLevel::ALL {
            assert!(!level.label().is_empty());
        }
    }
}
