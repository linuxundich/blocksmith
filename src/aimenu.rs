//! Wires the editor's right-click context menu with a "KI-Aktionen"
//! section: predefined prompts (Übersetzen/Inhalt/Stil/Rechtschreibung/
//! Zeichensetzung/Länge) plus the user's own custom prompts, each sending
//! the current selection (or, if nothing is selected, the whole article)
//! to the Chat tab together with the matching prompt from `aiprompts.rs`.
//!
//! The menu is a single `gio::Menu` combining the editor's existing
//! spelling-suggestions menu (built in `editor.rs`) with the AI actions
//! built here, set once via `view.set_extra_menu`. The "Übersetzen"
//! submenu and the custom-prompts section are kept as long-lived `gio::Menu`
//! handles (`rebuild_translate_menu`/`rebuild_custom_prompts_menu`) so
//! `promptsettings.rs` can update them live when the user edits languages
//! or custom prompts, without needing the app restarted - `gio::Menu`
//! mutations are reflected immediately by anything displaying the model.

use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib};

use crate::aiprompts::{self, CustomPrompt};
use crate::chat::ChatView;

pub struct AiMenuHandles {
    pub translate_menu: gio::Menu,
    pub custom_prompts_menu: gio::Menu,
}

/// Builds the AI action group + context menu section and installs both on
/// `view`. Returns the live menu handles `promptsettings.rs` needs to keep
/// the "Übersetzen" and "Eigene Prompts" menu contents in sync with the
/// settings dialog.
pub fn install(
    view: &sourceview5::View,
    buffer: &sourceview5::Buffer,
    view_stack: &adw::ViewStack,
    chat_view: Rc<ChatView>,
    spelling_menu: &gio::MenuModel,
    image_alt_menu: &gio::MenuModel,
) -> AiMenuHandles {
    let translate_menu = gio::Menu::new();
    rebuild_translate_menu(&translate_menu, &aiprompts::load_translate_languages());

    let custom_prompts_menu = gio::Menu::new();
    rebuild_custom_prompts_menu(&custom_prompts_menu, &aiprompts::load_custom_prompts());

    let translate_item = gio::MenuItem::new(Some("Übersetzen"), None);
    translate_item.set_submenu(Some(&translate_menu));

    let builtin_actions = gio::Menu::new();
    builtin_actions.append_item(&translate_item);
    for prompt in aiprompts::BUILTIN_PROMPTS {
        if prompt.id == "adjust-length" {
            let item = gio::MenuItem::new(Some(&format!("{} …", prompt.title)), Some("ai.adjust-length"));
            builtin_actions.append_item(&item);
        } else {
            builtin_actions.append_item(&run_menu_item(prompt.title, prompt.id));
        }
    }

    let ai_menu = gio::Menu::new();
    ai_menu.append_section(Some("KI-Aktionen"), &builtin_actions);
    ai_menu.append_section(None, &custom_prompts_menu);

    let combined_extra_menu = gio::Menu::new();
    combined_extra_menu.append_section(None, spelling_menu);
    combined_extra_menu.append_section(None, image_alt_menu);
    combined_extra_menu.append_section(None, &ai_menu);
    view.set_extra_menu(Some(&combined_extra_menu));

    let actions = gio::SimpleActionGroup::new();

    let run_action = gio::SimpleAction::new("run", Some(&String::static_variant_type()));
    {
        let buffer = buffer.clone();
        let view_stack = view_stack.clone();
        let chat_view = chat_view.clone();
        run_action.connect_activate(move |_, param| {
            let Some(id) = param.and_then(glib::Variant::str) else { return };
            trigger_prompt_run(&buffer, &view_stack, &chat_view, id);
        });
    }
    actions.add_action(&run_action);

    let translate_action = gio::SimpleAction::new("translate", Some(&String::static_variant_type()));
    {
        let buffer = buffer.clone();
        let view_stack = view_stack.clone();
        let chat_view = chat_view.clone();
        translate_action.connect_activate(move |_, param| {
            let Some(language) = param.and_then(glib::Variant::str) else { return };
            trigger_translate(&buffer, &view_stack, &chat_view, language);
        });
    }
    actions.add_action(&translate_action);

    let adjust_length_action = gio::SimpleAction::new("adjust-length", None);
    {
        let buffer = buffer.clone();
        let view_stack = view_stack.clone();
        let chat_view = chat_view.clone();
        let view_weak = view.downgrade();
        adjust_length_action.connect_activate(move |_, _| {
            let Some(view) = view_weak.upgrade() else { return };
            let Some(root) = view.root() else { return };
            let Ok(window) = root.downcast::<gtk4::Window>() else { return };
            open_adjust_length_dialog(&window, &buffer, &view_stack, chat_view.clone());
        });
    }
    actions.add_action(&adjust_length_action);

    view.insert_action_group("ai", Some(&actions));

    AiMenuHandles { translate_menu, custom_prompts_menu }
}

fn run_menu_item(label: &str, prompt_id: &str) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some("ai.run"), Some(&prompt_id.to_variant()));
    item
}

