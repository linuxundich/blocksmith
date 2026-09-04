//! The right-hand live preview pane: a `WebKit` view rendering plain HTML
//! from the Markdown source (not the Gutenberg block-comment HTML — that's
//! only generated at export time, see `gutenberg::markdown_to_gutenberg`).
//!
//! Each top-level block is wrapped in a `<div data-line="N">`, `N` being the
//! 1-indexed Markdown source line it starts on, so the editor can drive
//! scroll-sync by asking the preview to scroll a specific *source line*
//! into view rather than assuming a fixed proportion of the document - an
//! image is one source line but can be many times taller than a text line
//! once rendered, so a naive "scroll to the same percentage" would drift.
//!
//! The preview also follows the app's light/dark mode and offers a choice
//! of typographic styles ("Modern"/"Klassisch"/"Sepia") via a small picker
//! above the `WebView` - both are baked directly into the generated HTML's
//! `<style>` block per render, rather than relying on the page's own
//! `prefers-color-scheme` media query, since we already regenerate the
//! whole document on every change anyway.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib, pango};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use webkit6::prelude::*;

use crate::document::Frontmatter;
use crate::fontutil;
use crate::media::{self, MediaItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewStyle {
    Modern,
    Classic,
    Sepia,
}

impl PreviewStyle {
    pub const ALL: [PreviewStyle; 3] = [PreviewStyle::Modern, PreviewStyle::Classic, PreviewStyle::Sepia];

    fn id(&self) -> &'static str {
        match self {
            PreviewStyle::Modern => "modern",
            PreviewStyle::Classic => "classic",
            PreviewStyle::Sepia => "sepia",
        }
    }

    fn from_id(s: &str) -> Self {
        match s.trim() {
            "classic" => PreviewStyle::Classic,
            "sepia" => PreviewStyle::Sepia,
            _ => PreviewStyle::Modern,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PreviewStyle::Modern => "Modern",
            PreviewStyle::Classic => "Klassisch",
            PreviewStyle::Sepia => "Sepia",
        }
    }
}

fn config_dir() -> PathBuf {
    let mut dir = glib::user_config_dir();
    dir.push("blocksmith");
    dir
}

fn preview_style_path() -> PathBuf {
    let mut path = config_dir();
    path.push("preview_style.txt");
    path
}

fn load_preview_style() -> PreviewStyle {
    std::fs::read_to_string(preview_style_path()).ok().map(|s| PreviewStyle::from_id(&s)).unwrap_or(PreviewStyle::Modern)
}

fn save_preview_style(style: PreviewStyle) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(preview_style_path(), style.id());
}

fn preview_font_path() -> PathBuf {
    let mut path = config_dir();
    path.push("preview_font.txt");
    path
}

