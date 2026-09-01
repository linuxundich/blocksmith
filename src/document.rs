//! Plain Markdown file I/O.
//!
//! Frontmatter (title/slug/status/categories/`wp_post_id`) is out of scope
//! for this milestone (see M3 in the project plan) — for now a document is
//! just a `.md` file's raw text.

use std::path::Path;

pub fn read(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

pub fn write(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}
