//! "Von WordPress öffnen" dialog: lists existing posts on the configured
//! site; picking one fetches its full content (raw Gutenberg block HTML),
//! resolves its category/tag ids back to names, converts the content back
//! to Markdown (`gutenberg::gutenberg_to_markdown`), and hands the result
//! to the caller to populate the editor - `window.rs` owns what happens
//! with that (filling the buffer, frontmatter, clearing `current_path`
//! since there's no local file yet).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::glib;

use crate::document::{Frontmatter, PostStatus};
use crate::{secrets, wpclient, wpsite};

pub struct ImportedPost {
    pub frontmatter: Frontmatter,
    pub body: String,
}

/// One status-grouped section of the post list ("Entwürfe"/"Veröffentlicht"/
/// "Weitere") - a heading plus its own boxed-list, hidden entirely while
/// its bucket is empty (e.g. no drafts exist).
#[derive(Clone)]
struct PostGroup {
    wrap: gtk4::Box,
    list_box: gtk4::ListBox,
    posts: Rc<RefCell<Vec<wpclient::PostSummary>>>,
}

fn build_post_group(title: &str) -> PostGroup {
    let heading = gtk4::Label::builder().label(title).xalign(0.0).build();
    heading.add_css_class("heading");

    let list_box = gtk4::ListBox::new();
    list_box.add_css_class("boxed-list");

    let wrap = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(6).build();
    wrap.append(&heading);
    wrap.append(&list_box);
    wrap.set_visible(false);

    PostGroup { wrap, list_box, posts: Rc::new(RefCell::new(Vec::new())) }
}

/// Wires one group's row activation - shared logic (fetch, convert, hand
/// off to the caller) between the "Entwürfe"/"Veröffentlicht"/"Weitere"
/// sections, `all_list_boxes` being every section's list box so all three
/// get disabled together while a post is loading, not just the one it was
/// picked from.
fn wire_group_row_activation(
    group: &PostGroup,
    site: wpsite::SiteConfig,
    status_label: gtk4::Label,
    on_selected: Rc<dyn Fn(ImportedPost)>,
    dialog_weak: glib::WeakRef<adw::Dialog>,
    all_list_boxes: Vec<gtk4::ListBox>,
) {
    let posts = group.posts.clone();
    group.list_box.connect_row_activated(move |_list_box, row| {
        let Some(post) = posts.borrow().get(row.index() as usize).cloned() else {
            return;
        };
        for list_box in &all_list_boxes {
            list_box.set_sensitive(false);
        }
        status_label.set_label(&format!("Lade „{}“ …", post.title));

        let site = site.clone();
        let (tx, rx) = mpsc::channel::<Result<ImportedPost, String>>();
        std::thread::spawn(move || {
            let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
                .map_err(|err| err.to_string())
                .and_then(|maybe_password| {
                    maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
                })
                .and_then(|password| fetch_and_convert(&site, &password, post.id));
            let _ = tx.send(outcome);
        });

        let status_label = status_label.clone();
        let all_list_boxes = all_list_boxes.clone();
        let on_selected = on_selected.clone();
        let dialog_weak = dialog_weak.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
            Ok(Ok(imported)) => {
                on_selected(imported);
                if let Some(dialog) = dialog_weak.upgrade() {
                    dialog.close();
                }
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                status_label.set_label(&format!("Fehler: {err}"));
                for list_box in &all_list_boxes {
                    list_box.set_sensitive(true);
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                status_label.set_label("Interner Fehler: Ladevorgang hat kein Ergebnis geliefert.");
                for list_box in &all_list_boxes {
                    list_box.set_sensitive(true);
                }
                glib::ControlFlow::Break
            }
        });
    });
}

