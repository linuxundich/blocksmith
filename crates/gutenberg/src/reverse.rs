//! Converts WordPress Gutenberg block-comment HTML back into Markdown - the
//! reverse of [`crate::markdown_to_gutenberg`] - so an existing WordPress
//! article can be pulled into Blocksmith, edited as Markdown, and pushed
//! back. Parses into the same [`Block`] tree the forward direction uses,
//! then renders that tree as Markdown text instead of Gutenberg HTML.
//!
//! Deliberately a hand-rolled comment/tag scanner, not a general HTML
//! parser: Gutenberg's block-comment grammar is simple and well-defined
//! (`<!-- wp:name {attrs}? -->...<!-- /wp:name -->` or a self-closing
//! `<!-- wp:name /-->`), and the HTML *inside* each block is exactly what
//! WordPress's own block library renders for that block type. Round-trip
//! fidelity is solid for content this crate produced (or plain, standard
//! Gutenberg blocks); anything unrecognized passes through as raw HTML
//! rather than silently losing content.

use crate::{Block, ColumnAlignment};

pub fn gutenberg_to_markdown(html: &str) -> String {
    render_markdown(&parse_gutenberg_blocks(html))
}

// ---------------------------------------------------------------------
// Comment scanning
// ---------------------------------------------------------------------

struct ParsedComment<'a> {
    closing: bool,
    name: &'a str,
    attrs: Option<&'a str>,
    self_closing: bool,
}

/// Finds the next HTML comment at or after `from`, returning its trimmed
/// inner text plus the byte range of the whole `<!-- ... -->` comment.
fn next_comment(html: &str, from: usize) -> Option<(&str, usize, usize)> {
    let start = html[from..].find("<!--")? + from;
    let inner_start = start + 4;
    let end_rel = html[inner_start..].find("-->")?;
    let inner_end = inner_start + end_rel;
    Some((html[inner_start..inner_end].trim(), start, inner_end + 3))
}

fn parse_wp_comment(inner: &str) -> Option<ParsedComment<'_>> {
    let closing = inner.starts_with('/');
    let body = if closing { &inner[1..] } else { inner };
    let body = body.strip_prefix("wp:")?;
    let self_closing = body.trim_end().ends_with('/');
    let body = if self_closing { body.trim_end().trim_end_matches('/').trim_end() } else { body };
    let (name, attrs) = match body.find(char::is_whitespace) {
        Some(idx) => (&body[..idx], Some(body[idx..].trim())),
        None => (body, None),
    };
    Some(ParsedComment { closing, name, attrs, self_closing })
}

fn push_stray(blocks: &mut Vec<Block>, html: &str) {
    let trimmed = html.trim();
    if !trimmed.is_empty() {
        blocks.push(Block::RawHtml { html: trimmed.to_string() });
    }
}

/// Depth-aware search (nested blocks of the *same* name, e.g. `wp:list`
/// inside `wp:list`, are common) for the closing comment matching the
/// opening block whose content starts at `start`.
fn find_block_end(html: &str, start: usize, name: &str) -> Option<(usize, usize)> {
    let mut depth = 1i32;
    let mut pos = start;
    loop {
        let (inner, cstart, cend) = next_comment(html, pos)?;
        if let Some(p) = parse_wp_comment(inner) {
            if p.name == name && !p.self_closing {
                if p.closing {
                    depth -= 1;
                    if depth == 0 {
                        return Some((cstart, cend));
                    }
                } else {
                    depth += 1;
                }
            }
        }
        pos = cend;
    }
}

fn parse_gutenberg_blocks(html: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut pos = 0;
    while pos < html.len() {
        let Some((inner, cstart, cend)) = next_comment(html, pos) else {
            push_stray(&mut blocks, &html[pos..]);
            break;
        };
        let Some(parsed) = parse_wp_comment(inner) else {
            pos = cend;
            continue;
        };
        if parsed.closing {
            pos = cend;
            continue;
        }
        if cstart > pos {
            push_stray(&mut blocks, &html[pos..cstart]);
        }
        if parsed.self_closing {
            blocks.push(make_block(parsed.name, parsed.attrs, ""));
            pos = cend;
            continue;
        }
        match find_block_end(html, cend, parsed.name) {
            Some((inner_end, after)) => {
                blocks.push(make_block(parsed.name, parsed.attrs, &html[cend..inner_end]));
                pos = after;
            }
            None => {
                blocks.push(make_block(parsed.name, parsed.attrs, &html[cend..]));
                pos = html.len();
            }
        }
    }
    blocks
}

