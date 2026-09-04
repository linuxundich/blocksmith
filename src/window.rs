use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk4::{gio, glib};

use crate::document::{Document, Frontmatter};
use crate::{
    aimenu, chat, codeview, document, editor, export, formatting, importer, linkpicker, mediapanel, preview, properties, settings, stats,
    statusbar, termcache,
};

const DEBOUNCE_MS: u64 = 250;

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let (editor_scroller, view, buffer, spelling_menu) = editor::build();
    let preview_pane = Rc::new(preview::PreviewPane::new());
    let stats_view = Rc::new(stats::StatsView::new());

    let toolbar = formatting::build(&view, &buffer);
    formatting::install_shortcuts(&view, &buffer);
    let editor_pane = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    editor_pane.append(&toolbar);
    editor_pane.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    editor_pane.append(&editor_scroller);
    editor_scroller.set_vexpand(true);

    let chat_view = Rc::new(chat::ChatView::new());
    let code_view = Rc::new(codeview::CodeView::new());

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled_with_icon(&preview_pane.widget, Some("preview"), "Vorschau", "view-reveal-symbolic");
    view_stack.add_titled_with_icon(&code_view.widget, Some("code"), "Gutenberg-Code", "text-x-generic-symbolic");
    view_stack.add_titled_with_icon(&stats_view.widget, Some("stats"), "Statistik", "view-list-symbolic");
    view_stack.add_titled_with_icon(&chat_view.widget, Some("chat"), "Chat", "chat-message-new-symbolic");
    {
        // The active provider/model may have changed in Einstellungen since
        // the Chat tab was built (or since it was last shown), so refresh
        // its provider label/model picker every time it becomes visible.
        let chat_view = chat_view.clone();
        view_stack.connect_visible_child_name_notify(move |stack| {
            if stack.visible_child_name().as_deref() == Some("chat") {
                chat_view.refresh();
            }
        });
    }
    // `Adw.InlineViewSwitcher` renders all tabs as one seamless linked pill
    // (unlike `Adw.ViewSwitcher`, which only highlights the active tab and
    // leaves the others as loose, ungrouped buttons). Held in a plain
    // `Gtk.Box` with the exact same margins/spacing as `formatting::build`'s
    // toolbar - not an `Adw.HeaderBar`, which carries its own themed
    // background and height that never quite matched the editor's toolbar
    // (and differently so across themes/styles) - so the two toolbar rows
    // above each pane read as one consistent design regardless of theme.
    let view_switcher = adw::InlineViewSwitcher::builder().stack(&view_stack).build();
    let switcher_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    switcher_bar.append(&view_switcher);

    let right_pane = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
    right_pane.append(&switcher_bar);
    right_pane.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    right_pane.append(&view_stack);
    view_stack.set_vexpand(true);

    let paned = gtk4::Paned::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .start_child(&editor_pane)
        .end_child(&right_pane)
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

    let media_button = gtk4::Button::from_icon_name("image-x-generic-symbolic");
    media_button.set_tooltip_text(Some("Medienverwaltung (Strg+Umschalt+M)"));
    media_button.set_action_name(Some("win.media-manager"));

    let settings_button = gtk4::Button::from_icon_name("open-menu-symbolic");
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
    header_bar.pack_end(&media_button);
    header_bar.pack_end(&publish_button);

    let status_bar = Rc::new(statusbar::StatusBar::new());

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&paned));
    toolbar_view.add_bottom_bar(&status_bar.widget);

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

    let ai_menu_handles = aimenu::install(&view, &buffer, &view_stack, chat_view.clone(), &spelling_menu);

    wire_live_preview(&buffer, &preview_pane, &stats_view, &code_view);
    wire_scroll_sync(&editor_scroller, &buffer, &preview_pane);
    wire_status_bar(&buffer, &status_bar);
    wire_new_action(&window, &buffer, &current_path, &frontmatter, &title, &preview_pane);
    wire_open_action(&window, &buffer, &current_path, &frontmatter, &title, &toast_overlay, &preview_pane);
    wire_open_from_wordpress_action(&window, &buffer, &current_path, &frontmatter, &title, &preview_pane);
    wire_save_action(&window, &buffer, &current_path, &frontmatter, &title, &toast_overlay, &preview_pane);
    wire_properties_action(&window, &frontmatter, &category_terms, &tag_terms, &current_path);
    wire_settings_action(&window, &buffer, ai_menu_handles, &preview_pane);
    wire_publish_action(&window, &buffer, &current_path, &frontmatter);
    wire_media_action(&window, &buffer, &current_path, &frontmatter);
    wire_insert_image_action(&window, &buffer, &current_path);
    wire_insert_post_link_action(&window, &buffer);

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