pub fn open(parent: &adw::ApplicationWindow, on_selected: impl Fn(ImportedPost) + 'static) {
    let site = wpsite::load();

    let status_label = gtk4::Label::new(None);
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);

    // Drafts first - that's what the user is most likely mid-way through
    // and looking for - then published, then anything else (pending
    // review, scheduled, private) in a catch-all last section.
    let drafts_group = build_post_group("Entwürfe");
    let published_group = build_post_group("Veröffentlicht");
    let other_group = build_post_group("Weitere");

    let lists_container = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).spacing(18).build();
    lists_container.append(&drafts_group.wrap);
    lists_container.append(&published_group.wrap);
    lists_container.append(&other_group.wrap);

    let list_scroller = gtk4::ScrolledWindow::builder().child(&lists_container).vexpand(true).min_content_height(360).build();

    let refresh_button = gtk4::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Aktualisieren"));

    let header = adw::HeaderBar::new();
    header.pack_end(&refresh_button);

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content_box.append(&status_label);
    content_box.append(&list_scroller);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content_box));

    let dialog = adw::Dialog::builder()
        .title("Von WordPress öffnen")
        .content_width(560)
        .content_height(560)
        .child(&toolbar_view)
        .build();

    if site.url.is_empty() {
        status_label.set_label("Keine WordPress-Verbindung eingerichtet - bitte zuerst in den Einstellungen konfigurieren.");
        dialog.present(Some(parent));
        return;
    }

    let all_list_boxes = vec![drafts_group.list_box.clone(), published_group.list_box.clone(), other_group.list_box.clone()];

    load_posts(site.clone(), &drafts_group, &published_group, &other_group, status_label.clone());
    {
        let site = site.clone();
        let drafts_group = drafts_group.clone();
        let published_group = published_group.clone();
        let other_group = other_group.clone();
        let status_label = status_label.clone();
        refresh_button.connect_clicked(move |_| {
            load_posts(site.clone(), &drafts_group, &published_group, &other_group, status_label.clone());
        });
    }

    let on_selected: Rc<dyn Fn(ImportedPost)> = Rc::new(on_selected);
    let dialog_weak = dialog.downgrade();
    wire_group_row_activation(&drafts_group, site.clone(), status_label.clone(), on_selected.clone(), dialog_weak.clone(), all_list_boxes.clone());
    wire_group_row_activation(&published_group, site.clone(), status_label.clone(), on_selected.clone(), dialog_weak.clone(), all_list_boxes.clone());
    wire_group_row_activation(&other_group, site, status_label, on_selected, dialog_weak, all_list_boxes);

    dialog.present(Some(parent));
}

fn load_posts(
    site: wpsite::SiteConfig,
    drafts_group: &PostGroup,
    published_group: &PostGroup,
    other_group: &PostGroup,
    status_label: gtk4::Label,
) {
    status_label.set_label("Lade Artikel …");
    let (tx, rx) = mpsc::channel::<Result<Vec<wpclient::PostSummary>, String>>();
    std::thread::spawn(move || {
        let outcome = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .map_err(|err| err.to_string())
            .and_then(|maybe_password| {
                maybe_password.ok_or_else(|| "Kein Application Password im Schlüsselbund gefunden.".to_string())
            })
            .and_then(|password| wpclient::Client::new(&site.url, &site.username, &password).list_posts().map_err(|err| err.to_string()));
        let _ = tx.send(outcome);
    });

    let drafts_list_box = drafts_group.list_box.clone();
    let drafts_wrap = drafts_group.wrap.clone();
    let drafts_posts = drafts_group.posts.clone();
    let published_list_box = published_group.list_box.clone();
    let published_wrap = published_group.wrap.clone();
    let published_posts = published_group.posts.clone();
    let other_list_box = other_group.list_box.clone();
    let other_wrap = other_group.wrap.clone();
    let other_posts = other_group.posts.clone();

    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(fetched)) => {
            let mut drafts = Vec::new();
            let mut published = Vec::new();
            let mut other = Vec::new();
            for post in fetched {
                match post.status.as_str() {
                    "draft" => drafts.push(post),
                    "publish" => published.push(post),
                    _ => other.push(post),
                }
            }
            let total = drafts.len() + published.len() + other.len();

            populate_group(&drafts_list_box, &drafts_wrap, &drafts, false);
            populate_group(&published_list_box, &published_wrap, &published, false);
            populate_group(&other_list_box, &other_wrap, &other, true);

            *drafts_posts.borrow_mut() = drafts;
            *published_posts.borrow_mut() = published;
            *other_posts.borrow_mut() = other;

            status_label.set_label(&format!("{total} Artikel gefunden. Zum Öffnen auswählen."));
            glib::ControlFlow::Break
        }
        Ok(Err(err)) => {
            status_label.set_label(&format!("Fehler beim Laden: {err}"));
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            status_label.set_label("Interner Fehler: Lade-Thread hat kein Ergebnis geliefert.");
            glib::ControlFlow::Break
        }
    });
}

/// Rebuilds one group's rows from `posts` and shows/hides the whole group
/// depending on whether it has anything to show. `show_status` includes
/// the status word in each row's subtitle (used for the catch-all "Weitere"
/// group, which mixes several statuses) - the "Entwürfe"/"Veröffentlicht"
/// groups don't need it, since their heading already says which.
fn populate_group(list_box: &gtk4::ListBox, wrap: &gtk4::Box, posts: &[wpclient::PostSummary], show_status: bool) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    for post in posts {
        let date = post.date.split('T').next().unwrap_or(&post.date);
        let subtitle = if show_status { format!("{} · {date}", status_display(&post.status)) } else { date.to_string() };
        let row = adw::ActionRow::builder().title(glib::markup_escape_text(&post.title).as_str()).subtitle(subtitle).activatable(true).build();
        list_box.append(&row);
    }
    wrap.set_visible(!posts.is_empty());
}

fn status_display(status: &str) -> &str {
    match status {
        "publish" => "Veröffentlicht",
        "draft" => "Entwurf",
        "pending" => "Ausstehend",
        "future" => "Geplant",
        "private" => "Privat",
        other => other,
    }
}