/// A saved Pango font description (e.g. `"Cantarell 11"`) if the user has
/// picked one - each `PreviewStyle`'s own font stays the default until
/// this is set, so choosing a style still looks like that style out of
/// the box.
fn load_preview_font_override() -> Option<String> {
    std::fs::read_to_string(preview_font_path()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn save_preview_font_override(desc: &str) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(preview_font_path(), desc);
}

fn reset_preview_font_override() {
    let _ = std::fs::remove_file(preview_font_path());
}

/// The "Vorschau" tab: a style picker above a `WebKit` view. Remembers the
/// last rendered Markdown so it can re-render immediately when the style
/// changes (picked in Einstellungen, see `appearance::build_page`) or the
/// app's light/dark mode flips, without needing the editor buffer passed
/// back in.
pub struct PreviewPane {
    pub widget: gtk4::Widget,
    web_view: webkit6::WebView,
    style: Rc<Cell<PreviewStyle>>,
    last_markdown: Rc<RefCell<String>>,
    /// The document's current per-image metadata (alt text, upload state),
    /// already reconciled against `last_markdown` by the caller - used to
    /// draw the upload-status/alt/format badges in each image's bottom-right
    /// corner (see `wrap_images_with_badges`).
    last_media: Rc<RefCell<Vec<MediaItem>>>,
    /// The current article's own directory, if it has one (an unsaved or
    /// WordPress-imported-but-not-yet-saved document has none) - passed to
    /// the `WebView` as its base URI so a relative `<img src="photo.png">`
    /// resolves against it, matching where "Bild einfügen"/Medienverwaltung
    /// already expect a local image to live (see
    /// `document::image_reference`). Without this the image reference is
    /// technically correct but the preview simply can't load it - a
    /// `WebView` has no other way to know which folder "here" means.
    doc_dir: Rc<RefCell<Option<PathBuf>>>,
}

impl PreviewPane {
    pub fn new() -> Self {
        let web_view = webkit6::WebView::new();
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);

        // This is a rendered article preview, not a browsing session - the
        // navigation-history items WebKit puts in its default context menu
        // (Zurück/Vor/Anhalten) never apply to anything here and would
        // just be confusing dead controls.
        web_view.connect_context_menu(|_web_view, context_menu, _hit_test_result| {
            for item in context_menu.items() {
                if matches!(item.stock_action(), webkit6::ContextMenuAction::GoBack | webkit6::ContextMenuAction::GoForward | webkit6::ContextMenuAction::Stop) {
                    context_menu.remove(&item);
                }
            }
            false
        });

        let style = Rc::new(Cell::new(load_preview_style()));
        let last_markdown: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let last_media: Rc<RefCell<Vec<MediaItem>>> = Rc::new(RefCell::new(Vec::new()));
        let doc_dir: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

        {
            let web_view = web_view.clone();
            let style = style.clone();
            let last_markdown = last_markdown.clone();
            let last_media = last_media.clone();
            let doc_dir = doc_dir.clone();
            adw::StyleManager::default().connect_dark_notify(move |style_manager| {
                let html = render_html(&last_markdown.borrow(), style.get(), style_manager.is_dark(), &last_media.borrow());
                web_view.load_html(&html, base_uri(doc_dir.borrow().as_deref()).as_deref());
            });
        }

        Self {
            widget: web_view.clone().upcast(),
            web_view,
            style,
            last_markdown,
            last_media,
            doc_dir,
        }
    }

    /// `media` should already be reconciled against `markdown` (see
    /// `media::reconcile`) - the caller owns the canonical `Frontmatter`,
    /// this pane only needs a snapshot to draw badges from.
    pub fn update(&self, markdown: &str, media: &[MediaItem]) {
        *self.last_markdown.borrow_mut() = markdown.to_string();
        *self.last_media.borrow_mut() = media.to_vec();
        self.rerender();
    }

    /// Re-renders with a fresh media snapshot without touching the last
    /// rendered Markdown - for callers that mutate `Frontmatter.media`
    /// directly (Medienverwaltung's upload button, the manual/AI alt-text
    /// dialogs) rather than by editing the article text, so the badges
    /// this pane draws don't go stale until the next keystroke happens to
    /// re-trigger `update`.
    pub fn refresh_media(&self, media: &[MediaItem]) {
        *self.last_media.borrow_mut() = media.to_vec();
        self.rerender();
    }

    /// Called whenever the article's own file location changes (opened,
    /// saved for the first time, reset by "Neu"/importing from WordPress) -
    /// re-renders immediately so a folder that just became known (or just
    /// stopped being known) takes effect right away, not only on the next
    /// edit.
    pub fn set_doc_dir(&self, doc_dir: Option<PathBuf>) {
        *self.doc_dir.borrow_mut() = doc_dir;
        self.rerender();
    }

    /// Adds a "KI-Alternativtext generieren…" item to the context menu when
    /// right-clicking directly on a rendered image - the preview-side entry
    /// point into `aialt::open` (the editor-side one is `imagealt.rs`,
    /// triggered from the image's `![alt](src)` line instead). A second
    /// `context-menu` handler alongside the one `new()` already installs
    /// (which only trims the default navigation items), since `frontmatter`
    /// isn't available yet at construction time - both handlers run against
    /// the same `ContextMenu` on every right-click.
    ///
    /// Takes `&Rc<Self>` rather than `&self` - `aialt::open` needs to hold
    /// onto this same pane (as an owned `Rc`) to refresh its badges once the
    /// dialog applies a new alt text, and a plain `&self` has no `Rc` of
    /// itself to hand out.
    pub fn install_ai_alt_text_menu(preview_pane: &Rc<Self>, window: &impl IsA<gtk4::Window>, frontmatter: Rc<RefCell<Frontmatter>>) {
        let window: gtk4::Window = window.clone().upcast();
        let doc_dir = preview_pane.doc_dir.clone();
        let last_markdown = preview_pane.last_markdown.clone();
        let preview_pane = preview_pane.clone();
        preview_pane.web_view.clone().connect_context_menu(move |_web_view, context_menu, hit_test_result| {
            if !hit_test_result.context_is_image() {
                return false;
            }
            let Some(image_uri) = hit_test_result.image_uri() else { return false };

            // Re-reconcile here (not just relying on whatever's already in
            // `frontmatter.media`) so a just-inserted image that hasn't been
            // through Medienverwaltung/the export dialog yet still gets a
            // working menu entry, not a silently-missing one.
            {
                let mut fm = frontmatter.borrow_mut();
                fm.media = media::reconcile(&fm.media, &last_markdown.borrow());
            }
            let doc_dir_value = doc_dir.borrow().clone();
            let Some(index) = item_index_for_image_uri(&frontmatter.borrow().media, &image_uri, doc_dir_value.as_deref()) else {
                return false;
            };

            let action = gio::SimpleAction::new("generate-ai-alt-text", None);
            {
                let frontmatter = frontmatter.clone();
                let window = window.clone();
                let doc_dir_value = doc_dir_value.clone();
                let preview_pane = preview_pane.clone();
                action.connect_activate(move |_, _| {
                    crate::aialt::open(&window, frontmatter.clone(), index, doc_dir_value.clone(), preview_pane.clone());
                });
            }
            let item = webkit6::ContextMenuItem::from_gaction(&action, "KI-Alternativtext generieren…", None);
            context_menu.append(&item);
            false
        });
    }

    pub fn scroll_to_line(&self, line: i32) {
        self.web_view.evaluate_javascript(&format!("window.scrollToLine && window.scrollToLine({line});"), None, None, gio::Cancellable::NONE, |_| {});
    }

    pub fn style(&self) -> PreviewStyle {
        self.style.get()
    }

    /// Called from the "Erscheinungsbild" settings page's style picker:
    /// persists the choice and re-renders immediately with the
    /// last-known Markdown, the same way an editor color-scheme change
    /// applies live without needing the dialog closed.
    pub fn set_style(&self, style: PreviewStyle) {
        self.style.set(style);
        save_preview_style(style);
        self.rerender();
    }

    /// The saved custom font, if any - `None` means each `PreviewStyle`
    /// still uses its own built-in font.
    pub fn font_override(&self) -> Option<String> {
        load_preview_font_override()
    }

    pub fn is_font_customized(&self) -> bool {
        load_preview_font_override().is_some()
    }

    pub fn set_font_override(&self, desc: &str) {
        save_preview_font_override(desc);
        self.rerender();
    }

    pub fn reset_font_override(&self) {
        reset_preview_font_override();
        self.rerender();
    }

    fn rerender(&self) {
        let dark = adw::StyleManager::default().is_dark();
        let html = render_html(&self.last_markdown.borrow(), self.style.get(), dark, &self.last_media.borrow());
        self.web_view.load_html(&html, base_uri(self.doc_dir.borrow().as_deref()).as_deref());
    }
}

