//! "Artikel-Eigenschaften" dialog: edits the document's frontmatter
//! (title/slug/status/categories/tags/featured image) in place. Changes are
//! synced live into the shared `Frontmatter` cell as the user types, mirroring
//! how GNOME preferences dialogs apply immediately without an OK button.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::{gio, glib};

use crate::document::{self, parse_list, Frontmatter, PostStatus};
use crate::i18n::tr;
use crate::{aialt, autocomplete, media, preview, secrets, tagsuggest, taxonomy, termcache, wpclient, wpsite};

/// Shows/hides an alt-text-length warning icon and sets its tooltip from
/// `media::alt_text_length_warning` - same non-blocking hint as
/// `mediapanel.rs`/`imagealt.rs` use for body-image alt text, applied here
/// to the featured image's alt text.
fn update_alt_length_warning(icon: &gtk4::Image, text: &str) {
    match media::alt_text_length_warning(text) {
        Some(message) => {
            icon.set_tooltip_text(Some(&message));
            icon.set_visible(true);
        }
        None => icon.set_visible(false),
    }
}

/// Google's search results truncate a URL display around this many
/// characters - past it, the SEO recommendation is a shorter slug (or,
/// where possible, a shorter category).
const RECOMMENDED_MAX_URL_LENGTH: usize = 73;

/// Builds the full URL a post's `slug` would actually publish at (domain,
/// the post's assumed-primary i.e. first category slug, and the post slug
/// itself) and its character count, for the SEO length check.
/// `category_slug` is `None` when no category is set yet (the check still
/// runs, just without that segment - WordPress would assign a default
/// category, whose real slug isn't knowable without an authenticated
/// site-settings lookup this app doesn't have).
fn seo_url_preview(domain: &str, category_slug: Option<&str>, slug: &str) -> (String, usize) {
    let mut url = domain.trim_end_matches('/').to_string();
    if let Some(category_slug) = category_slug.filter(|s| !s.is_empty()) {
        url.push('/');
        url.push_str(category_slug);
    }
    url.push('/');
    url.push_str(slug);
    url.push('/');
    let length = url.chars().count();
    (url, length)
}

/// Updates the "URL-Länge (SEO)" row from the current title/category/slug
/// state - called once at dialog build time and again on every slug/
/// category edit (`seo_url_preview` needs both).
fn refresh_url_length_row(
    row: &adw::ActionRow,
    icon: &gtk4::Image,
    domain: &str,
    frontmatter: &Rc<RefCell<Frontmatter>>,
    category_slugs: &Rc<RefCell<HashMap<String, String>>>,
) {
    let fm = frontmatter.borrow();
    if domain.is_empty() || fm.slug.is_empty() {
        row.set_subtitle(&tr("Wird berechnet, sobald WordPress-Seite und Slug gesetzt sind."));
        icon.set_visible(false);
        return;
    }
    let category_slug = fm.categories.first().map(|name| category_slugs.borrow().get(name).cloned().unwrap_or_else(|| document::slugify(name)));
    let (url, length) = seo_url_preview(domain, category_slug.as_deref(), &fm.slug);

    icon.set_visible(true);
    if length <= RECOMMENDED_MAX_URL_LENGTH {
        row.set_subtitle(&tr("{url} ({length} Zeichen)").replace("{url}", &url).replace("{length}", &length.to_string()));
        icon.set_icon_name(Some("object-select-symbolic"));
        icon.remove_css_class("error");
        icon.add_css_class("success");
    } else {
        row.set_subtitle(
            &tr("{url} ({length} Zeichen, empfohlen: max. {max})")
                .replace("{url}", &url)
                .replace("{length}", &length.to_string())
                .replace("{max}", &RECOMMENDED_MAX_URL_LENGTH.to_string()),
        );
        icon.set_icon_name(Some("dialog-warning-symbolic"));
        icon.remove_css_class("success");
        icon.add_css_class("error");
    }
}

