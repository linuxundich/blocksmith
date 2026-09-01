//! Markdown file I/O with a small hand-rolled YAML-frontmatter dialect.
//!
//! A plain `.md` file with no frontmatter block round-trips as plain text.
//! Once metadata (title/slug/status/categories/tags/featured image/WP post
//! id) is set via the properties dialog, it's persisted as a `---`-delimited
//! block at the top of the file so re-opening and re-exporting the same
//! article can target the same WordPress post (`wp_post_id`).
//!
//! Deliberately not a general YAML parser: the schema is small and fixed,
//! so a tiny purpose-built parser avoids pulling in a dependency (the
//! obvious choices, `serde_yaml` and its forks, are unmaintained) for a
//! handful of scalar/list fields.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostStatus {
    Draft,
    Pending,
    Publish,
}

impl PostStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PostStatus::Draft => "draft",
            PostStatus::Pending => "pending",
            PostStatus::Publish => "publish",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "pending" => PostStatus::Pending,
            "publish" => PostStatus::Publish,
            _ => PostStatus::Draft,
        }
    }

    pub const ALL: [PostStatus; 3] = [PostStatus::Draft, PostStatus::Pending, PostStatus::Publish];

    /// Human-readable German label, for the properties dialog's dropdown.
    pub fn label(&self) -> &'static str {
        match self {
            PostStatus::Draft => "Entwurf",
            PostStatus::Pending => "Ausstehend",
            PostStatus::Publish => "Veröffentlicht",
        }
    }
}