fn make_block(name: &str, attrs: Option<&str>, inner: &str) -> Block {
    match name {
        "paragraph" => Block::Paragraph {
            html: inline_html_to_markdown(&strip_wrapper_tag(inner, "p")),
        },
        "heading" => {
            let level = detect_heading_level(inner).unwrap_or(2);
            let html = inline_html_to_markdown(&strip_wrapper_tag(inner, &format!("h{level}")));
            Block::Heading { level, html }
        }
        "list" => {
            let ordered = attr_flag(attrs, "ordered");
            let list_tag = if ordered { "ol" } else { "ul" };
            let items = parse_list_items(&strip_wrapper_tag(inner, list_tag));
            Block::List { ordered, items }
        }
        "quote" => Block::BlockQuote {
            blocks: parse_gutenberg_blocks(&strip_wrapper_tag(inner, "blockquote")),
        },
        "code" => {
            let code = strip_wrapper_tag(&strip_wrapper_tag(inner, "pre"), "code");
            Block::CodeBlock {
                lang: None,
                text: unescape_entities(code.trim()),
            }
        }
        "image" => Block::Image {
            url: extract_attr(inner, "src").unwrap_or_default(),
            alt: extract_attr(inner, "alt").unwrap_or_default(),
            title: extract_attr(inner, "title").filter(|t| !t.is_empty()),
        },
        "separator" => Block::ThematicBreak,
        "table" => parse_table_block(inner),
        // Unrecognized block types (custom blocks, embeds, ...) and our own
        // "html" passthrough both just keep their raw HTML - nothing lost.
        _ => Block::RawHtml { html: inner.trim().to_string() },
    }
}

fn attr_flag(attrs: Option<&str>, key: &str) -> bool {
    attrs.is_some_and(|a| a.contains(&format!("\"{key}\":true")))
}

fn detect_heading_level(inner: &str) -> Option<u8> {
    let trimmed = inner.trim_start();
    (1..=6u8).find(|level| trimmed.starts_with(&format!("<h{level}")))
}

/// Strips a leading `<tag ...>` and trailing `</tag>` wrapper, returning
/// what's between. Falls back to the input unchanged if `tag` isn't found,
/// so malformed input degrades gracefully instead of panicking or vanishing.
fn strip_wrapper_tag(html: &str, tag: &str) -> String {
    let html = html.trim();
    let open_prefix = format!("<{tag}");
    if !html.starts_with(&open_prefix) {
        return html.to_string();
    }
    let Some(gt) = html.find('>') else {
        return html.to_string();
    };
    let close_tag = format!("</{tag}>");
    match html.rfind(&close_tag) {
        Some(close_pos) if close_pos >= gt => html[gt + 1..close_pos].to_string(),
        _ => html[gt + 1..].to_string(),
    }
}

fn parse_list_items(list_inner: &str) -> Vec<Vec<Block>> {
    let mut items = Vec::new();
    let mut pos = 0;
    while pos < list_inner.len() {
        let Some((inner, _cstart, cend)) = next_comment(list_inner, pos) else {
            break;
        };
        let Some(parsed) = parse_wp_comment(inner) else {
            pos = cend;
            continue;
        };
        if parsed.closing || parsed.name != "list-item" {
            pos = cend;
            continue;
        }
        if parsed.self_closing {
            items.push(vec![Block::Paragraph { html: String::new() }]);
            pos = cend;
            continue;
        }
        match find_block_end(list_inner, cend, "list-item") {
            Some((inner_end, after)) => {
                items.push(parse_list_item_content(&list_inner[cend..inner_end]));
                pos = after;
            }
            None => {
                items.push(parse_list_item_content(&list_inner[cend..]));
                pos = list_inner.len();
            }
        }
    }
    items
}

/// A list item's own text comes first (as a `Paragraph`), followed by any
/// nested list found inside the same `<li>` - mirroring exactly how the
/// forward direction structures a list item's `Vec<Block>`.
fn parse_list_item_content(li_wrapped: &str) -> Vec<Block> {
    let li_inner = strip_wrapper_tag(li_wrapped, "li");
    if let Some((inner, cstart, _cend)) = next_comment(&li_inner, 0) {
        if let Some(parsed) = parse_wp_comment(inner) {
            if !parsed.closing && parsed.name == "list" {
                let text = li_inner[..cstart].trim();
                let mut result = vec![Block::Paragraph {
                    html: inline_html_to_markdown(text),
                }];
                result.extend(parse_gutenberg_blocks(&li_inner[cstart..]));
                return result;
            }
        }
    }
    vec![Block::Paragraph {
        html: inline_html_to_markdown(li_inner.trim()),
    }]
}

