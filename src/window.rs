use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::{gio, glib};
use webkit6::prelude::*;

use crate::document::{Document, Frontmatter};
use crate::{chat, document, editor, export, formatting, importer, preview, properties, settings, stats, termcache};

const DEBOUNCE_MS: u64 = 250;

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let (editor_scroller, view, buffer) = editor::build();
    let web_view = preview::build();
    let stats_view = Rc::new(stats::StatsView::new());

    let toolbar = formatting::build(&view, &buffer);
    formatting::install_shortcuts(&view, &buffer);
    let editor_pane = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    editor_pane.append(&toolbar);
    editor_pane.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    editor_pane.append(&editor_scroller);
    editor_scroller.set_vexpand(true);

    let chat_view = chat::ChatView::new();

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled_with_icon(&web_view, Some("preview"), "Vorschau", "view-reveal-symbolic");
    view_stack.add_titled_with_icon(&stats_view.widget, Some("stats"), "Statistik", "view-list-symbolic");
    view_stack.add_titled_with_icon(&chat_view.widget, Some("chat"), "Chat", "chat-symbolic");
    let view_switcher = adw::ViewSwitcher::builder().stack(&view_stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    let switcher_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .build();
    switcher_bar.append(&view_switcher);

    let preview_pane = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    preview_pane.append(&switcher_bar);
    preview_pane.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    preview_pane.append(&view_stack);
    view_stack.set_vexpand(true);

    let paned = gtk4::Paned::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .start_child(&editor_pane)
        .end_child(&preview_pane)
        .resize_start_child(true)
        .resize_end_child(true)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .position(650)
        .build();

    let title = adw::WindowTitle::new("Blocksmith", "Unbenannt");

    let new_button = gtk4::Button::from_icon_name("document-new-symbolic");
    new_button.set_tooltip_text(Some("Neu (Strg+N)"));
    new_button.set_action_name(Some("win.new"));

    let open_button = gtk4::Button::from_icon_name("document-open-symbolic");
    open_button.set_tooltip_text(Some("Öffnen (Strg+O)"));
    open_button.set_action_name(Some("win.open"));

    let open_from_wp_button = gtk4::Button::from_icon_name("folder-remote-symbolic");
    open_from_wp_button.set_tooltip_text(Some("Von WordPress öffnen (Strg+Umschalt+O)"));
    open_from_wp_button.set_action_name(Some("win.open-from-wordpress"));

    let save_button = gtk4::Button::from_icon_name("document-save-symbolic");
    save_button.set_tooltip_text(Some("Speichern (Strg+S)"));
    save_button.set_action_name(Some("win.save"));

    let properties_button = gtk4::Button::from_icon_name("document-properties-symbolic");
    properties_button.set_tooltip_text(Some("Artikel-Eigenschaften"));
    properties_button.set_action_name(Some("win.properties"));

    let settings_button = gtk4::Button::from_icon_name("preferences-system-symbolic");
    settings_button.set_tooltip_text(Some("Einstellungen (Strg+,)"));
    settings_button.set_action_name(Some("win.settings"));

    let publish_button = gtk4::Button::from_icon_name("send-to-symbolic");
    publish_button.set_tooltip_text(Some("Artikel exportieren (Strg+Umschalt+P)"));
    publish_button.set_action_name(Some("win.publish"));
    publish_button.add_css_class("suggested-action");

    let header_bar = adw::HeaderBar::new();
    header_bar.set_title_widget(Some(&title));
    header_bar.pack_start(&new_button);
    header_bar.pack_start(&open_button);
    header_bar.pack_start(&open_from_wp_button);
    header_bar.pack_start(&save_button);
    header_bar.pack_end(&settings_button);
    header_bar.pack_end(&properties_button);
    header_bar.pack_end(&publish_button);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&paned));

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar_view));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Blocksmith")
        .default_width(1280)
        .default_height(800)
        .content(&toast_overlay)
        .build();

    let current_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let frontmatter: Rc<RefCell<Frontmatter>> = Rc::new(RefCell::new(Frontmatter::default()));

    let cached_terms = termcache::load();
    let category_terms: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(cached_terms.categories));
    let tag_terms: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(cached_terms.tags));
    termcache::spawn_refresh(category_terms.clone(), tag_terms.clone());

    wire_live_preview(&buffer, &web_view, &stats_view);
    wire_scroll_sync(&editor_scroller, &view, &web_view);
    wire_new_action(&window, &buffer, &current_path, &frontmatter, &title);
    wire_open_action(&window, &buffer, &current_path, &frontmatter, &title, &toast_overlay);
    wire_open_from_wordpress_action(&window, &buffer, &current_path, &frontmatter, &title);
    wire_save_action(&window, &buffer, &current_path, &frontmatter, &title, &toast_overlay);
    wire_properties_action(&window, &frontmatter, &category_terms, &tag_terms);
    wire_settings_action(&window);
    wire_publish_action(&window, &buffer, &current_path, &frontmatter);

    window
}

