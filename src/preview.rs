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

use pulldown_cmark::{Event, Options, Parser, TagEnd};
use webkit6::prelude::*;

pub fn build() -> webkit6::WebView {
    let view = webkit6::WebView::new();
    view.set_hexpand(true);
    view.set_vexpand(true);
    view
}

pub fn render_html(markdown: &str) -> String {
    let body = render_body_with_line_anchors(markdown);

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
fn render_body_with_line_anchors(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<(Event, std::ops::Range<usize>)> = Parser::new_ext(markdown, options).into_offset_iter().collect();

    let mut out = String::new();
    let mut i = 0;
    while i < events.len() {
        match &events[i].0 {
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
    out
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
        let out = render_body_with_line_anchors("Hello world.\n");
        assert_eq!(out, "<div data-line=\"1\"><p>Hello world.</p>\n</div>\n");
    }

    #[test]
    fn blocks_separated_by_blank_lines_get_their_own_starting_line() {
        let markdown = "# Title\n\nSecond paragraph.\n\nThird paragraph.\n";
        let out = render_body_with_line_anchors(markdown);
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
        let out = render_body_with_line_anchors(markdown);
        assert!(out.contains("<div data-line=\"1\"><p>Intro text.</p>"));
        assert!(out.contains("<div data-line=\"3\"><p><img src=\"cat.png\" alt=\"a cat\""));
        assert!(out.contains("<div data-line=\"5\"><p>Outro text.</p>"));
    }

    #[test]
    fn thematic_break_is_tagged() {
        let out = render_body_with_line_anchors("Text.\n\n---\n\nMore text.\n");
        assert!(out.contains("<div data-line=\"3\"><hr/></div>"));
    }

    #[test]
    fn full_html_embeds_the_scroll_to_line_script() {
        let html = render_html("Hello");
        assert!(html.contains("window.scrollToLine = function(line)"));
        assert!(html.contains("data-line=\"1\""));
    }
}
