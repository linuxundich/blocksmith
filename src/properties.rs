//! "Artikel-Eigenschaften" dialog: edits the document's frontmatter
//! (title/slug/status/categories/tags/featured image) in place. Changes are
//! synced live into the shared `Frontmatter` cell as the user types, mirroring
//! how GNOME preferences dialogs apply immediately without an OK button.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::gio;

use crate::document::{self, parse_list, Frontmatter, PostStatus};
use crate::i18n::tr;
use crate::{autocomplete, taxonomy, termcache};

pub fn open(
    parent: &adw::ApplicationWindow,
    frontmatter: Rc<RefCell<Frontmatter>>,
    category_terms: Rc<RefCell<Vec<String>>>,
    tag_terms: Rc<RefCell<Vec<String>>>,
    doc_dir: Option<PathBuf>,
) {
    let current = frontmatter.borrow().clone();

    let title_row = adw::EntryRow::builder().title(tr("Titel")).text(current.title.as_str()).build();
    let slug_row = adw::EntryRow::builder().title(tr("Slug")).text(current.slug.as_str()).build();
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
    let featured_image_row = adw::EntryRow::builder()
        .title(tr("Featured Image (Pfad)"))
        .text(current.featured_image.clone().unwrap_or_default().as_str())
        .build();
    let featured_image_picker_button = gtk4::Button::from_icon_name("insert-image-symbolic");
    featured_image_picker_button.set_tooltip_text(Some(&tr("Bild auswählen…")));
    featured_image_picker_button.set_valign(gtk4::Align::Center);
    featured_image_picker_button.add_css_class("flat");
    featured_image_row.add_suffix(&featured_image_picker_button);

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

    let refresh_button = gtk4::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some(&tr("Kategorien & Tags von WordPress aktualisieren")));
    refresh_button.add_css_class("flat");
    {
        let category_terms = category_terms.clone();
        let tag_terms = tag_terms.clone();
        refresh_button.connect_clicked(move |_| {
            termcache::spawn_refresh(category_terms.clone(), tag_terms.clone());
        });
    }

    let manage_terms_button = gtk4::Button::from_icon_name("document-edit-symbolic");
    manage_terms_button.set_tooltip_text(Some(&tr("Kategorien & Tags verwalten…")));
    manage_terms_button.add_css_class("flat");
    {
        let parent = parent.clone();
        let category_terms = category_terms.clone();
        let tag_terms = tag_terms.clone();
        manage_terms_button.connect_clicked(move |_| {
            taxonomy::open(&parent, category_terms.clone(), tag_terms.clone());
        });
    }

    let header_suffix_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_suffix_box.append(&manage_terms_button);
    header_suffix_box.append(&refresh_button);

    let group = adw::PreferencesGroup::builder().title(tr("Artikel-Eigenschaften")).build();
    group.set_header_suffix(Some(&header_suffix_box));
    group.add(&title_row);
    group.add(&slug_row);
    group.add(&excerpt_row);
    group.add(&status_row);
    group.add(&scheduled_row);
    group.add(&categories_row);
    group.add(&tags_row);
    group.add(&featured_image_row);

    autocomplete::attach(&categories_row, category_terms);
    autocomplete::attach(&tags_row, tag_terms);

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
        slug_row.connect_changed(move |row| {
            frontmatter.borrow_mut().slug = row.text().to_string();
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
        categories_row.connect_changed(move |row| {
            frontmatter.borrow_mut().categories = parse_list(&row.text());
        });
    }
    {
        let frontmatter = frontmatter.clone();
        tags_row.connect_changed(move |row| {
            frontmatter.borrow_mut().tags = parse_list(&row.text());
        });
    }
    {
        let frontmatter = frontmatter.clone();
        featured_image_row.connect_changed(move |row| {
            let text = row.text().to_string();
            frontmatter.borrow_mut().featured_image = (!text.is_empty()).then_some(text);
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

    dialog.present(Some(parent));
}
