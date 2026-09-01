//! Converts Markdown into WordPress Gutenberg block-comment HTML.
//!
//! Pipeline: `pulldown-cmark` event stream -> [`Block`] tree -> block-comment
//! annotated HTML (`<!-- wp:paragraph -->...`). Kept free of any GTK
//! dependency so it can be unit tested and reused headlessly.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

mod reverse;
pub use reverse::gutenberg_to_markdown;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph { html: String },
    Heading { level: u8, html: String },
    List { ordered: bool, items: Vec<Vec<Block>> },
    BlockQuote { blocks: Vec<Block> },
    CodeBlock { lang: Option<String>, text: String },
    Image { url: String, alt: String, title: Option<String> },
    ThematicBreak,
    Table {
        alignments: Vec<ColumnAlignment>,
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// Passthrough for constructs not (yet) mapped to a specific Gutenberg
    /// block (footnotes, definition lists, ...) and for raw HTML the author
    /// wrote directly in the Markdown source.
    RawHtml { html: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnAlignment {
    None,
    Left,
    Center,
    Right,
}

impl From<Alignment> for ColumnAlignment {
    fn from(a: Alignment) -> Self {
        match a {
            Alignment::None => ColumnAlignment::None,
            Alignment::Left => ColumnAlignment::Left,
            Alignment::Center => ColumnAlignment::Center,
            Alignment::Right => ColumnAlignment::Right,
        }
    }
}

/// Parse Markdown into a `Block` tree.
pub fn parse_markdown(md: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<Event> = Parser::new_ext(md, options).collect();
    parse_blocks(&events, 0, events.len())
}

/// Render a `Block` tree as Gutenberg block-comment HTML, ready to hand to
/// the WordPress REST API as a post's `content`.
pub fn render_blocks(blocks: &[Block]) -> String {
    blocks.iter().map(render_block).collect::<Vec<_>>().join("\n\n")
}

/// Convenience one-shot: Markdown source -> Gutenberg block-comment HTML.
pub fn markdown_to_gutenberg(md: &str) -> String {
    render_blocks(&parse_markdown(md))
}

// ---------------------------------------------------------------------
// Parsing: pulldown-cmark event stream -> Block tree
// ---------------------------------------------------------------------

fn is_container_block_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::Table(_)
    )
}