fn show_toast(overlay: &adw::ToastOverlay, message: &str) {
    overlay.add_toast(adw::Toast::new(message));
}

fn subtitle_for(path: Option<&Path>, frontmatter: &Frontmatter) -> String {
    if !frontmatter.title.is_empty() {
        return frontmatter.title.clone();
    }
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unbenannt".to_string())
}

fn wire_live_preview(buffer: &sourceview5::Buffer, web_view: &webkit6::WebView, stats_view: &Rc<stats::StatsView>) {
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let web_view_clone = web_view.clone();
    let stats_view_clone = stats_view.clone();
    let debounce_clone = debounce.clone();
    buffer.connect_changed(move |buf| {
        if let Some(id) = debounce_clone.borrow_mut().take() {
            id.remove();
        }
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let web_view = web_view_clone.clone();
        let stats_view = stats_view_clone.clone();
        let debounce_inner = debounce_clone.clone();
        let id = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
            web_view.load_html(&preview::render_html(&text), None);
            stats_view.update(&text);
            *debounce_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debounce_clone.borrow_mut() = Some(id);
    });

    web_view.load_html(&preview::render_html(""), None);
    stats_view.update("");
}

const SCROLL_SYNC_DEBOUNCE_MS: u64 = 80;

/// Drives the preview's scroll position from the editor's: on every editor
/// scroll, finds the source line currently at the top of the editor's
/// viewport and asks the preview (via `preview::render_html`'s embedded
/// `scrollToLine`) to bring the block starting at or before that line to
/// its own top - matching by source line rather than by scroll percentage,
/// since a block's rendered height doesn't correspond to its line count
/// (an image is one source line but can render far taller than that).
fn wire_scroll_sync(scroller: &gtk4::ScrolledWindow, view: &sourceview5::View, web_view: &webkit6::WebView) {
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let view = view.clone();
    let web_view = web_view.clone();
    scroller.vadjustment().connect_value_changed(move |_| {
        if let Some(id) = debounce.borrow_mut().take() {
            id.remove();
        }
        let view = view.clone();
        let web_view = web_view.clone();
        let debounce_inner = debounce.clone();
        let id = glib::timeout_add_local(Duration::from_millis(SCROLL_SYNC_DEBOUNCE_MS), move || {
            let rect = view.visible_rect();
            if let Some(iter) = view.iter_at_location(rect.x(), rect.y()) {
                let line = iter.line() + 1;
                web_view.evaluate_javascript(
                    &format!("window.scrollToLine && window.scrollToLine({line});"),
                    None,
                    None,
                    gio::Cancellable::NONE,
                    |_| {},
                );
            }
            *debounce_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debounce.borrow_mut() = Some(id);
    });
}

fn wire_new_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    title: &adw::WindowTitle,
) {
    let action = gio::SimpleAction::new("new", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    action.connect_activate(move |_, _| {
        buffer.set_text("");
        *current_path.borrow_mut() = None;
        *frontmatter.borrow_mut() = Frontmatter::default();
        title.set_subtitle("Unbenannt");
    });
    window.add_action(&action);
}

fn wire_open_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    title: &adw::WindowTitle,
    toast_overlay: &adw::ToastOverlay,
) {
    let action = gio::SimpleAction::new("open", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let toast_overlay = toast_overlay.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };

        let filter = gtk4::FileFilter::new();
        filter.add_suffix("md");
        filter.set_name(Some("Markdown"));
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);

        let dialog = gtk4::FileDialog::builder()
            .title("Markdown-Datei öffnen")
            .filters(&filters)
            .build();

        let buffer = buffer.clone();
        let current_path = current_path.clone();
        let frontmatter = frontmatter.clone();
        let title = title.clone();
        let toast_overlay = toast_overlay.clone();
        dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            match document::read(&path) {
                Ok(doc) => {
                    buffer.set_text(&doc.body);
                    title.set_subtitle(&subtitle_for(Some(&path), &doc.frontmatter));
                    *frontmatter.borrow_mut() = doc.frontmatter;
                    *current_path.borrow_mut() = Some(path);
                }
                Err(err) => show_toast(&toast_overlay, &format!("Öffnen fehlgeschlagen: {err}")),
            }
        });
    });
    window.add_action(&action);
}