fn fetch_and_convert(site: &wpsite::SiteConfig, password: &str, post_id: u64) -> Result<ImportedPost, String> {
    let client = wpclient::Client::new(&site.url, &site.username, password);
    let detail = client.get_post(post_id).map_err(|err| err.to_string())?;

    let mut categories = Vec::new();
    for id in &detail.categories {
        categories.push(client.get_term_name("categories", *id).map_err(|err| err.to_string())?);
    }
    let mut tags = Vec::new();
    for id in &detail.tags {
        tags.push(client.get_term_name("tags", *id).map_err(|err| err.to_string())?);
    }

    let body = gutenberg::gutenberg_to_markdown(&detail.content);
    let frontmatter = Frontmatter {
        title: detail.title,
        slug: detail.slug,
        status: PostStatus::from_str(&detail.status),
        categories,
        tags,
        featured_image: None,
        wp_post_id: Some(detail.id),
        featured_media_id: (detail.featured_media != 0).then_some(detail.featured_media),
        media: crate::media::reconcile(&[], &body),
    };

    Ok(ImportedPost { frontmatter, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads an existing, real, already-published post that's known (as of
    /// writing) to carry categories, tags, AND a featured image - checking
    /// that `fetch_and_convert` actually surfaces all three, not just the
    /// content. Read-only (no mutation of the site), but pinned to a
    /// specific post id, so it'll need updating if that post ever changes
    /// categories/tags/featured image, or is deleted.
    #[test]
    #[ignore]
    fn fetch_and_convert_surfaces_categories_tags_and_featured_image() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");

        // "Rutile 0.2.2 – Eine moderne Alternative zu Tilix" on
        // linuxundich.de: category "GNU/Linux", tags including "Gnome", and
        // a featured image (media id 45270 as of writing).
        let imported = fetch_and_convert(&site, &password, 45269).expect("fetch_and_convert failed");

        assert!(!imported.frontmatter.categories.is_empty(), "expected at least one category, got none");
        assert!(!imported.frontmatter.tags.is_empty(), "expected at least one tag, got none");
        assert!(
            imported.frontmatter.tags.iter().any(|t| t == "Gnome"),
            "expected tag 'Gnome' among {:?}",
            imported.frontmatter.tags
        );
        assert!(imported.frontmatter.featured_media_id.is_some(), "expected a featured_media_id, got None");
    }

    /// Creates a real post on the configured WordPress site (content
    /// generated by our own forward converter, exercising most block
    /// types), fetches it back, and converts it to Markdown - checking
    /// that actual WordPress storage/serving of the content (not just our
    /// own in-memory forward+reverse conversion) round-trips cleanly.
    /// Ignored by default; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn open_existing_post_round_trips_through_real_wordpress() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = wpclient::Client::new(&site.url, &site.username, &password);

        let markdown = "# Rundreise-Test\n\n\
             Ein **fetter** Text mit [Link](https://example.com).\n\n\
             - eins\n- zwei\n\n\
             > Zitat\n\n\
             ```\ncode\n```\n\n\
             ![alt](https://example.com/x.png)\n\n\
             ---\n\n\
             | A | B |\n|---|---|\n| 1 | 2 |\n";
        let content = gutenberg::markdown_to_gutenberg(markdown);

        let created = client
            .create_post(&serde_json::json!({
                "title": "Blocksmith round-trip test",
                "content": content,
                "status": "draft",
            }))
            .expect("create_post failed");

        let imported = fetch_and_convert(&site, &password, created.id).expect("fetch_and_convert failed");

        assert_eq!(imported.frontmatter.title, "Blocksmith round-trip test");
        assert_eq!(imported.frontmatter.wp_post_id, Some(created.id));
        assert!(imported.body.contains("# Rundreise-Test"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("**fetter**"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("[Link](https://example.com)"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("- eins"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("> Zitat"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("```\ncode\n```"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("![alt](https://example.com/x.png)"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("---"), "body was:\n{}", imported.body);
        assert!(imported.body.contains("| A | B |"), "body was:\n{}", imported.body);

        client.delete_post(created.id).expect("cleanup delete_post failed");
    }

    #[test]
    #[ignore]
    fn list_posts_against_real_site() {
        let site = wpsite::load();
        assert!(!site.url.is_empty(), "no WordPress site configured (run the connection dialog first)");
        let password = futures_lite::future::block_on(secrets::load_app_password(&site.url, &site.username))
            .expect("keyring lookup failed")
            .expect("no application password stored for this site/user");
        let client = wpclient::Client::new(&site.url, &site.username, &password);

        let posts = client.list_posts().expect("list_posts failed");
        assert!(!posts.is_empty(), "expected at least one existing post on the real site");
        assert!(
            posts.iter().any(|p| p.link.starts_with("http")),
            "expected at least one post to carry a real permalink, got {:?}",
            posts.iter().map(|p| &p.link).collect::<Vec<_>>()
        );
    }
}
