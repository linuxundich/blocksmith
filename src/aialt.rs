//! AI-generated alt text: right-click an image - its `![alt](src)` line in
//! the editor (see `imagealt.rs`) or the rendered image itself in the
//! Vorschau pane (see `preview.rs`) - to have the active KI-Chat provider
//! look at the real image bytes and propose an accessible alt text, at one
//! of three levels of detail. Also reachable for the featured image from
//! Artikel-Eigenschaften (`properties.rs`), which isn't a `MediaItem` at
//! all. The result is shown for review/correction before anything is
//! written; applying it just calls the caller's own `on_apply` (see
//! `open`'s doc comment) - so it's the caller's job to decide where that
//! text actually goes (`MediaItem.alt` for a body image,
//! `Frontmatter.featured_image_alt` for the featured one), not this
//! module's.
//!
//! Deliberately no attempt to hide this behind "is an LLM actually
//! configured" - matching `imagealt.rs`'s own choice to always show its
//! menu item and explain rather than silently do nothing: a missing/invalid
//! API key surfaces as a normal inline error from the same generation call
//! chat.rs already makes, not as a pre-check.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::i18n::tr;
use crate::{chatconfig, export, llm, secrets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailLevel {
    Standard,
    Detailed,
    Precise,
}

impl DetailLevel {
    const ALL: [DetailLevel; 3] = [DetailLevel::Standard, DetailLevel::Detailed, DetailLevel::Precise];

    fn id(&self) -> &'static str {
        match self {
            DetailLevel::Standard => "standard",
            DetailLevel::Detailed => "detailed",
            DetailLevel::Precise => "precise",
        }
    }

    fn from_id(s: &str) -> Self {
        match s.trim() {
            "detailed" => DetailLevel::Detailed,
            "precise" => DetailLevel::Precise,
            _ => DetailLevel::Standard,
        }
    }

    fn label(&self) -> String {
        match self {
            DetailLevel::Standard => tr("Standard (kurz & bündig)"),
            DetailLevel::Detailed => tr("Ausführlich"),
            DetailLevel::Precise => tr("Hohe Genauigkeit"),
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

fn config_dir() -> PathBuf {
    let mut dir = glib::user_config_dir();
    dir.push("blocksmith");
    dir
}

fn detail_level_path() -> PathBuf {
    let mut path = config_dir();
    path.push("ai_alt_text_detail_level.txt");
    path
}

/// The detail level picked last time this dialog was used, for any image -
/// deliberately global rather than per-image, matching how a user tends to
/// settle on one preferred level for the whole article/site rather than
/// re-deciding per picture.
fn load_detail_level() -> DetailLevel {
    std::fs::read_to_string(detail_level_path()).map(|s| DetailLevel::from_id(&s)).unwrap_or(DetailLevel::Standard)
}

fn save_detail_level(level: DetailLevel) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(detail_level_path(), level.id());
}

/// Opens the review dialog for one image, identified only by `title`
/// (shown as the dialog's group heading - a filename for a body image, or
/// a fixed label for the featured image, which has no filename of its
/// own) and `source` (resolved against `doc_dir` the same way any other
/// local image reference is). Applying the result just calls `on_apply`
/// with the trimmed text - deliberately not tied to `MediaItem`/
/// `Frontmatter.media` at all, so the same dialog serves both a body
/// image (`imagealt.rs`/`preview.rs`, writing into `frontmatter.media
/// [index].alt`) and the featured image (`properties.rs`, writing into
/// `Frontmatter.featured_image_alt`), which isn't a `MediaItem` and has
/// no index into that list.
pub fn open(window: &gtk4::Window, title: String, source: String, doc_dir: Option<PathBuf>, on_apply: impl Fn(String) + 'static) {
    let level_labels: Vec<String> = DetailLevel::ALL.iter().map(DetailLevel::label).collect();
    let level_label_refs: Vec<&str> = level_labels.iter().map(String::as_str).collect();
    let selected_level = DetailLevel::ALL.iter().position(|l| *l == load_detail_level()).unwrap_or(0) as u32;
    let level_row = adw::ComboRow::builder()
        .title(tr("Detailgrad"))
        .model(&gtk4::StringList::new(&level_label_refs))
        .selected(selected_level)
        .build();
    level_row.connect_selected_notify(|row| {
        save_detail_level(DetailLevel::ALL[row.selected() as usize]);
    });

    let generate_button = gtk4::Button::with_label(&tr("Text generieren"));
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
    text_buffer.set_text(&tr("Noch kein Text generiert - auf „Text generieren“ klicken."));

    let frame = gtk4::Frame::new(None);
    frame.set_child(Some(&text_view));
    let text_scroller = gtk4::ScrolledWindow::builder().child(&frame).min_content_height(120).vexpand(true).build();

    let generate_row_box = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(8).build();
    generate_row_box.append(&generate_button);

    let group = adw::PreferencesGroup::builder().title(&title).build();
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

    let apply_button = gtk4::Button::with_label(&tr("Übernehmen"));
    apply_button.add_css_class("suggested-action");

    let header = adw::HeaderBar::new();
    header.pack_end(&apply_button);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));

    let dialog = adw::Dialog::builder().title(tr("KI-Alternativtext")).content_width(460).content_height(420).child(&toolbar_view).build();

    {
        let source = source.clone();
        let doc_dir = doc_dir.clone();
        let level_row = level_row.clone();
        let generate_button_for_click = generate_button.clone();
        let status_label = status_label.clone();
        let text_buffer = text_buffer.clone();
        generate_button.connect_clicked(move |_| {
            let level = DetailLevel::ALL[level_row.selected() as usize];
            // `source` doubles as the mime-detection input - it always has
            // a real file extension (a bare filename for a body image, a
            // local path for the featured image), and
            // `export::mime_from_extension` only ever looks at the part
            // after the last `.` anyway, so a full path works the same as
            // a bare filename here.
            run_generation(level, &source, &source, doc_dir.clone(), &generate_button_for_click, &status_label, &text_buffer);
        });
    }

    {
        let text_buffer = text_buffer.clone();
        let dialog = dialog.clone();
        apply_button.connect_clicked(move |_| {
            let text = text_buffer.text(&text_buffer.start_iter(), &text_buffer.end_iter(), false).to_string();
            on_apply(text.trim().to_string());
            dialog.close();
        });
    }

    dialog.present(Some(window));
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
    status_label.set_label(&tr("Wird generiert …"));
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
                    .ok_or_else(|| tr("Kein {provider}-API-Key in den Einstellungen hinterlegt.").replace("{provider}", provider.label()))?;
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
            status_label.set_label(&tr("Fehler: {err}").replace("{err}", &err));
            generate_button.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            status_label.set_label(&tr("Interner Fehler: Generierungs-Thread hat kein Ergebnis geliefert."));
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

    #[test]
    fn every_detail_level_round_trips_through_its_id() {
        for level in DetailLevel::ALL {
            assert_eq!(DetailLevel::from_id(level.id()), level);
        }
    }

    #[test]
    fn from_id_falls_back_to_standard_for_garbage() {
        assert_eq!(DetailLevel::from_id("not-a-level"), DetailLevel::Standard);
        assert_eq!(DetailLevel::from_id(""), DetailLevel::Standard);
    }
}