fn parse_table_block(inner: &str) -> Block {
    let table_inner = strip_wrapper_tag(inner, "table");
    let mut alignments = Vec::new();
    let mut header = Vec::new();
    if let Some(thead) = extract_between(&table_inner, "<thead>", "</thead>") {
        if let Some(row) = extract_between(thead, "<tr>", "</tr>") {
            for cell_tag in extract_all_tags(row, "th") {
                alignments.push(alignment_from_style(cell_tag.0));
                header.push(inline_html_to_markdown(cell_tag.1));
            }
        }
    }
    let tbody = extract_between(&table_inner, "<tbody>", "</tbody>").unwrap_or(table_inner.as_str());
    let mut rows = Vec::new();
    for row in extract_all_between(tbody, "<tr>", "</tr>") {
        let cells = extract_all_tags(row, "td").into_iter().map(|(_, content)| inline_html_to_markdown(content)).collect();
        rows.push(cells);
    }
    Block::Table { alignments, header, rows }
}

fn alignment_from_style(open_tag: &str) -> ColumnAlignment {
    if open_tag.contains("text-align:left") {
        ColumnAlignment::Left
    } else if open_tag.contains("text-align:center") {
        ColumnAlignment::Center
    } else if open_tag.contains("text-align:right") {
        ColumnAlignment::Right
    } else {
        ColumnAlignment::None
    }
}

/// Finds the first `<start>...<end>` span and returns what's between.
fn extract_between<'a>(html: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s = html.find(start)? + start.len();
    let e = html[s..].find(end)? + s;
    Some(&html[s..e])
}

/// Finds every `<start>...<end>` span (non-overlapping, in order).
fn extract_all_between<'a>(html: &'a str, start: &str, end: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(s_rel) = html[pos..].find(start) {
        let s = pos + s_rel + start.len();
        let Some(e_rel) = html[s..].find(end) else { break };
        let e = s + e_rel;
        out.push(&html[s..e]);
        pos = e + end.len();
    }
    out
}

/// Finds every `<tagname ...>content</tagname>` element, returning each
/// element's opening tag (for reading attributes like `style=`) alongside
/// its inner content.
fn extract_all_tags<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let mut out = Vec::new();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut pos = 0;
    while let Some(s_rel) = html[pos..].find(&open_prefix) {
        let tag_start = pos + s_rel;
        let Some(gt_rel) = html[tag_start..].find('>') else { break };
        let content_start = tag_start + gt_rel + 1;
        let Some(e_rel) = html[content_start..].find(&close_tag) else { break };
        let content_end = content_start + e_rel;
        out.push((&html[tag_start..content_start], &html[content_start..content_end]));
        pos = content_end + close_tag.len();
    }
    out
}

fn extract_attr(html: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find('"')? + start;
    Some(unescape_entities(&html[start..end]))
}

// ---------------------------------------------------------------------
// Inline HTML -> Markdown
// ---------------------------------------------------------------------

fn inline_html_to_markdown(html: &str) -> String {
    let mut out = String::new();
    let mut link_hrefs: Vec<String> = Vec::new();
    let mut i = 0;
    while i < html.len() {
        if html.as_bytes()[i] == b'<' {
            let Some(rel_end) = html[i..].find('>') else {
                break;
            };
            let tag_content = &html[i + 1..i + rel_end];
            i += rel_end + 1;
            let is_closing = tag_content.starts_with('/');
            let body = tag_content.trim_start_matches('/').trim_end_matches('/');
            let name = body.split_whitespace().next().unwrap_or("").to_lowercase();
            if is_closing {
                match name.as_str() {
                    "strong" | "b" => out.push_str("**"),
                    "em" | "i" => out.push('*'),
                    "code" => out.push('`'),
                    "s" | "del" => out.push_str("~~"),
                    "a" => out.push_str(&format!("]({})", link_hrefs.pop().unwrap_or_default())),
                    _ => {}
                }
            } else {
                match name.as_str() {
                    "strong" | "b" => out.push_str("**"),
                    "em" | "i" => out.push('*'),
                    "code" => out.push('`'),
                    "s" | "del" => out.push_str("~~"),
                    "br" => out.push('\n'),
                    "a" => {
                        link_hrefs.push(extract_attr(body, "href").unwrap_or_default());
                        out.push('[');
                    }
                    _ => {}
                }
            }
        } else {
            let next = html[i..].find('<').map(|p| i + p).unwrap_or(html.len());
            out.push_str(&unescape_entities(&html[i..next]));
            i = next;
        }
    }
    out.trim().to_string()
}

fn unescape_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
}

// ---------------------------------------------------------------------
// Block tree -> Markdown
// ---------------------------------------------------------------------

fn render_markdown(blocks: &[Block]) -> String {
    blocks.iter().map(render_block_markdown).collect::<Vec<_>>().join("\n\n")
}

