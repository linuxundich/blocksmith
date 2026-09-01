//! The right-hand live preview pane: a `WebKit` view rendering plain HTML
//! from the Markdown source (not the Gutenberg block-comment HTML — that's
//! only generated at export time, see `gutenberg::markdown_to_gutenberg`).

use webkit6::prelude::*;

pub fn build() -> webkit6::WebView {
    let view = webkit6::WebView::new();
    view.set_hexpand(true);
    view.set_vexpand(true);
    view
}

pub fn render_html(markdown: &str) -> String {
    let mut body = String::new();
    let options = pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_TASKLISTS;
    let parser = pulldown_cmark::Parser::new_ext(markdown, options);
    pulldown_cmark::html::push_html(&mut body, parser);

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
body {{ font-family: -apple-system, Cantarell, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.6; color: #1e1e1e; }}
pre {{ background: #f2f2f2; padding: .75rem; border-radius: 6px; overflow-x: auto; }}
code {{ background: #f2f2f2; padding: .1rem .3rem; border-radius: 4px; }}
pre code {{ background: none; padding: 0; }}
blockquote {{ border-left: 4px solid #ccc; margin-left: 0; padding-left: 1rem; color: #555; }}
img {{ max-width: 100%; }}
table {{ border-collapse: collapse; }}
th, td {{ border: 1px solid #ccc; padding: .4rem .6rem; }}
hr {{ border: none; border-top: 1px solid #ccc; }}
</style></head><body>{body}</body></html>"#
    )
}