/// Find the index of the `End` event matching the `Start` event at `start`,
/// using `Tag::to_end()` for depth counting. `Start`/`End` pairs are always
/// balanced by construction, so a Start only ever affects depth for the
/// specific `end_marker` it would itself produce.
fn find_matching_end(events: &[Event], start: usize, end_marker: &TagEnd) -> usize {
    let mut depth = 0usize;
    let mut j = start;
    while j < events.len() {
        match &events[j] {
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

fn inline_html(events: &[Event]) -> String {
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, events.iter().cloned());
    out.trim().to_string()
}

fn collect_text(events: &[Event]) -> String {
    let mut s = String::new();
    for e in events {
        match e {
            Event::Text(t) | Event::Code(t) => s.push_str(t),
            Event::SoftBreak | Event::HardBreak => s.push('\n'),
            _ => {}
        }
    }
    s
}

fn collect_raw_html(events: &[Event]) -> String {
    let mut s = String::new();
    for e in events {
        if let Event::Html(t) = e {
            s.push_str(t);
        }
    }
    s.trim_end().to_string()
}

fn heading_level_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A lone image is CommonMark's only way to express a "block-level" image:
/// `![alt](url)` on its own line parses as a Paragraph containing exactly one
/// inline Image. Detect that shape so it becomes a `wp:image` block instead
/// of a paragraph wrapping an `<img>`.
fn as_lone_image(events: &[Event]) -> Option<Block> {
    let Some(Event::Start(Tag::Image { dest_url, title, .. })) = events.first() else {
        return None;
    };
    let Some(Event::End(TagEnd::Image)) = events.last() else {
        return None;
    };
    let alt = collect_text(&events[1..events.len() - 1]);
    let title = if title.is_empty() { None } else { Some(title.to_string()) };
    Some(Block::Image {
        url: dest_url.to_string(),
        alt,
        title,
    })
}

fn parse_blocks(events: &[Event], mut i: usize, stop: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    while i < stop {
        match &events[i] {
            Event::Rule => {
                blocks.push(Block::ThematicBreak);
                i += 1;
            }
            Event::Start(tag) if is_container_block_tag(tag) => {
                let end_marker = tag.to_end();
                let end = find_matching_end(events, i, &end_marker);
                match tag {
                    Tag::Paragraph => {
                        let inner = &events[i + 1..end];
                        blocks.push(as_lone_image(inner).unwrap_or_else(|| Block::Paragraph {
                            html: inline_html(inner),
                        }));
                    }
                    Tag::Heading { level, .. } => {
                        let inner = &events[i + 1..end];
                        blocks.push(Block::Heading {
                            level: heading_level_num(*level),
                            html: inline_html(inner),
                        });
                    }
                    Tag::BlockQuote(_) => {
                        blocks.push(Block::BlockQuote {
                            blocks: parse_blocks(events, i + 1, end),
                        });
                    }
                    Tag::List(start_num) => {
                        blocks.push(Block::List {
                            ordered: start_num.is_some(),
                            items: parse_list_items(events, i + 1, end),
                        });
                    }
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                                Some(lang.to_string())
                            }
                            _ => None,
                        };
                        blocks.push(Block::CodeBlock {
                            lang,
                            text: collect_text(&events[i + 1..end]),
                        });
                    }
                    Tag::HtmlBlock => {
                        blocks.push(Block::RawHtml {
                            html: collect_raw_html(&events[i + 1..end]),
                        });
                    }
                    Tag::Table(aligns) => {
                        blocks.push(parse_table(events, i, end, aligns));
                    }
                    _ => unreachable!("is_container_block_tag guards this match"),
                }
                i = end + 1;
            }
            _ => {
                // Bare inline run: covers tight list items (no Paragraph
                // wrapper emitted by pulldown-cmark) and any other stray
                // inline content at block position.
                let run_start = i;
                while i < stop {
                    match &events[i] {
                        Event::Rule => break,
                        Event::Start(t) if is_container_block_tag(t) => break,
                        _ => i += 1,
                    }
                }
                let html = inline_html(&events[run_start..i]);
                if !html.is_empty() {
                    blocks.push(Block::Paragraph { html });
                }
            }
        }
    }
    blocks
}