fn wire_live_preview(
    buffer: &sourceview5::Buffer,
    preview_pane: &Rc<preview::PreviewPane>,
    stats_view: &Rc<stats::StatsView>,
    code_view: &Rc<codeview::CodeView>,
) {
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let preview_pane_clone = preview_pane.clone();
    let stats_view_clone = stats_view.clone();
    let code_view_clone = code_view.clone();
    let debounce_clone = debounce.clone();
    buffer.connect_changed(move |buf| {
        if let Some(id) = debounce_clone.borrow_mut().take() {
            id.remove();
        }
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let preview_pane = preview_pane_clone.clone();
        let stats_view = stats_view_clone.clone();
        let code_view = code_view_clone.clone();
        let debounce_inner = debounce_clone.clone();
        let id = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
            preview_pane.update(&text);
            stats_view.update(&text);
            code_view.update(&text);
            *debounce_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debounce_clone.borrow_mut() = Some(id);
    });

    preview_pane.update("");
    stats_view.update("");
    code_view.update("");
}

/// The bottom status bar: word count/reading time for the whole document
/// (debounced on `changed`, same rhythm as the preview/stats/code panels),
/// plus the same two numbers for the current selection - tracked via
/// `mark-set`, since that's the signal that fires for both cursor moves
/// and selection drags, with its own (shorter) debounce since it fires far
/// more often than `changed` while the user is just moving the cursor.
fn wire_status_bar(buffer: &sourceview5::Buffer, status_bar: &Rc<statusbar::StatusBar>) {
    const SELECTION_DEBOUNCE_MS: u64 = 120;

    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let status_bar_clone = status_bar.clone();
    let debounce_clone = debounce.clone();
    buffer.connect_changed(move |buf| {
        if let Some(id) = debounce_clone.borrow_mut().take() {
            id.remove();
        }
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let status_bar = status_bar_clone.clone();
        let debounce_inner = debounce_clone.clone();
        let id = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
            status_bar.update_document(&text);
            *debounce_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debounce_clone.borrow_mut() = Some(id);
    });
    status_bar.update_document("");

    let selection_debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let status_bar_clone = status_bar.clone();
    buffer.connect_mark_set(move |buf, _iter, _mark| {
        if let Some(id) = selection_debounce.borrow_mut().take() {
            id.remove();
        }
        let selection = buf.selection_bounds().map(|(start, end)| buf.text(&start, &end, false).to_string());
        let status_bar = status_bar_clone.clone();
        let selection_debounce_inner = selection_debounce.clone();
        let id = glib::timeout_add_local(Duration::from_millis(SELECTION_DEBOUNCE_MS), move || {
            status_bar.update_selection(selection.as_deref());
            *selection_debounce_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *selection_debounce.borrow_mut() = Some(id);
    });
}

const SCROLL_SYNC_THROTTLE_MS: u64 = 60;

/// Drives the preview's scroll position from the editor's: on every editor
/// scroll, estimates the source line currently at the top of the editor's
/// viewport and asks the preview (via `preview::render_html`'s embedded
/// `scrollToLine`) to bring the block starting at or before that line to
/// its own top.
///
/// The editor-side estimate is scroll-fraction-based (`adjustment position
/// / scrollable range` times total line count) rather than pixel-based:
/// `TextView::iter_at_location` at the buffer-coordinate left edge (`x=0`)
/// turned out to unreliably return `None` once the line-number gutter is
/// showing (verified with `examples/scroll_sync_debug.rs`), and nudging the
/// x by a few pixels "worked" in one manual check but isn't a principled
/// fix. A fraction-based estimate is fine *here* because editor lines are
/// plain, uniformly-tall text (unlike the preview's rendered blocks, where
/// an image can be many times taller than one source line) - that height
/// mismatch is handled entirely on the preview side via `data-line`
/// block-boundary snapping, so the editor side never needs pixel-perfect
/// line detection to get the overall sync right.
///
/// This is throttled, not debounced: a debounce (cancel-and-reschedule on
/// every event) only ever fires once scrolling has *stopped*, so the
/// preview sits frozen for the whole scroll gesture and then snaps to the
/// final position - exactly the "jumps instead of scrolling" symptom this
/// was built to fix. A throttle instead fires at most once per interval
/// *while* scrolling continues (leading edge immediately, a single
/// trailing-edge call queued for whatever's left of the window so the
/// final position is never dropped), so the preview visibly tracks the
/// editor the whole time instead of only catching up afterward.
fn wire_scroll_sync(scroller: &gtk4::ScrolledWindow, buffer: &sourceview5::Buffer, preview_pane: &Rc<preview::PreviewPane>) {
    let throttle_interval = Duration::from_millis(SCROLL_SYNC_THROTTLE_MS);
    let last_synced: Rc<Cell<Instant>> = Rc::new(Cell::new(Instant::now() - throttle_interval));
    let trailing: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let buffer = buffer.clone();
    let preview_pane = preview_pane.clone();

    scroller.vadjustment().connect_value_changed(move |adjustment| {
        let elapsed = last_synced.get().elapsed();
        if elapsed >= throttle_interval {
            if let Some(id) = trailing.borrow_mut().take() {
                id.remove();
            }
            sync_scroll(&buffer, &preview_pane, adjustment);
            last_synced.set(Instant::now());
            return;
        }
        if trailing.borrow().is_some() {
            return;
        }
        let adjustment = adjustment.clone();
        let buffer = buffer.clone();
        let preview_pane = preview_pane.clone();
        let last_synced = last_synced.clone();
        let trailing_inner = trailing.clone();
        let id = glib::timeout_add_local(throttle_interval - elapsed, move || {
            sync_scroll(&buffer, &preview_pane, &adjustment);
            last_synced.set(Instant::now());
            *trailing_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *trailing.borrow_mut() = Some(id);
    });
}

fn sync_scroll(buffer: &sourceview5::Buffer, preview_pane: &preview::PreviewPane, adjustment: &gtk4::Adjustment) {
    let total_lines = buffer.end_iter().line() + 1;
    let line = estimate_visible_line(adjustment.value(), adjustment.upper(), adjustment.page_size(), total_lines);
    preview_pane.scroll_to_line(line);
}

/// Pure scroll-fraction-to-line estimate backing `wire_scroll_sync` - see
/// that function's doc comment for why fraction-based is the right call
/// here specifically (editor lines are uniform height, unlike the
/// preview's rendered blocks).
fn estimate_visible_line(value: f64, upper: f64, page_size: f64, total_lines: i32) -> i32 {
    let scrollable_range = (upper - page_size).max(1.0);
    let fraction = (value / scrollable_range).clamp(0.0, 1.0);
    ((fraction * f64::from(total_lines)).round() as i32 + 1).clamp(1, total_lines.max(1))
}

fn wire_new_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    title: &adw::WindowTitle,
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let action = gio::SimpleAction::new("new", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let preview_pane = preview_pane.clone();
    action.connect_activate(move |_, _| {
        buffer.set_text("");
        *current_path.borrow_mut() = None;
        *frontmatter.borrow_mut() = Frontmatter::default();
        title.set_subtitle("Unbenannt");
        preview_pane.set_doc_dir(None);
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
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let action = gio::SimpleAction::new("open", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let toast_overlay = toast_overlay.clone();
    let preview_pane = preview_pane.clone();
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
        let preview_pane = preview_pane.clone();
        dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            match document::read(&path) {
                Ok(doc) => {
                    buffer.set_text(&doc.body);
                    title.set_subtitle(&subtitle_for(Some(&path), &doc.frontmatter));
                    *frontmatter.borrow_mut() = doc.frontmatter;
                    let doc_dir = path.parent().map(Path::to_path_buf);
                    *current_path.borrow_mut() = Some(path);
                    preview_pane.set_doc_dir(doc_dir);
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
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let action = gio::SimpleAction::new("open-from-wordpress", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let preview_pane = preview_pane.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let buffer = buffer.clone();
        let current_path = current_path.clone();
        let frontmatter = frontmatter.clone();
        let title = title.clone();
        let preview_pane = preview_pane.clone();
        importer::open(&window, move |imported| {
            buffer.set_text(&imported.body);
            title.set_subtitle(&subtitle_for(None, &imported.frontmatter));
            *frontmatter.borrow_mut() = imported.frontmatter;
            *current_path.borrow_mut() = None;
            preview_pane.set_doc_dir(None);
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
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let action = gio::SimpleAction::new("save", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let frontmatter = frontmatter.clone();
    let title = title.clone();
    let toast_overlay = toast_overlay.clone();
    let preview_pane = preview_pane.clone();
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
        let preview_pane = preview_pane.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            if let Err(err) = document::write(&path, &doc) {
                show_toast(&toast_overlay, &format!("Speichern fehlgeschlagen: {err}"));
                return;
            }
            title.set_subtitle(&subtitle_for(Some(&path), &doc.frontmatter));
            let doc_dir = path.parent().map(Path::to_path_buf);
            *current_path.borrow_mut() = Some(path);
            preview_pane.set_doc_dir(doc_dir);
        });
    });
    window.add_action(&action);
}

fn wire_properties_action(
    window: &adw::ApplicationWindow,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    category_terms: &Rc<RefCell<Vec<String>>>,
    tag_terms: &Rc<RefCell<Vec<String>>>,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
) {
    let action = gio::SimpleAction::new("properties", None);
    let frontmatter = frontmatter.clone();
    let category_terms = category_terms.clone();
    let tag_terms = tag_terms.clone();
    let current_path = current_path.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            let doc_dir = current_path.borrow().as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
            properties::open(&window, frontmatter.clone(), category_terms.clone(), tag_terms.clone(), doc_dir);
        }
    });
    window.add_action(&action);
}

fn wire_settings_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    ai_menu_handles: aimenu::AiMenuHandles,
    preview_pane: &Rc<preview::PreviewPane>,
) {
    let action = gio::SimpleAction::new("settings", None);
    let buffer = buffer.clone();
    let preview_pane = preview_pane.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            settings::open(&window, &buffer, &ai_menu_handles, &preview_pane);
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

fn wire_insert_image_action(window: &adw::ApplicationWindow, buffer: &sourceview5::Buffer, current_path: &Rc<RefCell<Option<PathBuf>>>) {
    let action = gio::SimpleAction::new("insert-image", None);
    let buffer = buffer.clone();
    let current_path = current_path.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };

        let filter = gtk4::FileFilter::new();
        filter.add_mime_type("image/*");
        filter.set_name(Some("Bilder"));
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);

        let dialog = gtk4::FileDialog::builder().title("Bild einfügen").filters(&filters).build();

        let buffer = buffer.clone();
        let doc_dir = current_path.borrow().as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
        dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            let reference = document::image_reference(&path, doc_dir.as_deref());
            formatting::insert_image(&buffer, &reference);
        });
    });
    window.add_action(&action);
}