pub fn open(
    parent: &adw::ApplicationWindow,
    body: String,
    frontmatter: Rc<RefCell<Frontmatter>>,
    term_caches: termcache::TermCacheHandles,
    doc_dir: Option<PathBuf>,
    preview_pane: Rc<preview::PreviewPane>,
) {
    let termcache::TermCacheHandles { categories: category_terms, tags: tag_terms, category_slugs } = term_caches;
    let site = wpsite::load();
    let current = frontmatter.borrow().clone();

    let title_row = adw::EntryRow::builder().title(tr("Titel")).text(current.title.as_str()).build();
    let slug_row = adw::EntryRow::builder().title(tr("Slug")).text(current.slug.as_str()).build();
    let slug_generate_button = gtk4::Button::from_icon_name("view-refresh-symbolic");
    slug_generate_button.set_tooltip_text(Some(&tr("Slug aus Titel generieren")));
    slug_generate_button.set_valign(gtk4::Align::Center);
    slug_generate_button.add_css_class("flat");
    slug_row.add_suffix(&slug_generate_button);

    // SEO recommendation (see `RECOMMENDED_MAX_URL_LENGTH`): the *whole*
    // published URL, not just the slug - domain + category + slug, since
    // that's what actually shows up truncated in search results. Needs the
    // category's real WordPress slug (not a guess from its name - the two
    // can differ, e.g. a category renamed without updating its slug), so
    // this reads `category_slugs` rather than deriving it locally.
    let url_length_row = adw::ActionRow::builder().title(tr("URL-Länge (SEO)")).build();
    let url_length_icon = gtk4::Image::new();
    url_length_icon.set_valign(gtk4::Align::Center);
    url_length_row.add_suffix(&url_length_icon);

    let excerpt_row = adw::EntryRow::builder()
        .title(tr("Auszug / Meta-Beschreibung"))
        .text(current.excerpt.clone().unwrap_or_default().as_str())
        .build();
    let seo_title_row = adw::EntryRow::builder()
        .title(tr("SEO-Titel"))
        .text(current.rank_math_title.clone().unwrap_or_default().as_str())
        .build();
    let seo_description_row = adw::EntryRow::builder()
        .title(tr("SEO-Beschreibung"))
        .text(current.rank_math_description.clone().unwrap_or_default().as_str())
        .build();
    let focus_keyword_row = adw::EntryRow::builder()
        .title(tr("Fokus-Keyword"))
        .text(current.rank_math_focus_keyword.clone().unwrap_or_default().as_str())
        .build();
    let categories_row = adw::EntryRow::builder()
        .title(tr("Kategorien (Komma-getrennt)"))
        .text(current.categories.join(", ").as_str())
        .build();
    let tags_row = adw::EntryRow::builder()
        .title(tr("Tags (Komma-getrennt)"))
        .text(current.tags.join(", ").as_str())
        .build();
    // Analyzes title/body against the site's already-known tags
    // (`tag_terms`) and proposes a handful to add - see `tagsuggest.rs`.
    let tags_suggest_button = gtk4::Button::from_icon_name("chat-message-new-symbolic");
    tags_suggest_button.set_tooltip_text(Some(&tr("KI-Tags vorschlagen…")));
    tags_suggest_button.set_valign(gtk4::Align::Center);
    tags_suggest_button.add_css_class("flat");
    tags_row.add_suffix(&tags_suggest_button);
    let featured_image_row = adw::EntryRow::builder()
        .title(tr("Featured Image (Pfad)"))
        .text(current.featured_image.clone().unwrap_or_default().as_str())
        .build();
    let featured_image_picker_button = gtk4::Button::from_icon_name("insert-image-symbolic");
    featured_image_picker_button.set_tooltip_text(Some(&tr("Bild auswählen…")));
    featured_image_picker_button.set_valign(gtk4::Align::Center);
    featured_image_picker_button.add_css_class("flat");
    featured_image_row.add_suffix(&featured_image_picker_button);
    // Unlike a body image's alt text (`MediaItem.alt`, scanned from the
    // Markdown), the featured image is a single `Frontmatter` field with
    // nothing to scan it from - so it needs its own plain entry here
    // rather than piggybacking on Medienverwaltung's per-image rows. Sent
    // as the resulting WordPress attachment's `alt_text` on upload
    // (`mediapanel.rs`), same as any other image.
    let featured_image_alt_row = adw::EntryRow::builder()
        .title(tr("Alt-Text für Aufmacherbild"))
        .text(current.featured_image_alt.clone().unwrap_or_default().as_str())
        .build();
    // Same "KI-Alternativtext generieren…" flow the editor/preview context
    // menus already offer for a body image (`aialt::open`) - only useful
    // once there's actually a local file to send to the vision model, so
    // hidden whenever `featured_image` is unset (already uploaded, or
    // never set), same condition `featured_image_picker_button`'s own
    // upload-button sibling in `mediapanel.rs` uses.
    let featured_image_ai_button = gtk4::Button::from_icon_name("chat-message-new-symbolic");
    featured_image_ai_button.set_tooltip_text(Some(&tr("KI-Alternativtext generieren…")));
    featured_image_ai_button.set_valign(gtk4::Align::Center);
    featured_image_ai_button.add_css_class("flat");
    featured_image_ai_button.set_visible(current.featured_image.is_some());
    featured_image_alt_row.add_suffix(&featured_image_ai_button);
    // Non-blocking hint for an unusually long alt text (see
    // `media::alt_text_length_warning`) - a suffix icon with a tooltip,
    // matching the same warning on the per-image rows in
    // `mediapanel.rs`/`imagealt.rs`.
    let featured_image_alt_length_warning_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    featured_image_alt_length_warning_icon.add_css_class("warning");
    featured_image_alt_length_warning_icon.set_visible(false);
    featured_image_alt_row.add_suffix(&featured_image_alt_length_warning_icon);
    update_alt_length_warning(&featured_image_alt_length_warning_icon, &featured_image_alt_row.text());

    let status_labels: Vec<String> = PostStatus::ALL.iter().map(|s| s.label()).collect();
    let status_label_refs: Vec<&str> = status_labels.iter().map(String::as_str).collect();
    let status_model = gtk4::StringList::new(&status_label_refs);
    let selected_index = PostStatus::ALL.iter().position(|s| *s == current.status).unwrap_or(0);
    let status_row = adw::ComboRow::builder()
        .title(tr("Status"))
        .model(&status_model)
        .selected(selected_index as u32)
        .build();

    let scheduled_row = adw::EntryRow::builder()
        .title(tr("Veröffentlichungstermin (JJJJ-MM-TT HH:MM)"))
        .text(current.scheduled_at.as_deref().map(document::format_scheduled_at_for_display).unwrap_or_default().as_str())
        .build();
    scheduled_row.set_visible(current.status == PostStatus::Future);

    // "Nicht ändern" (index 0) leaves `author_id` unset, so the post keeps
    // whichever author it already has on export (see `export.rs`'s own
    // comment on why an unset `author_id` is never sent at all) - the
    // options after it are populated once `list_users()` resolves, below.
    // Starts insensitive with just the cached name (if any) or a loading
    // placeholder, since the real list isn't known yet; `author_populating`
    // guards `connect_selected_notify` from firing (and clobbering
    // `frontmatter.author_id`) while this initial, not-yet-real model is
    // still showing.
    let author_options: Rc<RefCell<Vec<wpclient::WpUser>>> = Rc::new(RefCell::new(Vec::new()));
    let author_populating = Rc::new(Cell::new(true));
    let initial_author_label = current.author_name.clone().unwrap_or_else(|| tr("Wird geladen …"));
    let author_row = adw::ComboRow::builder()
        .title(tr("Autor"))
        .model(&gtk4::StringList::new(&[tr("Nicht ändern").as_str(), initial_author_label.as_str()]))
        .selected(if current.author_id.is_some() { 1 } else { 0 })
        .sensitive(false)
        .build();

    {
        let site = site.clone();
        let author_row = author_row.clone();
        let author_options = author_options.clone();
        let author_populating = author_populating.clone();
        let current_author_id = current.author_id;
        let current_author_name = current.author_name.clone();
        let (tx, rx) = mpsc::channel::<Result<Vec<wpclient::WpUser>, String>>();
        std::thread::spawn(move || {
            let outcome = if site.url.is_empty() {
                Err(tr("Keine WordPress-Verbindung eingerichtet."))
            } else {
                futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                    .map_err(|err| err.to_string())
                    .and_then(|maybe_password| maybe_password.ok_or_else(|| tr("Kein Application Password im Schlüsselbund gefunden.")))
                    .and_then(|password| wpclient::Client::new(&site.url, &site.username, &password).list_users().map_err(|err| err.to_string()))
            };
            let _ = tx.send(outcome);
        });
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok(mut users)) => {
                // The currently-set author might not be among `users` (a
                // deleted account, or one the API's `context=edit` still
                // can't see for permission reasons) - appended as a
                // synthetic extra entry from the cached name rather than
                // silently dropped, so the picker never shows a name-less
                // "Autor" for a document that had one.
                let known = current_author_id.is_some_and(|id| users.iter().any(|u| u.id == id));
                if let (false, Some(id)) = (known, current_author_id) {
                    users.push(wpclient::WpUser { id, name: current_author_name.clone().unwrap_or_else(|| id.to_string()) });
                }
                let mut labels = vec![tr("Nicht ändern")];
                labels.extend(users.iter().map(|u| u.name.clone()));
                let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let selected = current_author_id
                    .and_then(|id| users.iter().position(|u| u.id == id))
                    .map(|pos| pos as u32 + 1)
                    .unwrap_or(0);
                *author_options.borrow_mut() = users;
                author_row.set_model(Some(&gtk4::StringList::new(&label_refs)));
                author_row.set_selected(selected);
                author_row.set_sensitive(true);
                author_populating.set(false);
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                author_row.set_model(Some(&gtk4::StringList::new(&[&tr("Nicht ändern"), &tr("Fehler beim Laden: {err}").replace("{err}", &err)])));
                author_row.set_selected(0);
                author_populating.set(false);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                author_row.set_model(Some(&gtk4::StringList::new(&[&tr("Nicht ändern"), &tr("Interner Fehler: kein Ergebnis vom Ladevorgang.")])));
                author_row.set_selected(0);
                author_populating.set(false);
                glib::ControlFlow::Break
            }
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let author_options = author_options.clone();
        let author_populating = author_populating.clone();
        author_row.connect_selected_notify(move |row| {
            if author_populating.get() {
                return;
            }
            let mut fm = frontmatter.borrow_mut();
            match row.selected().checked_sub(1).and_then(|index| author_options.borrow().get(index as usize).cloned()) {
                Some(user) => {
                    fm.author_id = Some(user.id);
                    fm.author_name = Some(user.name);
                }
                None => {
                    fm.author_id = None;
                    fm.author_name = None;
                }
            }
        });
    }

    // Sent unconditionally on export as the "Worthy" WordPress plugin's own
    // `wp-worthy-pixel.ignored` REST field (see `export.rs`) - harmless on
    // a site without that plugin. Active (the default) means "tracked as
    // normal"; see the Statistik tab for how close the article already is
    // to Worthy's own minimum length before a counting pixel is even
    // eligible to report at all.
    let vgwort_row = adw::SwitchRow::builder()
        .title(tr("VG-Wort-Zählmarke"))
        .subtitle(tr("Aus lassen, um diesen Artikel bewusst von der VG-Wort-Zählung auszuschließen."))
        .active(!current.vgwort_ignored)
        .build();
    {
        let frontmatter = frontmatter.clone();
        vgwort_row.connect_active_notify(move |row| {
            frontmatter.borrow_mut().vgwort_ignored = !row.is_active();
        });
    }

    let refresh_button = gtk4::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some(&tr("Kategorien & Tags von WordPress aktualisieren")));
    refresh_button.add_css_class("flat");
    let term_caches = termcache::TermCacheHandles {
        categories: category_terms.clone(),
        tags: tag_terms.clone(),
        category_slugs: category_slugs.clone(),
    };
    {
        let term_caches = term_caches.clone();
        refresh_button.connect_clicked(move |_| {
            termcache::spawn_refresh(&term_caches);
        });
    }

    let manage_terms_button = gtk4::Button::from_icon_name("document-edit-symbolic");
    manage_terms_button.set_tooltip_text(Some(&tr("Kategorien & Tags verwalten…")));
    manage_terms_button.add_css_class("flat");
    {
        let parent = parent.clone();
        let term_caches = term_caches.clone();
        manage_terms_button.connect_clicked(move |_| {
            taxonomy::open(&parent, term_caches.clone());
        });
    }

    let header_suffix_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_suffix_box.append(&manage_terms_button);
    header_suffix_box.append(&refresh_button);

    let group = adw::PreferencesGroup::builder().title(tr("Artikel-Eigenschaften")).build();
    group.set_header_suffix(Some(&header_suffix_box));
    group.add(&title_row);
    group.add(&slug_row);
    group.add(&url_length_row);
    group.add(&excerpt_row);
    group.add(&status_row);
    group.add(&scheduled_row);
    group.add(&author_row);
    group.add(&vgwort_row);
    group.add(&categories_row);
    group.add(&tags_row);
    group.add(&featured_image_row);
    group.add(&featured_image_alt_row);

    autocomplete::attach(&categories_row, category_terms);
    autocomplete::attach(&tags_row, tag_terms.clone());

    // A separate group, not just more rows in "Artikel-Eigenschaften": these
    // three only matter on a site that actually has RankMath active (see
    // `Frontmatter::rank_math_title`'s doc comment), so keeping them
    // visually distinct signals that up front rather than implying they're
    // as universally applicable as the fields above.
    let seo_group = adw::PreferencesGroup::builder().title(tr("RankMath SEO")).build();
    seo_group.add(&seo_title_row);
    seo_group.add(&seo_description_row);
    seo_group.add(&focus_keyword_row);

    let groups_box = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(24).build();
    groups_box.append(&group);
    groups_box.append(&seo_group);

    let clamp = adw::Clamp::builder().maximum_size(480).child(&groups_box).build();
    let scroller = gtk4::ScrolledWindow::builder().child(&clamp).vexpand(true).build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title(tr("Artikel-Eigenschaften"))
        .content_width(480)
        .content_height(520)
        .child(&toolbar_view)
        .build();

    {
        let frontmatter = frontmatter.clone();
        title_row.connect_changed(move |row| {
            frontmatter.borrow_mut().title = row.text().to_string();
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let url_length_row = url_length_row.clone();
        let url_length_icon = url_length_icon.clone();
        let domain = site.url.clone();
        let category_slugs = category_slugs.clone();
        slug_row.connect_changed(move |row| {
            frontmatter.borrow_mut().slug = row.text().to_string();
            refresh_url_length_row(&url_length_row, &url_length_icon, &domain, &frontmatter, &category_slugs);
        });
    }
    {
        let title_row = title_row.clone();
        let slug_row = slug_row.clone();
        slug_generate_button.connect_clicked(move |_| {
            // Triggers the `connect_changed` handler above, which persists it.
            slug_row.set_text(&document::slugify(&title_row.text()));
        });
    }
    {
        let frontmatter = frontmatter.clone();
        excerpt_row.connect_changed(move |row| {
            let text = row.text().to_string();
            frontmatter.borrow_mut().excerpt = (!text.is_empty()).then_some(text);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        seo_title_row.connect_changed(move |row| {
            let text = row.text().to_string();
            frontmatter.borrow_mut().rank_math_title = (!text.is_empty()).then_some(text);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        seo_description_row.connect_changed(move |row| {
            let text = row.text().to_string();
            frontmatter.borrow_mut().rank_math_description = (!text.is_empty()).then_some(text);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        focus_keyword_row.connect_changed(move |row| {
            let text = row.text().to_string();
            frontmatter.borrow_mut().rank_math_focus_keyword = (!text.is_empty()).then_some(text);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let url_length_row = url_length_row.clone();
        let url_length_icon = url_length_icon.clone();
        let domain = site.url.clone();
        let category_slugs = category_slugs.clone();
        categories_row.connect_changed(move |row| {
            frontmatter.borrow_mut().categories = parse_list(&row.text());
            refresh_url_length_row(&url_length_row, &url_length_icon, &domain, &frontmatter, &category_slugs);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        tags_row.connect_changed(move |row| {
            frontmatter.borrow_mut().tags = parse_list(&row.text());
        });
    }
    {
        let parent = parent.clone();
        let body = body.clone();
        let title_row = title_row.clone();
        let tags_row = tags_row.clone();
        let tag_terms = tag_terms.clone();
        tags_suggest_button.connect_clicked(move |_| {
            let article_title = title_row.text().to_string();
            let current_tags = parse_list(&tags_row.text());
            let existing_tags = tag_terms.borrow().clone();
            let tags_row = tags_row.clone();
            tagsuggest::open(parent.upcast_ref::<gtk4::Window>(), article_title, body.clone(), existing_tags, current_tags, move |accepted| {
                let mut tags = parse_list(&tags_row.text());
                for tag in accepted {
                    if !tags.iter().any(|existing| existing.eq_ignore_ascii_case(&tag)) {
                        tags.push(tag);
                    }
                }
                // Triggers the `connect_changed` handler above, which persists it.
                tags_row.set_text(&tags.join(", "));
            });
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let featured_image_ai_button = featured_image_ai_button.clone();
        featured_image_row.connect_changed(move |row| {
            let text = row.text().to_string();
            featured_image_ai_button.set_visible(!text.is_empty());
            frontmatter.borrow_mut().featured_image = (!text.is_empty()).then_some(text);
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let featured_image_alt_length_warning_icon = featured_image_alt_length_warning_icon.clone();
        featured_image_alt_row.connect_changed(move |row| {
            let text = row.text().to_string();
            update_alt_length_warning(&featured_image_alt_length_warning_icon, &text);
            frontmatter.borrow_mut().featured_image_alt = (!text.is_empty()).then_some(text);
        });
    }
    {
        let parent = parent.clone();
        let frontmatter = frontmatter.clone();
        let doc_dir = doc_dir.clone();
        let featured_image_alt_row = featured_image_alt_row.clone();
        featured_image_ai_button.connect_clicked(move |_| {
            let Some(source) = frontmatter.borrow().featured_image.clone() else { return };
            let featured_image_alt_row = featured_image_alt_row.clone();
            aialt::open(parent.upcast_ref::<gtk4::Window>(), tr("Aufmacherbild"), source, doc_dir.clone(), move |text| {
                // Triggers the `connect_changed` handler above, which
                // persists it and refreshes the length-warning icon - same
                // "set text, let the existing handler do the rest" pattern
                // `featured_image_picker_button`'s own callback uses.
                featured_image_alt_row.set_text(&text);
            });
        });
    }
    {
        let parent = parent.clone();
        let featured_image_row = featured_image_row.clone();
        featured_image_picker_button.connect_clicked(move |_| {
            let filter = gtk4::FileFilter::new();
            filter.add_mime_type("image/*");
            filter.set_name(Some(&tr("Bilder")));
            let filters = gio::ListStore::new::<gtk4::FileFilter>();
            filters.append(&filter);

            let file_dialog = gtk4::FileDialog::builder().title(tr("Featured Image auswählen")).filters(&filters).build();

            let featured_image_row = featured_image_row.clone();
            let doc_dir = doc_dir.clone();
            file_dialog.open(Some(&parent), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                let reference = document::image_reference(&path, doc_dir.as_deref());
                // Triggers the `connect_changed` handler above, which persists it.
                featured_image_row.set_text(&reference);
            });
        });
    }
    {
        let frontmatter = frontmatter.clone();
        let scheduled_row = scheduled_row.clone();
        status_row.connect_selected_notify(move |row| {
            if let Some(status) = PostStatus::ALL.get(row.selected() as usize) {
                frontmatter.borrow_mut().status = *status;
                scheduled_row.set_visible(*status == PostStatus::Future);
            }
        });
    }
    {
        let frontmatter = frontmatter.clone();
        scheduled_row.connect_changed(move |row| {
            let text = row.text().to_string();
            let parsed = document::parse_scheduled_at(&text);
            row.remove_css_class("error");
            if !text.trim().is_empty() && parsed.is_none() {
                row.add_css_class("error");
            }
            frontmatter.borrow_mut().scheduled_at = parsed;
        });
    }

    refresh_url_length_row(&url_length_row, &url_length_icon, &site.url, &frontmatter, &category_slugs);

    // The preview's magazine-style header shows exactly these fields, but
    // isn't refreshed live on every keystroke here (unlike this dialog's
    // own rows, which all write straight into the shared `Frontmatter`) -
    // deferred to close, matching `imagealt.rs`'s own "write back on
    // `connect_closed`, not per keystroke" reasoning for its dialog.
    dialog.connect_closed(move |_| {
        preview_pane.set_article_header(&frontmatter.borrow());
    });

    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seo_url_preview_joins_domain_category_and_slug() {
        let (url, length) = seo_url_preview("https://linuxundich.de", Some("gnu-linux"), "mein-testartikel");
        assert_eq!(url, "https://linuxundich.de/gnu-linux/mein-testartikel/");
        assert_eq!(length, url.chars().count());
    }

    #[test]
    fn seo_url_preview_omits_the_category_segment_when_none_is_set() {
        let (url, _) = seo_url_preview("https://linuxundich.de", None, "mein-testartikel");
        assert_eq!(url, "https://linuxundich.de/mein-testartikel/");
    }

    #[test]
    fn seo_url_preview_strips_a_trailing_slash_from_the_domain() {
        let (url, _) = seo_url_preview("https://linuxundich.de/", Some("news"), "post");
        assert_eq!(url, "https://linuxundich.de/news/post/");
    }
}