fn parse_list_items(events: &[Event], mut i: usize, stop: usize) -> Vec<Vec<Block>> {
    let mut items = Vec::new();
    while i < stop {
        if matches!(&events[i], Event::Start(Tag::Item)) {
            let end = find_matching_end(events, i, &TagEnd::Item);
            items.push(parse_blocks(events, i + 1, end));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    items
}

fn parse_table(events: &[Event], start: usize, end: usize, aligns: &[Alignment]) -> Block {
    let alignments: Vec<ColumnAlignment> = aligns.iter().map(|a| (*a).into()).collect();
    let mut header = Vec::new();
    let mut rows = Vec::new();
    let mut i = start + 1;
    while i < end {
        match &events[i] {
            Event::Start(Tag::TableHead) => {
                let head_end = find_matching_end(events, i, &TagEnd::TableHead);
                header = parse_table_row_cells(events, i + 1, head_end);
                i = head_end + 1;
            }
            Event::Start(Tag::TableRow) => {
                let row_end = find_matching_end(events, i, &TagEnd::TableRow);
                rows.push(parse_table_row_cells(events, i + 1, row_end));
                i = row_end + 1;
            }
            _ => i += 1,
        }
    }
    Block::Table {
        alignments,
        header,
        rows,
    }
}

fn parse_table_row_cells(events: &[Event], mut i: usize, end: usize) -> Vec<String> {
    let mut cells = Vec::new();
    while i < end {
        if matches!(&events[i], Event::Start(Tag::TableCell)) {
            let cell_end = find_matching_end(events, i, &TagEnd::TableCell);
            cells.push(inline_html(&events[i + 1..cell_end]));
            i = cell_end + 1;
        } else {
            i += 1;
        }
    }
    cells
}

// ---------------------------------------------------------------------
// Rendering: Block tree -> Gutenberg block-comment HTML
// ---------------------------------------------------------------------

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn wrap(name: &str, attrs: Option<String>, content: &str) -> String {
    let attrs_str = attrs.map(|a| format!(" {a}")).unwrap_or_default();
    format!("<!-- wp:{name}{attrs_str} -->\n{content}\n<!-- /wp:{name} -->")
}

fn block_inner_html(block: &Block) -> String {
    match block {
        Block::Paragraph { html } => html.clone(),
        other => render_block(other),
    }
}

fn render_list_item(blocks: &[Block]) -> String {
    if blocks.is_empty() {
        return wrap("list-item", None, "<li></li>");
    }
    let mut content = block_inner_html(&blocks[0]);
    for b in &blocks[1..] {
        content.push('\n');
        content.push_str(&render_block(b));
    }
    wrap("list-item", None, &format!("<li>{content}</li>"))
}

fn render_list(ordered: bool, items: &[Vec<Block>]) -> String {
    let tag = if ordered { "ol" } else { "ul" };
    let attrs = ordered.then(|| "{\"ordered\":true}".to_string());
    let items_html = items
        .iter()
        .map(|item| render_list_item(item))
        .collect::<Vec<_>>()
        .join("\n");
    wrap(
        "list",
        attrs,
        &format!("<{tag} class=\"wp-block-list\">\n{items_html}\n</{tag}>"),
    )
}

fn align_style(alignments: &[ColumnAlignment], idx: usize) -> &'static str {
    match alignments.get(idx) {
        Some(ColumnAlignment::Left) => " style=\"text-align:left\"",
        Some(ColumnAlignment::Center) => " style=\"text-align:center\"",
        Some(ColumnAlignment::Right) => " style=\"text-align:right\"",
        _ => "",
    }
}

fn render_table(alignments: &[ColumnAlignment], header: &[String], rows: &[Vec<String>]) -> String {
    let thead = if header.is_empty() {
        String::new()
    } else {
        let cells: String = header
            .iter()
            .enumerate()
            .map(|(idx, h)| format!("<th{}>{h}</th>", align_style(alignments, idx)))
            .collect();
        format!("<thead><tr>{cells}</tr></thead>")
    };
    let body_rows: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .enumerate()
                .map(|(idx, c)| format!("<td{}>{c}</td>", align_style(alignments, idx)))
                .collect();
            format!("<tr>{cells}</tr>")
        })
        .collect();
    wrap(
        "table",
        None,
        &format!("<figure class=\"wp-block-table\"><table><tbody>{body_rows}</tbody></table></figure>")
            .replacen("<tbody>", &format!("{thead}<tbody>"), 1),
    )
}