/// A `file://` URI for `dir`, suitable as a `WebView` base URI - built via
/// `gio::File` rather than a hand-formatted `format!("file://{}", ...)` so
/// a directory path containing characters that need percent-encoding is
/// still handled correctly.
fn base_uri(dir: Option<&Path>) -> Option<String> {
    let dir = dir?;
    let uri = gio::File::for_path(dir).uri();
    Some(if uri.ends_with('/') { uri.to_string() } else { format!("{uri}/") })
}

/// Matches a WebKit-resolved image URI back to the `MediaItem` it renders.
/// A `file://` URI (the normal case for a locally-referenced image, since
/// the preview's base URI - see `base_uri` above - turns a relative
/// `![](photo.png)` into an absolute `file://` address before WebKit ever
/// sees it) is converted back to a plain path and compared against each
/// item's Markdown source resolved the same way `export::resolve_local_path`
/// resolves it for upload/hashing; anything else (a remote `http(s)://` URL,
/// `gio::File::path()` returns `None` for those) is compared directly, since
/// a remote source is never rewritten.
fn item_index_for_image_uri(items: &[MediaItem], image_uri: &str, doc_dir: Option<&Path>) -> Option<usize> {
    if let Some(path) = gio::File::for_uri(image_uri).path() {
        return items.iter().position(|item| crate::export::resolve_local_path(&item.source, doc_dir) == path);
    }
    items.iter().position(|item| item.source == image_uri)
}