fn render_block_markdown(block: &Block) -> String {
    match block {
        Block::Paragraph { html } => html.clone(),
        Block::Heading { level, html } => format!("{} {html}", "#".repeat(*level as usize)),
        Block::List { ordered, items } => render_list_markdown(*ordered, items, 0),
        Block::BlockQuote { blocks } => render_markdown(blocks)
            .lines()
            .map(|line| if line.is_empty() { ">".to_string() } else { format!("> {line}") })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::CodeBlock { lang, text } => format!("```{}\n{text}\n```", lang.clone().unwrap_or_default()),
        Block::Image { url, alt, title } => match title {
            Some(title) => format!("![{alt}]({url} \"{title}\")"),
            None => format!("![{alt}]({url})"),
        },
        Block::ThematicBreak => "---".to_string(),
        Block::Table { alignments, header, rows } => render_table_markdown(alignments, header, rows),
        Block::RawHtml { html } => html.clone(),
    }
}

fn render_list_markdown(ordered: bool, items: &[Vec<Block>], indent: usize) -> String {
    let pad = " ".repeat(indent);
    items
        .iter()
        .enumerate()
        .map(|(idx, item_blocks)| {
            let marker = if ordered { format!("{}.", idx + 1) } else { "-".to_string() };
            let mut lines = Vec::new();
            for (i, block) in item_blocks.iter().enumerate() {
                if i == 0 {
                    lines.push(format!("{pad}{marker} {}", render_block_markdown(block)));
                } else if let Block::List { ordered: nested_ordered, items: nested_items } = block {
                    lines.push(render_list_markdown(*nested_ordered, nested_items, indent + 2));
                } else {
                    lines.push(render_block_markdown(block));
                }
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_table_markdown(alignments: &[ColumnAlignment], header: &[String], rows: &[Vec<String>]) -> String {
    let mut lines = Vec::new();
    if !header.is_empty() {
        lines.push(format!("| {} |", header.join(" | ")));
        let seps: Vec<String> = (0..header.len())
            .map(|i| match alignments.get(i) {
                Some(ColumnAlignment::Left) => ":---".to_string(),
                Some(ColumnAlignment::Center) => ":---:".to_string(),
                Some(ColumnAlignment::Right) => "---:".to_string(),
                _ => "---".to_string(),
            })
            .collect();
        lines.push(format!("| {} |", seps.join(" | ")));
    }
    for row in rows {
        lines.push(format!("| {} |", row.join(" | ")));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown_to_gutenberg;

    fn round_trip(markdown: &str) -> String {
        gutenberg_to_markdown(&markdown_to_gutenberg(markdown))
    }

    #[test]
    fn paragraph_round_trips() {
        assert_eq!(round_trip("Hello **world**, this is *fun*."), "Hello **world**, this is *fun*.");
    }

    #[test]
    fn heading_levels_round_trip() {
        assert_eq!(round_trip("## Title"), "## Title");
        assert_eq!(round_trip("### Sub"), "### Sub");
    }

    #[test]
    fn unordered_list_round_trips() {
        assert_eq!(round_trip("- one\n- two\n"), "- one\n- two");
    }

    #[test]
    fn ordered_list_round_trips() {
        assert_eq!(round_trip("1. first\n2. second\n"), "1. first\n2. second");
    }

    #[test]
    fn nested_list_round_trips() {
        assert_eq!(round_trip("- a\n  - nested\n- b\n"), "- a\n  - nested\n- b");
    }

    #[test]
    fn blockquote_round_trips() {
        assert_eq!(round_trip("> quoted text"), "> quoted text");
    }

    #[test]
    fn code_block_round_trips() {
        assert_eq!(round_trip("```\nlet x = 1;\n```"), "```\nlet x = 1;\n```");
    }

    #[test]
    fn image_round_trips() {
        assert_eq!(
            round_trip("![a cat](https://example.com/cat.png)"),
            "![a cat](https://example.com/cat.png)"
        );
    }

    #[test]
    fn thematic_break_round_trips() {
        assert_eq!(round_trip("---"), "---");
    }

    #[test]
    fn table_round_trips() {
        let out = round_trip("| A | B |\n|---|---|\n| 1 | 2 |\n");
        assert_eq!(out, "| A | B |\n| --- | --- |\n| 1 | 2 |");
    }

    #[test]
    fn raw_html_round_trips() {
        assert_eq!(round_trip("<div class=\"embed\">hi</div>"), "<div class=\"embed\">hi</div>");
    }

    #[test]
    fn link_round_trips() {
        assert_eq!(round_trip("Check [this](https://example.com) out."), "Check [this](https://example.com) out.");
    }

    #[test]
    fn multiple_blocks_round_trip_with_blank_line_separation() {
        assert_eq!(round_trip("# Title\n\nSome text.\n"), "# Title\n\nSome text.");
    }

    #[test]
    fn table_alignment_round_trips() {
        let out = round_trip("| A | B | C |\n|:---|:---:|---:|\n| 1 | 2 | 3 |\n");
        assert_eq!(out, "| A | B | C |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |");
    }
}