impl Default for PostStatus {
    fn default() -> Self {
        PostStatus::Draft
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frontmatter {
    pub title: String,
    pub slug: String,
    pub status: PostStatus,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub featured_image: Option<String>,
    /// Set once the document has been published/updated via the WordPress
    /// REST API (see M5), so re-exporting updates the same post.
    pub wp_post_id: Option<u64>,
    /// The WordPress media id of the post's *current* featured image, when
    /// the document was opened from an existing post (`importer.rs`) - not
    /// user-editable. `featured_image` is a local path to *upload as a new*
    /// featured image; this carries the existing remote one through
    /// unchanged on re-export until the user actually sets `featured_image`.
    pub featured_media_id: Option<u64>,
}

impl Frontmatter {
    pub fn has_metadata(&self) -> bool {
        self != &Frontmatter::default()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub frontmatter: Frontmatter,
    pub body: String,
}

pub fn read(path: &Path) -> std::io::Result<Document> {
    let raw = std::fs::read_to_string(path)?;
    Ok(parse(&raw))
}

pub fn write(path: &Path, doc: &Document) -> std::io::Result<()> {
    std::fs::write(path, serialize(doc))
}

pub fn parse(input: &str) -> Document {
    // `split_inclusive` keeps each line's own newline attached, so
    // concatenating a sub-slice of `lines` reproduces the original bytes
    // exactly (including whether the file ends with a trailing newline) -
    // unlike `.lines()`, which would silently drop that information.
    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let is_delimiter = |l: &str| l.trim_end_matches(['\n', '\r']) == "---";

    if lines.first().map(|l| is_delimiter(l)) != Some(true) {
        return Document {
            frontmatter: Frontmatter::default(),
            body: input.to_string(),
        };
    }

    let Some(end) = lines[1..].iter().position(|l| is_delimiter(l)) else {
        return Document {
            frontmatter: Frontmatter::default(),
            body: input.to_string(),
        };
    };
    let end = end + 1; // index into `lines`, relative to the full slice

    let mut frontmatter = Frontmatter::default();
    for line in &lines[1..end] {
        let content = line.trim_end_matches(['\n', '\r']);
        let Some((key, value)) = content.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "title" => frontmatter.title = unquote(value),
            "slug" => frontmatter.slug = unquote(value),
            "status" => frontmatter.status = PostStatus::from_str(value),
            "categories" => frontmatter.categories = parse_list(value),
            "tags" => frontmatter.tags = parse_list(value),
            "featured_image" => {
                frontmatter.featured_image = (!value.is_empty()).then(|| unquote(value));
            }
            "wp_post_id" => frontmatter.wp_post_id = value.parse::<u64>().ok(),
            "wp_featured_media_id" => frontmatter.featured_media_id = value.parse::<u64>().ok(),
            _ => {}
        }
    }

    let mut body_lines = &lines[(end + 1).min(lines.len())..];
    // Exactly one blank separator line is conventional right after the
    // closing `---` (and is what `serialize` writes) - drop it so
    // round-tripping doesn't grow an extra blank line each time.
    if body_lines.first() == Some(&"\n") || body_lines.first() == Some(&"\r\n") {
        body_lines = &body_lines[1..];
    }
    let body = body_lines.concat();

    Document { frontmatter, body }
}

pub fn serialize(doc: &Document) -> String {
    if !doc.frontmatter.has_metadata() {
        return doc.body.clone();
    }

    let fm = &doc.frontmatter;
    let mut out = String::from("---\n");
    out.push_str(&format!("title: \"{}\"\n", escape(&fm.title)));
    out.push_str(&format!("slug: \"{}\"\n", escape(&fm.slug)));
    out.push_str(&format!("status: {}\n", fm.status.as_str()));
    out.push_str(&format!("categories: {}\n", render_list(&fm.categories)));
    out.push_str(&format!("tags: {}\n", render_list(&fm.tags)));
    if let Some(img) = &fm.featured_image {
        out.push_str(&format!("featured_image: \"{}\"\n", escape(img)));
    }
    if let Some(id) = fm.wp_post_id {
        out.push_str(&format!("wp_post_id: {id}\n"));
    }
    if let Some(id) = fm.featured_media_id {
        out.push_str(&format!("wp_featured_media_id: {id}\n"));
    }
    out.push_str("---\n\n");
    out.push_str(&doc.body);
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let quoted = (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''));
    if quoted && s.len() >= 2 {
        unescape(&s[1..s.len() - 1])
    } else {
        s.to_string()
    }
}

/// Parses a comma-separated list, with or without surrounding `[...]` -
/// reused both for frontmatter values and for the properties dialog's
/// plain "Tech, Rust" style text entry.
pub fn parse_list(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(s);
    inner
        .split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

fn render_list(items: &[String]) -> String {
    let rendered: Vec<String> = items.iter().map(|i| format!("\"{}\"", escape(i))).collect();
    format!("[{}]", rendered.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_markdown_without_frontmatter_round_trips() {
        let input = "# Just a heading\n\nSome text.\n";
        let doc = parse(input);
        assert_eq!(doc.frontmatter, Frontmatter::default());
        assert_eq!(doc.body, input);
        assert_eq!(serialize(&doc), input);
    }

    #[test]
    fn full_frontmatter_parses_all_fields() {
        let input = "---\n\
                     title: \"Hello World\"\n\
                     slug: \"hello-world\"\n\
                     status: publish\n\
                     categories: [\"Tech\", \"Rust\"]\n\
                     tags: [\"gtk\", \"wordpress\"]\n\
                     featured_image: \"/tmp/cat.png\"\n\
                     wp_post_id: 42\n\
                     wp_featured_media_id: 7\n\
                     ---\n\
                     \n\
                     Body text here.\n";
        let doc = parse(input);
        assert_eq!(doc.frontmatter.title, "Hello World");
        assert_eq!(doc.frontmatter.slug, "hello-world");
        assert_eq!(doc.frontmatter.status, PostStatus::Publish);
        assert_eq!(doc.frontmatter.categories, vec!["Tech", "Rust"]);
        assert_eq!(doc.frontmatter.tags, vec!["gtk", "wordpress"]);
        assert_eq!(doc.frontmatter.featured_image.as_deref(), Some("/tmp/cat.png"));
        assert_eq!(doc.frontmatter.wp_post_id, Some(42));
        assert_eq!(doc.frontmatter.featured_media_id, Some(7));
        assert_eq!(doc.body, "Body text here.\n");
    }

    #[test]
    fn serialize_without_metadata_stays_plain() {
        let doc = Document {
            frontmatter: Frontmatter::default(),
            body: "just text\n".to_string(),
        };
        assert_eq!(serialize(&doc), "just text\n");
    }

    #[test]
    fn serialize_then_parse_round_trips_with_metadata() {
        let doc = Document {
            frontmatter: Frontmatter {
                title: "A \"quoted\" title".to_string(),
                slug: "a-quoted-title".to_string(),
                status: PostStatus::Pending,
                categories: vec!["Cat A".to_string(), "Cat B".to_string()],
                tags: vec!["one".to_string()],
                featured_image: None,
                wp_post_id: Some(7),
                featured_media_id: Some(99),
            },
            body: "Some **body**.\n".to_string(),
        };
        let round_tripped = parse(&serialize(&doc));
        assert_eq!(round_tripped, doc);
    }

    #[test]
    fn unknown_status_falls_back_to_draft() {
        assert_eq!(PostStatus::from_str("bogus"), PostStatus::Draft);
    }
}