/// Rebuilds `menu`'s contents in place from `languages` - called both at
/// startup and live from `promptsettings.rs` whenever the user edits the
/// language list, since mutating an already-installed `gio::Menu` updates
/// any popover showing it immediately.
pub fn rebuild_translate_menu(menu: &gio::Menu, languages: &[String]) {
    menu.remove_all();
    for language in languages {
        let item = gio::MenuItem::new(Some(language), None);
        item.set_action_and_target_value(Some("ai.translate"), Some(&language.to_variant()));
        menu.append_item(&item);
    }
}

/// Rebuilds `menu`'s contents in place from `prompts` - see
/// `rebuild_translate_menu` for why this is an in-place mutation rather
/// than swapping the menu object out.
pub fn rebuild_custom_prompts_menu(menu: &gio::Menu, prompts: &[CustomPrompt]) {
    menu.remove_all();
    for prompt in prompts {
        if prompt.title.trim().is_empty() {
            continue;
        }
        let id = format!("custom:{}", prompt.id);
        menu.append_item(&run_menu_item(&prompt.title, &id));
    }
}

fn selected_or_full_text(buffer: &sourceview5::Buffer) -> String {
    if let Some((start, end)) = buffer.selection_bounds() {
        buffer.text(&start, &end, false).to_string()
    } else {
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string()
    }
}

fn dispatch_to_chat(view_stack: &adw::ViewStack, chat_view: &ChatView, display_label: &str, full_prompt: String) {
    view_stack.set_visible_child_name("chat");
    chat_view.refresh();
    chat_view.run_action(display_label, full_prompt);
}

fn trigger_prompt_run(buffer: &sourceview5::Buffer, view_stack: &adw::ViewStack, chat_view: &ChatView, id: &str) {
    let content = selected_or_full_text(buffer);
    let (title, template) = if let Some(custom_id) = id.strip_prefix("custom:") {
        let Some(prompt) = aiprompts::load_custom_prompts().into_iter().find(|p| p.id == custom_id) else {
            return;
        };
        (prompt.title, prompt.template)
    } else {
        (aiprompts::builtin_title(id).to_string(), aiprompts::load_prompt_text(id))
    };
    let full_prompt = format!("{template}\n\n---\n\n{content}");
    dispatch_to_chat(view_stack, chat_view, &title, full_prompt);
}

fn trigger_translate(buffer: &sourceview5::Buffer, view_stack: &adw::ViewStack, chat_view: &ChatView, language: &str) {
    let content = selected_or_full_text(buffer);
    let template = aiprompts::load_prompt_text("translate").replace("{language}", language);
    let full_prompt = format!("{template}\n\n---\n\n{content}");
    dispatch_to_chat(view_stack, chat_view, &format!("Übersetzen → {language}"), full_prompt);
}

fn open_adjust_length_dialog(window: &gtk4::Window, buffer: &sourceview5::Buffer, view_stack: &adw::ViewStack, chat_view: Rc<ChatView>) {
    let dialog = adw::AlertDialog::builder()
        .heading("Länge anpassen")
        .body("Wie soll die Auswahl (oder der ganze Artikel, falls nichts markiert ist) angepasst werden?")
        .build();
    dialog.add_response("cancel", "Abbrechen");
    dialog.add_response("apply", "Anwenden");
    dialog.set_default_response(Some("apply"));
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);

    let amount_adjustment = gtk4::Adjustment::new(800.0, 1.0, 100_000.0, 10.0, 100.0, 0.0);
    let amount_row = adw::SpinRow::builder().title("Zielumfang").adjustment(&amount_adjustment).build();

    let words_toggle = gtk4::ToggleButton::with_label("Wörter");
    let chars_toggle = gtk4::ToggleButton::with_label("Zeichen");
    chars_toggle.set_group(Some(&words_toggle));
    words_toggle.set_active(true);
    let unit_box = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(0).build();
    unit_box.add_css_class("linked");
    unit_box.append(&words_toggle);
    unit_box.append(&chars_toggle);

    let approx_toggle = gtk4::ToggleButton::with_label("Etwa");
    let exact_toggle = gtk4::ToggleButton::with_label("Genau");
    exact_toggle.set_group(Some(&approx_toggle));
    approx_toggle.set_active(true);
    let precision_box = gtk4::Box::builder().orientation(gtk4::Orientation::Horizontal).spacing(0).build();
    precision_box.add_css_class("linked");
    precision_box.append(&approx_toggle);
    precision_box.append(&exact_toggle);

    let content = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(12).margin_top(12).build();
    content.append(&amount_row);
    content.append(&unit_box);
    content.append(&precision_box);
    dialog.set_extra_child(Some(&content));

    let buffer = buffer.clone();
    let view_stack = view_stack.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "apply" {
            return;
        }
        let amount = amount_adjustment.value().round() as i64;
        let unit = if chars_toggle.is_active() { "Zeichen" } else { "Wörter" };
        let precision = if exact_toggle.is_active() { "genau" } else { "etwa" };
        let length_instruction = format!("auf {precision} {amount} {unit}");

        let content = selected_or_full_text(&buffer);
        let template = aiprompts::load_prompt_text("adjust-length").replace("{length_instruction}", &length_instruction);
        let full_prompt = format!("{template}\n\n---\n\n{content}");
        dispatch_to_chat(&view_stack, &chat_view, &format!("Länge anpassen ({length_instruction})"), full_prompt);
    });

    dialog.present(Some(window));
}