fn wire_open_from_wordpress_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    title: &adw::WindowTitle,
) {
    let action = gio::SimpleAction::new("open-from-wordpress", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let buffer = buffer.clone();
        let current_path = current_path.clone();
        let frontmatter = frontmatter.clone();
        let title = title.clone();
        importer::open(&window, move |imported| {
            buffer.set_text(&imported.body);
            title.set_subtitle(&subtitle_for(None, &imported.frontmatter));
            *frontmatter.borrow_mut() = imported.frontmatter;
            *current_path.borrow_mut() = None;
        });
    });
    window.add_action(&action);
}

fn wire_save_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    title: &adw::WindowTitle,
    toast_overlay: &adw::ToastOverlay,
) {
    let action = gio::SimpleAction::new("save", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let toast_overlay = toast_overlay.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
        let doc = Document {
            frontmatter: frontmatter.borrow().clone(),
            body,
        };

        if let Some(path) = current_path.borrow().clone() {
            if let Err(err) = document::write(&path, &doc) {
                show_toast(&toast_overlay, &format!("Speichern fehlgeschlagen: {err}"));
            }
            return;
        }

        let dialog = gtk4::FileDialog::builder()
            .title("Markdown-Datei speichern")
            .initial_name("artikel.md")
            .build();

        let current_path = current_path.clone();
        let title = title.clone();
        let toast_overlay = toast_overlay.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            if let Err(err) = document::write(&path, &doc) {
                show_toast(&toast_overlay, &format!("Speichern fehlgeschlagen: {err}"));
                return;
            }
            title.set_subtitle(&subtitle_for(Some(&path), &doc.frontmatter));
            *current_path.borrow_mut() = Some(path);
        });
    });
    window.add_action(&action);
}

fn wire_properties_action(
    window: &adw::ApplicationWindow,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    category_terms: &Rc<RefCell<Vec<String>>>,
    tag_terms: &Rc<RefCell<Vec<String>>>,
) {
    let action = gio::SimpleAction::new("properties", None);
    let frontmatter = frontmatter.clone();
    let category_terms = category_terms.clone();
    let tag_terms = tag_terms.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            properties::open(&window, frontmatter.clone(), category_terms.clone(), tag_terms.clone());
        }
    });
    window.add_action(&action);
}

fn wire_settings_action(window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("settings", None);
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            settings::open(&window);
        }
    });
    window.add_action(&action);
}

fn wire_publish_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
) {
    let action = gio::SimpleAction::new("publish", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let body = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
        let doc_dir = current_path.borrow().as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
        export::open(&window, body, frontmatter.clone(), doc_dir);
    });
    window.add_action(&action);
}