/// One typographic style's CSS, in its light and dark variant. Baked
/// directly into the generated document (see the module docs for why),
/// not left to a `prefers-color-scheme` media query.
fn style_css(style: PreviewStyle, dark: bool) -> &'static str {
    match (style, dark) {
        (PreviewStyle::Modern, false) => {
            "body { font-family: -apple-system, Cantarell, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.6; color: #1e1e1e; background: #ffffff; }
             pre { background: #f2f2f2; padding: .75rem; border-radius: 6px; overflow-x: auto; }
             code { background: #f2f2f2; padding: .1rem .3rem; border-radius: 4px; }
             pre code { background: none; padding: 0; }
             blockquote { border-left: 4px solid #ccc; margin-left: 0; padding-left: 1rem; color: #555; }
             a { color: #1c71d8; }
             hr { border: none; border-top: 1px solid #ccc; }"
        }
        (PreviewStyle::Modern, true) => {
            "body { font-family: -apple-system, Cantarell, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.6; color: #e3e3e3; background: #1e1e1e; }
             pre { background: #2d2d2d; padding: .75rem; border-radius: 6px; overflow-x: auto; }
             code { background: #2d2d2d; padding: .1rem .3rem; border-radius: 4px; }
             pre code { background: none; padding: 0; }
             blockquote { border-left: 4px solid #555; margin-left: 0; padding-left: 1rem; color: #aaa; }
             a { color: #78aeed; }
             hr { border: none; border-top: 1px solid #444; }"
        }
        (PreviewStyle::Classic, false) => {
            "body { font-family: Georgia, 'Times New Roman', serif; max-width: 38rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.7; color: #222222; background: #ffffff; text-align: justify; }
             h1, h2, h3 { font-family: Georgia, serif; }
             p { text-indent: 1.5em; margin: 0 0 .2em 0; }
             blockquote { font-style: italic; border-left: 2px solid #999; padding-left: 1rem; color: #444; }
             pre { background: #f5f0e6; padding: .75rem; border: 1px solid #ddd; overflow-x: auto; text-indent: 0; }
             code { background: #f5f0e6; padding: .1rem .3rem; }
             pre code { background: none; padding: 0; }
             a { color: #8a3324; }
             hr { border: none; border-top: 1px double #999; margin: 2rem 0; }"
        }
        (PreviewStyle::Classic, true) => {
            "body { font-family: Georgia, 'Times New Roman', serif; max-width: 38rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.7; color: #dddddd; background: #181818; text-align: justify; }
             h1, h2, h3 { font-family: Georgia, serif; }
             p { text-indent: 1.5em; margin: 0 0 .2em 0; }
             blockquote { font-style: italic; border-left: 2px solid #666; padding-left: 1rem; color: #bbb; }
             pre { background: #242220; padding: .75rem; border: 1px solid #3a3a3a; overflow-x: auto; text-indent: 0; }
             code { background: #242220; padding: .1rem .3rem; }
             pre code { background: none; padding: 0; }
             a { color: #e0947e; }
             hr { border: none; border-top: 1px double #555; margin: 2rem 0; }"
        }
        (PreviewStyle::Sepia, false) => {
            "body { font-family: Georgia, serif; max-width: 40rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.7; color: #5b4636; background: #f4ecd8; }
             pre { background: #ece0c8; padding: .75rem; border-radius: 6px; overflow-x: auto; }
             code { background: #ece0c8; padding: .1rem .3rem; border-radius: 4px; }
             pre code { background: none; padding: 0; }
             blockquote { border-left: 4px solid #c8b78e; margin-left: 0; padding-left: 1rem; color: #7a6650; }
             a { color: #8a5a2b; }
             hr { border: none; border-top: 1px solid #c8b78e; }"
        }
        (PreviewStyle::Sepia, true) => {
            "body { font-family: Georgia, serif; max-width: 40rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.7; color: #d9c9a3; background: #2b2418; }
             pre { background: #3a3223; padding: .75rem; border-radius: 6px; overflow-x: auto; }
             code { background: #3a3223; padding: .1rem .3rem; border-radius: 4px; }
             pre code { background: none; padding: 0; }
             blockquote { border-left: 4px solid #5c4f34; margin-left: 0; padding-left: 1rem; color: #c2ab7e; }
             a { color: #d4a15f; }
             hr { border: none; border-top: 1px solid #5c4f34; }"
        }
    }
}

pub fn render_html(markdown: &str, style: PreviewStyle, dark: bool, media: &[MediaItem]) -> String {
    let body = render_body_with_line_anchors(markdown, media);
    let css = style_css(style, dark);
    // A user-picked font (if any) overrides just the two font properties,
    // applied after the style's own block so the cascade lets it win
    // while everything else the style defines (colors, indentation,
    // justification, ...) stays intact.
    let font_override_css = load_preview_font_override()
        .map(|desc| format!("body {{ {} }}", fontutil::css_declarations(&pango::FontDescription::from_string(&desc))))
        .unwrap_or_default();

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
{css}
{font_override_css}
img {{ max-width: 100%; }}
table {{ border-collapse: collapse; }}
th, td {{ border: 1px solid #ccc; padding: .4rem .6rem; }}
{BADGE_CSS}
</style></head><body>{body}<script>
window.scrollToLine = function(line) {{
  const blocks = document.querySelectorAll('[data-line]');
  let target = null;
  for (const b of blocks) {{
    if (parseInt(b.getAttribute('data-line'), 10) <= line) {{ target = b; }} else {{ break; }}
  }}
  if (target) {{ target.scrollIntoView({{block: 'start', behavior: 'auto'}}); }}
}};
</script></body></html>"#
    )
}

/// Renders the document as a sequence of `<div data-line="N">...</div>`
/// wrappers, one per top-level Markdown block, using pulldown-cmark's own
/// HTML renderer for each block's inner content so output stays consistent
/// with plain rendering.
fn render_body_with_line_anchors(markdown: &str, media: &[MediaItem]) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<(Event, std::ops::Range<usize>)> = Parser::new_ext(markdown, options).into_offset_iter().collect();

    let mut out = String::new();
    let mut i = 0;
    while i < events.len() {
        match &events[i].0 {
            Event::Start(Tag::CodeBlock(kind)) => {
                let kind = kind.clone();
                let end = find_matching_end(&events, i, &TagEnd::CodeBlock);
                let line = line_number(markdown, events[i].1.start);
                let inner = render_code_block_with_line_anchors(&events[i..=end], &kind, line);
                out.push_str(&format!("<div data-line=\"{line}\">{inner}</div>\n"));
                i = end + 1;
            }
            Event::Start(tag) => {
                let end_marker = tag.to_end();
                let end = find_matching_end(&events, i, &end_marker);
                let line = line_number(markdown, events[i].1.start);
                let mut inner = String::new();
                pulldown_cmark::html::push_html(&mut inner, events[i..=end].iter().map(|(event, _)| event.clone()));
                out.push_str(&format!("<div data-line=\"{line}\">{inner}</div>\n"));
                i = end + 1;
            }
            Event::Rule => {
                let line = line_number(markdown, events[i].1.start);
                out.push_str(&format!("<div data-line=\"{line}\"><hr/></div>\n"));
                i += 1;
            }
            _ => i += 1,
        }
    }
    wrap_images_with_badges(&out, media)
}

const BADGE_CSS: &str = ".img-wrap { position: relative; display: inline-block; max-width: 100%; }
.img-wrap img { display: block; }
.img-badges { position: absolute; right: 6px; bottom: 6px; display: flex; gap: 4px; }
.img-badge { background: rgba(0, 0, 0, 0.65); color: #fff; font: 11px/1.4 -apple-system, Cantarell, sans-serif; font-weight: 600; letter-spacing: .02em; padding: 2px 6px; border-radius: 4px; }";

/// Wraps every `<img ...>` tag in `html` with a `.img-wrap` container and,
/// when the image matches a tracked `MediaItem`, a bottom-right badge
/// cluster (see `badges_html`) - a lightweight string-level pass rather
/// than a full HTML parser dependency, matching this module's existing
/// preference for direct string manipulation over pulling in another
/// crate for something this narrow.
fn wrap_images_with_badges(html: &str, media: &[MediaItem]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<img ") {
        out.push_str(&rest[..start]);
        let Some(tag_end_rel) = rest[start..].find('>') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let tag_end = start + tag_end_rel + 1;
        let tag = &rest[start..tag_end];
        let src = extract_attr(tag, "src").map(|s| unescape_html_attr(&s)).unwrap_or_default();

        out.push_str("<span class=\"img-wrap\">");
        out.push_str(tag);
        out.push_str(&badges_html(&src, media));
        out.push_str("</span>");
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    out
}

/// Pulls `attr="value"` out of a single HTML tag's source text - pulldown-
/// cmark always quotes attribute values with `"`, so this doesn't need to
/// handle the unquoted/single-quoted forms a general HTML parser would.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = start + tag[start..].find('"')?;
    Some(tag[start..end].to_string())
}

/// Undoes pulldown-cmark's HTML-entity escaping of the `src` attribute, so
/// it can be compared against a plain `MediaItem.source` string again.
fn unescape_html_attr(value: &str) -> String {
    value.replace("&amp;", "&").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">")
}

/// The badge cluster for one image, in the fixed order Upload-Status → Alt
/// → Bildformat whenever more than one applies - empty (no wrapper span at
/// all) if none of the three apply, e.g. an image with no matching
/// `MediaItem` or an unrecognized/missing file extension.
fn badges_html(src: &str, media: &[MediaItem]) -> String {
    let Some(item) = media.iter().find(|item| item.source == src) else {
        return String::new();
    };

    let mut badges: Vec<(String, &str)> = Vec::new();
    if item.wordpress.is_some() {
        badges.push(("↑".to_string(), "Bereits zu WordPress hochgeladen"));
    }
    if !item.alt.is_undefined() {
        badges.push(("Alt".to_string(), "Alternativtext ist definiert"));
    }
    if let Some(format) = image_format_label(&item.filename) {
        badges.push((format, "Bildformat"));
    }
    if badges.is_empty() {
        return String::new();
    }

    let mut html = String::from("<span class=\"img-badges\">");
    for (label, title) in badges {
        html.push_str(&format!(
            "<span class=\"img-badge\" title=\"{}\">{}</span>",
            glib::markup_escape_text(title),
            glib::markup_escape_text(&label)
        ));
    }
    html.push_str("</span>");
    html
}

/// The uppercased file extension (`"cat.png"` → `"PNG"`), or `None` for a
/// filename with no extension to show at all.
fn image_format_label(filename: &str) -> Option<String> {
    if !filename.contains('.') {
        return None;
    }
    filename.rsplit('.').next().map(str::to_uppercase)
}

/// Fenced/indented code blocks can span dozens of lines - if the whole
/// block were a single scroll-sync anchor (like every other block type
/// gets), the preview would sit completely frozen while the editor
/// scrolls through it, only jumping once you scroll past the block
/// entirely. Each *line* of the block's content gets its own anchor
/// instead (nested `<span data-line="N">` inside the shared `<pre><code>`),
/// so `window.scrollToLine`'s "closest preceding data-line" search - which
/// already walks every `[data-line]` element in document order, not just
/// top-level blocks - keeps working smoothly inside long code samples too.
fn render_code_block_with_line_anchors(events: &[(Event, std::ops::Range<usize>)], kind: &CodeBlockKind, block_start_line: usize) -> String {
    let mut code = String::new();
    for (event, _) in events {
        if let Event::Text(text) = event {
            code.push_str(text);
        }
    }

    let lang_class = match kind {
        CodeBlockKind::Fenced(info) => info
            .split_whitespace()
            .next()
            .filter(|lang| !lang.is_empty())
            .map(|lang| format!(" class=\"language-{}\"", glib::markup_escape_text(lang))),
        CodeBlockKind::Indented => None,
    };

    let mut html = format!("<pre><code{}>", lang_class.unwrap_or_default());
    // pulldown-cmark's code content always ends with a trailing newline;
    // drop the empty element `split('\n')` would otherwise produce for it.
    for (index, line_text) in code.strip_suffix('\n').unwrap_or(&code).split('\n').enumerate() {
        let line = block_start_line + 1 + index;
        html.push_str(&format!("<span data-line=\"{line}\">{}</span>\n", glib::markup_escape_text(line_text)));
    }
    html.push_str("</code></pre>");
    html
}

fn line_number(markdown: &str, byte_offset: usize) -> usize {
    markdown[..byte_offset].matches('\n').count() + 1
}

/// Same `Tag::to_end()` depth-counting trick as `crates/gutenberg` uses -
/// duplicated rather than shared, since this module's concern (HTML with
/// line anchors for scroll-sync) is unrelated to that crate's (Gutenberg
/// block-comment conversion).
fn find_matching_end(events: &[(Event, std::ops::Range<usize>)], start: usize, end_marker: &TagEnd) -> usize {
    let mut depth = 0usize;
    let mut j = start;
    while j < events.len() {
        match &events[j].0 {
            Event::Start(t) if &t.to_end() == end_marker => depth += 1,
            Event::End(e) if e == end_marker => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
        j += 1;
    }
    events.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_paragraph_is_tagged_with_its_line() {
        let out = render_body_with_line_anchors("Hello world.\n", &[]);
        assert_eq!(out, "<div data-line=\"1\"><p>Hello world.</p>\n</div>\n");
    }

    #[test]
    fn blocks_separated_by_blank_lines_get_their_own_starting_line() {
        let markdown = "# Title\n\nSecond paragraph.\n\nThird paragraph.\n";
        let out = render_body_with_line_anchors(markdown, &[]);
        assert_eq!(
            out,
            "<div data-line=\"1\"><h1>Title</h1>\n</div>\n\
             <div data-line=\"3\"><p>Second paragraph.</p>\n</div>\n\
             <div data-line=\"5\"><p>Third paragraph.</p>\n</div>\n"
        );
    }

    #[test]
    fn image_only_line_is_tagged_with_its_single_source_line_despite_render_height() {
        // The whole point of anchoring by source line rather than by
        // proportion of total lines: this image is one line of Markdown,
        // but renders far taller than a text line - scroll-sync must still
        // key off "line 3", not some fraction of the document's line count.
        let markdown = "Intro text.\n\n![a cat](cat.png)\n\nOutro text.\n";
        let out = render_body_with_line_anchors(markdown, &[]);
        assert!(out.contains("<div data-line=\"1\"><p>Intro text.</p>"));
        assert!(out.contains("<div data-line=\"3\"><p><span class=\"img-wrap\"><img src=\"cat.png\" alt=\"a cat\""), "{out}");
        assert!(out.contains("<div data-line=\"5\"><p>Outro text.</p>"));
    }

    fn media_item(source: &str, filename: &str, alt: crate::media::AltText, uploaded: bool) -> MediaItem {
        MediaItem {
            id: "media-001".to_string(),
            filename: filename.to_string(),
            source: source.to_string(),
            alt,
            caption: None,
            wordpress: uploaded.then_some(crate::media::WordPressMediaRef {
                media_id: 1,
                url: "https://example.com/cat.png".to_string(),
                content_hash: "abc".to_string(),
            }),
        }
    }

    #[test]
    fn an_image_with_no_matching_media_item_gets_no_badges() {
        let markdown = "![a cat](cat.png)\n";
        let out = render_body_with_line_anchors(markdown, &[]);
        assert!(!out.contains("img-badges"), "{out}");
    }

    #[test]
    fn badges_appear_in_upload_alt_format_order_when_all_three_apply() {
        let item = media_item("cat.png", "cat.png", crate::media::AltText::Text("a cat".into()), true);
        let out = render_body_with_line_anchors("![a cat](cat.png)\n", std::slice::from_ref(&item));
        let badges_start = out.find("img-badges").expect("expected a badge cluster");
        let upload_pos = out.find('↑').expect("expected the upload badge");
        let alt_pos = out.find(">Alt<").expect("expected the alt badge");
        let format_pos = out.find(">PNG<").expect("expected the format badge");
        assert!(badges_start < upload_pos && upload_pos < alt_pos && alt_pos < format_pos, "{out}");
    }

    #[test]
    fn only_the_applicable_badges_are_shown() {
        let item = media_item("cat.png", "cat.png", crate::media::AltText::Undefined, false);
        let out = render_body_with_line_anchors("![a cat](cat.png)\n", std::slice::from_ref(&item));
        assert!(!out.contains('↑'), "{out}");
        assert!(!out.contains(">Alt<"), "{out}");
        assert!(out.contains(">PNG<"), "{out}");
    }

    #[test]
    fn deliberately_empty_alt_still_counts_as_defined() {
        let item = media_item("cat.png", "cat.png", crate::media::AltText::Empty, false);
        let out = render_body_with_line_anchors("![](cat.png)\n", std::slice::from_ref(&item));
        assert!(out.contains(">Alt<"), "{out}");
    }

    #[test]
    fn image_format_label_uppercases_the_extension() {
        assert_eq!(image_format_label("photo.webp"), Some("WEBP".to_string()));
        assert_eq!(image_format_label("photo.PNG"), Some("PNG".to_string()));
        assert_eq!(image_format_label("no-extension"), None);
    }

    #[test]
    fn thematic_break_is_tagged() {
        let out = render_body_with_line_anchors("Text.\n\n---\n\nMore text.\n", &[]);
        assert!(out.contains("<div data-line=\"3\"><hr/></div>"));
    }

    #[test]
    fn multiline_code_block_tags_each_line_individually() {
        // The whole point: a long fenced code block used to be one opaque
        // scroll-sync anchor, so scrolling through it in the editor never
        // moved the preview at all until you scrolled past the block
        // entirely. Each line inside it needs its own `data-line` now.
        let markdown = "Intro.\n\n```bash\nfirst\nsecond\nthird\n```\n\nOutro.\n";
        let out = render_body_with_line_anchors(markdown, &[]);
        assert!(out.contains("<div data-line=\"3\">"), "{out}");
        assert!(out.contains("<span data-line=\"4\">first</span>"), "{out}");
        assert!(out.contains("<span data-line=\"5\">second</span>"), "{out}");
        assert!(out.contains("<span data-line=\"6\">third</span>"), "{out}");
        assert!(out.contains("<div data-line=\"9\">"), "{out}");
    }

    #[test]
    fn fenced_code_block_language_becomes_a_css_class() {
        let out = render_body_with_line_anchors("```rust\nfn main() {}\n```\n", &[]);
        assert!(out.contains("class=\"language-rust\""), "{out}");
    }

    #[test]
    fn code_block_content_is_html_escaped() {
        let out = render_body_with_line_anchors("```\n<script>alert(1)</script>\n```\n", &[]);
        assert!(out.contains("&lt;script&gt;"), "{out}");
        assert!(!out.contains("<script>"), "{out}");
    }

    #[test]
    fn full_html_embeds_the_scroll_to_line_script() {
        let html = render_html("Hello", PreviewStyle::Modern, false, &[]);
        assert!(html.contains("window.scrollToLine = function(line)"));
        assert!(html.contains("data-line=\"1\""));
    }

    #[test]
    fn preview_style_id_round_trips() {
        for style in PreviewStyle::ALL {
            assert_eq!(PreviewStyle::from_id(style.id()), style);
        }
    }

    #[test]
    fn item_index_for_image_uri_matches_a_local_file_uri_back_to_its_source() {
        let dir = std::env::temp_dir().join(format!("blocksmith-preview-uri-match-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cat.png"), b"fake png bytes").unwrap();

        let items = vec![MediaItem {
            id: "media-001".to_string(),
            filename: "cat.png".to_string(),
            source: "cat.png".to_string(),
            alt: crate::media::AltText::Undefined,
            caption: None,
            wordpress: None,
        }];
        let uri = gio::File::for_path(dir.join("cat.png")).uri();

        assert_eq!(item_index_for_image_uri(&items, &uri, Some(&dir)), Some(0));
        assert_eq!(item_index_for_image_uri(&items, &uri, None), None, "no doc_dir means the source can't resolve to this path");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn item_index_for_image_uri_matches_a_remote_url_directly() {
        let items = vec![MediaItem {
            id: "media-001".to_string(),
            filename: "cat.png".to_string(),
            source: "https://example.com/cat.png".to_string(),
            alt: crate::media::AltText::Undefined,
            caption: None,
            wordpress: None,
        }];
        assert_eq!(item_index_for_image_uri(&items, "https://example.com/cat.png", None), Some(0));
        assert_eq!(item_index_for_image_uri(&items, "https://example.com/dog.png", None), None);
    }

    #[test]
    fn every_style_has_both_a_light_and_dark_variant() {
        for style in PreviewStyle::ALL {
            assert!(!style_css(style, false).is_empty());
            assert!(!style_css(style, true).is_empty());
            assert_ne!(style_css(style, false), style_css(style, true));
        }
    }
}
