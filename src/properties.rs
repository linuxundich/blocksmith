//! "Artikel-Eigenschaften" dialog: edits the document's frontmatter
//! (title/slug/status/categories/tags/featured image) in place. Changes are
//! synced live into the shared `Frontmatter` cell as the user types, mirroring
//! how GNOME preferences dialogs apply immediately without an OK button.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::document::{parse_list, Frontmatter, PostStatus};
use crate::{autocomplete, termcache};

pub fn open(
    parent: &adw::ApplicationWindow,
    frontmatter: Rc<RefCell<Frontmatter>>,
    category_terms: Rc<RefCell<Vec<String>>>,
    tag_terms: Rc<RefCell<Vec<String>>>,
) {
    let current = frontmatter.borrow().clone();

    let title_row = adw::EntryRow::builder().title("Titel").text(current.title.as_str()).build();
    let slug_row = adw::EntryRow::builder().title("Slug").text(current.slug.as_str()).build();
    let categories_row = adw::EntryRow::builder()
        .title("Kategorien (Komma-getrennt)")
        .text(current.categories.join(", ").as_str())
        .build();
    let tags_row = adw::EntryRow::builder()
        .title("Tags (Komma-getrennt)")
        .text(current.tags.join(", ").as_str())
        .build();
    let featured_image_row = adw::EntryRow::builder()
        .title("Featured Image (Pfad)")
        .text(current.featured_image.clone().unwrap_or_default().as_str())
        .build();

    let status_labels: Vec<&str> = PostStatus::ALL.iter().map(|s| s.label()).collect();
    let status_model = gtk4::StringList::new(&status_labels);
    let selected_index = PostStatus::ALL.iter().position(|s| *s == current.status).unwrap_or(0);
    let status_row = adw::ComboRow::builder()
        .title("Status")
        .model(&status_model)
        .selected(selected_index as u32)
        .build();

    let refresh_button = gtk4::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Kategorien & Tags von WordPress aktualisieren"));
    refresh_button.add_css_class("flat");
    {
        let category_terms = category_terms.clone();
        let tag_terms = tag_terms.clone();
        refresh_button.connect_clicked(move |_| {
            termcache::spawn_refresh(category_terms.clone(), tag_terms.clone());
        });
    }

    let group = adw::PreferencesGroup::builder().title("Artikel-Eigenschaften").build();
    group.set_header_suffix(Some(&refresh_button));
    group.add(&title_row);
    group.add(&slug_row);
    group.add(&status_row);
    group.add(&categories_row);
    group.add(&tags_row);
    group.add(&featured_image_row);

    autocomplete::attach(&categories_row, category_terms);
    autocomplete::attach(&tags_row, tag_terms);

    let clamp = adw::Clamp::builder().maximum_size(480).child(&group).build();
    let scroller = gtk4::ScrolledWindow::builder().child(&clamp).vexpand(true).build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title("Artikel-Eigenschaften")
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
        let frontmatter = frontmatter.clone();
        status_row.connect_selected_notify(move |row| {
            if let Some(status) = PostStatus::ALL.get(row.selected() as usize) {
                frontmatter.borrow_mut().status = *status;
            }
        });
    }

    dialog.present(Some(parent));
}