fn render_block(block: &Block) -> String {
    match block {
        Block::Paragraph { html } => wrap("paragraph", None, &format!("<p>{html}</p>")),
        Block::Heading { level, html } => {
            let attrs = (*level != 2).then(|| format!("{{\"level\":{level}}}"));
            wrap("heading", attrs, &format!("<h{level}>{html}</h{level}>"))
        }
        Block::List { ordered, items } => render_list(*ordered, items),
        Block::BlockQuote { blocks } => {
            let inner = render_blocks(blocks);
            wrap(
                "quote",
                None,
                &format!("<blockquote class=\"wp-block-quote\">{inner}</blockquote>"),
            )
        }
        Block::CodeBlock { lang: _, text } => wrap(
            "code",
            None,
            &format!(
                "<pre class=\"wp-block-code\"><code>{}</code></pre>",
                escape_html(text.trim_end_matches('\n'))
            ),
        ),
        Block::Image { url, alt, title } => {
            let title_attr = title
                .as_ref()
                .map(|t| format!(" title=\"{}\"", escape_html(t)))
                .unwrap_or_default();
            wrap(
                "image",
                None,
                &format!(
                    "<figure class=\"wp-block-image\"><img src=\"{}\" alt=\"{}\"{title_attr}/></figure>",
                    escape_html(url),
                    escape_html(alt)
                ),
            )
        }
        Block::ThematicBreak => wrap(
            "separator",
            None,
            "<hr class=\"wp-block-separator has-alpha-channel-opacity\"/>",
        ),
        Block::Table {
            alignments,
            header,
            rows,
        } => render_table(alignments, header, rows),
        Block::RawHtml { html } => wrap("html", None, html.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_becomes_wp_paragraph() {
        assert_eq!(
            markdown_to_gutenberg("Hello **world**."),
            "<!-- wp:paragraph -->\n<p>Hello <strong>world</strong>.</p>\n<!-- /wp:paragraph -->"
        );
    }

    #[test]
    fn heading_level_two_has_no_attrs() {
        assert_eq!(
            markdown_to_gutenberg("## Title"),
            "<!-- wp:heading -->\n<h2>Title</h2>\n<!-- /wp:heading -->"
        );
    }

    #[test]
    fn heading_level_three_carries_level_attr() {
        assert_eq!(
            markdown_to_gutenberg("### Sub"),
            "<!-- wp:heading {\"level\":3} -->\n<h3>Sub</h3>\n<!-- /wp:heading -->"
        );
    }

    #[test]
    fn unordered_list_uses_list_item_children() {
        let out = markdown_to_gutenberg("- one\n- two\n");
        assert_eq!(
            out,
            "<!-- wp:list -->\n<ul class=\"wp-block-list\">\n\
             <!-- wp:list-item -->\n<li>one</li>\n<!-- /wp:list-item -->\n\
             <!-- wp:list-item -->\n<li>two</li>\n<!-- /wp:list-item -->\n\
             </ul>\n<!-- /wp:list -->"
        );
    }

    #[test]
    fn ordered_list_gets_ordered_attribute() {
        let out = markdown_to_gutenberg("1. first\n2. second\n");
        assert!(out.starts_with("<!-- wp:list {\"ordered\":true} -->\n<ol"));
        assert!(out.contains("<li>first</li>"));
    }

    #[test]
    fn nested_list_sits_inside_parent_li() {
        let out = markdown_to_gutenberg("- a\n  - nested\n- b\n");
        assert!(out.contains("<li>a\n<!-- wp:list -->"));
        assert!(out.contains("<li>nested</li>"));
    }

    #[test]
    fn blockquote_wraps_child_paragraph_block() {
        let out = markdown_to_gutenberg("> quoted text");
        assert_eq!(
            out,
            "<!-- wp:quote -->\n<blockquote class=\"wp-block-quote\">\
             <!-- wp:paragraph -->\n<p>quoted text</p>\n<!-- /wp:paragraph --></blockquote>\n\
             <!-- /wp:quote -->"
        );
    }

    #[test]
    fn fenced_code_block_becomes_wp_code() {
        let out = markdown_to_gutenberg("```rust\nlet x = 1;\n```");
        assert_eq!(
            out,
            "<!-- wp:code -->\n<pre class=\"wp-block-code\"><code>let x = 1;</code></pre>\n<!-- /wp:code -->"
        );
    }

    #[test]
    fn lone_image_line_becomes_wp_image() {
        let out = markdown_to_gutenberg("![a cat](https://example.com/cat.png)");
        assert_eq!(
            out,
            "<!-- wp:image -->\n<figure class=\"wp-block-image\">\
             <img src=\"https://example.com/cat.png\" alt=\"a cat\"/></figure>\n<!-- /wp:image -->"
        );
    }

    #[test]
    fn thematic_break_becomes_wp_separator() {
        let out = markdown_to_gutenberg("---");
        assert_eq!(
            out,
            "<!-- wp:separator -->\n<hr class=\"wp-block-separator has-alpha-channel-opacity\"/>\n<!-- /wp:separator -->"
        );
    }

    #[test]
    fn table_becomes_wp_table() {
        let out = markdown_to_gutenberg("| A | B |\n|---|---|\n| 1 | 2 |\n");
        assert_eq!(
            out,
            "<!-- wp:table -->\n<figure class=\"wp-block-table\"><table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table></figure>\n<!-- /wp:table -->"
        );
    }

    #[test]
    fn raw_html_block_is_passed_through() {
        let out = markdown_to_gutenberg("<div class=\"embed\">hi</div>");
        assert_eq!(
            out,
            "<!-- wp:html -->\n<div class=\"embed\">hi</div>\n<!-- /wp:html -->"
        );
    }

    #[test]
    fn multiple_blocks_are_joined_with_blank_line() {
        let out = markdown_to_gutenberg("# Title\n\nSome text.\n");
        assert_eq!(
            out,
            "<!-- wp:heading {\"level\":1} -->\n<h1>Title</h1>\n<!-- /wp:heading -->\n\n\
             <!-- wp:paragraph -->\n<p>Some text.</p>\n<!-- /wp:paragraph -->"
        );
    }
}
