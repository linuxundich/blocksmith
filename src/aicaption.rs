//! AI-generated image captions ("Bildunterschrift"): reachable from the
//! same "KI-Aktionen" context menus `aialt.rs`'s alt-text generation
//! already uses (`imagealt.rs`'s editor menu, `preview.rs`'s image
//! right-click menu) - a deliberately separate flow rather than folded
//! into `aialt.rs`, since a caption needs a different prompt (an editorial
//! style with a fixed target length, and - unlike alt text - real article
//! context, not just the image itself) and has no "Detailgrad" concept to
//! offer, so its dialog is simpler by one control.
//!
//! `context` (the text immediately surrounding the image in the article -
//! see `imagealt::surrounding_context`) is folded directly into the vision
//! prompt sent alongside the image, so the caption can describe what the
//! photo is illustrating *in the article*, not just what's visible in
//! isolation. Same "review before apply" shape as `aialt.rs`: nothing is
//! written until the user clicks "Übernehmen", and applying it just calls
//! the caller's own `on_apply` - so it's the caller's job to decide where
//! the text actually goes (`MediaItem.caption`, via `imagealt.rs`).

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::i18n::tr;
use crate::{chatconfig, export, llm, secrets};

/// The user's own captions typically run 20-25 words - stated explicitly in
/// the prompt rather than left to the model's own judgment of "a caption's
/// usual length".
fn build_prompt(context: &str) -> String {
    let mut prompt = String::from(
        "Du schreibst eine Bildunterschrift (Caption) für ein Bild in einem Blogartikel, im \
         redaktionellen Stil des Artikels - nicht nur eine reine Bildbeschreibung, sondern ein \
         Satz, der das Bild inhaltlich in den Artikel einordnet. Ziel-Länge: 20 bis 25 Wörter. \
         Antworte ausschließlich mit der Bildunterschrift selbst, ohne Anführungszeichen, ohne \
         Erklärung, ohne Zeilenumbrüche.",
    );
    if !context.trim().is_empty() {
        prompt.push_str("\n\nKontext aus dem Artikeltext direkt vor und nach dem Bild:\n");
        prompt.push_str(context.trim());
    }
    prompt
}

/// Opens the review dialog for one image, identified the same way
/// `aialt::open` is (`title`/`source`/`doc_dir`) - see that function's doc
/// comment for the exact meaning of each. `context` is the surrounding
/// article text (`imagealt::surrounding_context`); passing an empty string
/// is fine, `build_prompt` just omits that part of the request.
pub fn open(window: &gtk4::Window, title: String, source: String, context: String, doc_dir: Option<PathBuf>, on_apply: impl Fn(String) + 'static) {
    let generate_button = gtk4::Button::with_label(&tr("Bildunterschrift generieren"));
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
    text_buffer.set_text(&tr("Noch keine Bildunterschrift generiert - auf „Bildunterschrift generieren“ klicken."));

    let frame = gtk4::Frame::new(None);
    frame.set_child(Some(&text_view));
    let text_scroller = gtk4::ScrolledWindow::builder().child(&frame).min_content_height(100).vexpand(true).build();

    let group = adw::PreferencesGroup::builder().title(&title).build();

    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(&group);
    content.append(&generate_button);
    content.append(&status_label);
    content.append(&text_scroller);

    let apply_button = gtk4::Button::with_label(&tr("Übernehmen"));
    apply_button.add_css_class("suggested-action");

    let header = adw::HeaderBar::new();
    header.pack_end(&apply_button);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));

    let dialog = adw::Dialog::builder().title(tr("KI-Bildunterschrift")).content_width(460).content_height(360).child(&toolbar_view).build();

    {
        let source = source.clone();
        let context = context.clone();
        let doc_dir = doc_dir.clone();
        let generate_button_for_click = generate_button.clone();
        let status_label = status_label.clone();
        let text_buffer = text_buffer.clone();
        generate_button.connect_clicked(move |_| {
            run_generation(&source, &context, doc_dir.clone(), &generate_button_for_click, &status_label, &text_buffer);
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

fn run_generation(source: &str, context: &str, doc_dir: Option<PathBuf>, generate_button: &gtk4::Button, status_label: &gtk4::Label, text_buffer: &gtk4::TextBuffer) {
    generate_button.set_sensitive(false);
    status_label.set_label(&tr("Wird generiert …"));
    status_label.set_visible(true);

    let source = source.to_string();
    let mime_type = export::mime_from_extension(&source);
    let prompt = build_prompt(context);
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
    fn build_prompt_states_the_target_word_count() {
        let prompt = build_prompt("");
        assert!(prompt.contains("20 bis 25 Wörter"), "{prompt}");
    }

    #[test]
    fn build_prompt_includes_the_surrounding_context_when_given() {
        let prompt = build_prompt("Der Autor testet ein neues Gerät.");
        assert!(prompt.contains("Der Autor testet ein neues Gerät."), "{prompt}");
    }

    #[test]
    fn build_prompt_omits_the_context_section_when_empty() {
        let prompt = build_prompt("");
        assert!(!prompt.contains("Kontext aus dem Artikeltext"), "{prompt}");
    }

    #[test]
    fn build_prompt_omits_the_context_section_when_only_whitespace() {
        let prompt = build_prompt("   \n  ");
        assert!(!prompt.contains("Kontext aus dem Artikeltext"), "{prompt}");
    }
}