fn wire_insert_post_link_action(window: &adw::ApplicationWindow, buffer: &sourceview5::Buffer) {
    let action = gio::SimpleAction::new("insert-post-link", None);
    let buffer = buffer.clone();
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            linkpicker::open(&window, &buffer);
        }
    });
    window.add_action(&action);
}

fn wire_media_action(
    window: &adw::ApplicationWindow,
    buffer: &sourceview5::Buffer,
    current_path: &Rc<RefCell<Option<PathBuf>>>,
    frontmatter: &Rc<RefCell<Frontmatter>>,
) {
    let action = gio::SimpleAction::new("media-manager", None);
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
        mediapanel::open(&window, body, frontmatter.clone(), doc_dir);
    });
    window.add_action(&action);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_visible_line_at_top_is_line_one() {
        assert_eq!(estimate_visible_line(0.0, 4020.0, 261.0, 200), 1);
    }

    #[test]
    fn estimate_visible_line_at_bottom_is_last_line() {
        assert_eq!(estimate_visible_line(3759.0, 4020.0, 261.0, 200), 200);
    }

    #[test]
    fn estimate_visible_line_partway_scales_proportionally() {
        // Matches the manually-verified diagnostic run: value=300 out of a
        // 3759 scrollable range in a 200-line buffer landed on line 15/16
        // via pixel-based iter_at_location.
        let line = estimate_visible_line(300.0, 4020.0, 261.0, 200);
        assert!((14..=17).contains(&line), "expected line near 15-16, got {line}");
    }

    #[test]
    fn estimate_visible_line_handles_short_document_without_dividing_by_zero() {
        assert_eq!(estimate_visible_line(0.0, 0.0, 0.0, 1), 1);
    }
}
