//! Converts clipboard HTML (as pasted from a browser, word processor, or
//! any other rich-text source) into Markdown, so Ctrl+V from outside the
//! app doesn't just drop all its formatting - see `window.rs`'s
//! `wire_paste_shortcut` for where this is actually wired into Ctrl+V.
//!
//! Uses `htmd` (a real HTML5 parser under the hood, via `html5ever` - the
//! same engine Firefox/Servo use) rather than a hand-rolled scanner:
//! `crates/gutenberg`'s own Gutenberg-comment scanner gets away with being
//! hand-rolled because that format is small, fixed, and entirely this
//! app's own well-defined output (see its module doc comment); pasted HTML
//! from an arbitrary external source (Google Docs, Word, a web page) is
//! none of those things, and a hand-rolled parser would be fragile against
//! it in a way that would defeat the point of this feature.

use htmd::options::{BulletListMarker, Options};
use htmd::HtmlToMarkdown;

/// Converts an HTML fragment to Markdown, using this app's own list
/// convention (`"- "` - a dash, single space, matching `formatting.rs`'s
/// "Liste" toolbar button) rather than `htmd`'s own defaults (`*`, three
/// spaces - a turndown.js convention this app has no reason to follow), so
/// pasted and hand-typed lists look consistent within the same document.
pub fn html_to_markdown(html: &str) -> Result<String, String> {
    let converter = HtmlToMarkdown::builder()
        .options(Options {
            bullet_list_marker: BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ..Default::default()
        })
        .build();
    converter.convert(html).map_err(|err| err.to_string())
}

/// Decodes a clipboard `text/html` payload's raw bytes into a plain HTML
/// string, handling the encoding quirks real-world clipboard sources
/// actually use:
///
/// - Plain UTF-8 bytes - what GTK/Linux apps (Firefox, LibreOffice, GNOME
///   apps) put there today, and the common case, tried first.
/// - UTF-16 (BOM-prefixed little-endian, or bare) - some Windows-heritage
///   sources still encode this way regardless of platform; tried as a
///   fallback once plain UTF-8 decoding fails.
///
/// Either way, if the decoded text turns out to use the Windows "CF_HTML"
/// convention (`Version:0.9\r\nStartHTML:...` headers wrapping a full HTML
/// document, with the actually-copied content marked by byte-offset
/// `StartFragment`/`EndFragment` header fields) - some cross-platform apps
/// keep using this even on Linux, for consistency with their Windows
/// build - only the marked fragment is kept, not the surrounding
/// `<html><head>...` noise (see `extract_cf_html_fragment`).
///
/// Returns `None` if the bytes are valid in neither encoding.
pub fn decode_clipboard_html(bytes: &[u8]) -> Option<String> {
    let decoded = String::from_utf8(bytes.to_vec()).ok().or_else(|| decode_utf16(bytes))?;
    Some(extract_cf_html_fragment(&decoded).unwrap_or(decoded))
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let (chunks, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = chunks.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16(&units).ok()
}

/// Extracts the `StartFragment`/`EndFragment`-marked slice out of a
/// Windows "CF_HTML" clipboard payload - `None` if `decoded` doesn't
/// actually have the `Version:` header this format starts with, so a
/// plain (unwrapped) `text/html` payload passes straight through.
fn extract_cf_html_fragment(decoded: &str) -> Option<String> {
    if !decoded.starts_with("Version:") {
        return None;
    }
    let start = header_offset(decoded, "StartFragment:")?;
    let end = header_offset(decoded, "EndFragment:")?;
    decoded.get(start..end).map(str::to_string)
}

fn header_offset(decoded: &str, key: &str) -> Option<usize> {
    let line = decoded.lines().find(|l| l.starts_with(key))?;
    line[key.len()..].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_markdown_converts_common_formatting() {
        let md = html_to_markdown("<h1>Title</h1><p>Hello <strong>world</strong>, this is <em>fun</em>.</p>").unwrap();
        assert_eq!(md, "# Title\n\nHello **world**, this is *fun*.");
    }

    #[test]
    fn html_to_markdown_uses_a_dash_and_single_space_for_bullets() {
        let md = html_to_markdown("<ul><li>one</li><li>two</li></ul>").unwrap();
        assert_eq!(md, "- one\n- two");
    }

    #[test]
    fn html_to_markdown_converts_a_link() {
        let md = html_to_markdown("<a href=\"https://example.com\">Example</a>").unwrap();
        assert_eq!(md, "[Example](https://example.com)");
    }

    #[test]
    fn decode_clipboard_html_reads_plain_utf8_bytes() {
        let html = "<p>Hello</p>";
        assert_eq!(decode_clipboard_html(html.as_bytes()).as_deref(), Some(html));
    }

    #[test]
    fn decode_clipboard_html_reads_utf16_le_with_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "<p>Hallo</p>".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_clipboard_html(&bytes).as_deref(), Some("<p>Hallo</p>"));
    }

    #[test]
    fn decode_clipboard_html_extracts_the_cf_html_fragment_only() {
        let payload = "Version:0.9\r\n\
                        StartHTML:0000000097\r\n\
                        EndHTML:0000000150\r\n\
                        StartFragment:0000000121\r\n\
                        EndFragment:0000000141\r\n\
                        <html><body>\r\n\
                        <!--StartFragment--><p>kept</p><!--EndFragment-->\r\n\
                        </body></html>";
        let start = payload.find("<p>kept</p>").unwrap();
        let end = start + "<p>kept</p>".len();
        // Rebuild with header offsets matching this exact payload, rather than
        // hand-counting bytes - keeps the test robust to incidental edits above.
        let payload = payload.replacen("0000000121", &format!("{start:010}"), 1).replacen("0000000141", &format!("{end:010}"), 1);
        assert_eq!(decode_clipboard_html(payload.as_bytes()).as_deref(), Some("<p>kept</p>"));
    }

    #[test]
    fn decode_clipboard_html_returns_none_for_invalid_bytes() {
        assert_eq!(decode_clipboard_html(&[0xFF, 0x00, 0x80]), None);
    }
}
