# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Licensed under GPL-3.0-or-later (`LICENSE`, `license` field in both crates'
  `Cargo.toml`).

## [0.1.0] - 2026-09-01

Initial development release: a working foundation, not yet able to publish
to WordPress.

### Added

- Split-pane editor window (GTK4 + libadwaita): a `GtkSourceView` Markdown
  editor on the left with markdown syntax highlighting, a `WebKitWebView`
  live HTML preview on the right, debounced re-rendering on every edit.
- Basic document actions: New / Open / Save for plain `.md` files
  (Ctrl+N/O/S), via GTK4's `FileDialog`.
- Markdown → Gutenberg block-comment HTML engine (`crates/gutenberg`),
  standalone and unit-tested independently of the GUI. Maps paragraphs,
  headings (with the `level` attribute only when it differs from the
  default), nested lists (`core/list` + `core/list-item`, WP 6.3+ format),
  block quotes, fenced code blocks, standalone images, thematic breaks,
  tables, and raw HTML blocks.
- Document frontmatter: title, slug, status (draft/pending/publish),
  categories, tags, featured image path, and WordPress post id, stored as a
  small hand-rolled YAML-like frontmatter block at the top of the `.md`
  file. Editable via an "Artikel-Eigenschaften" dialog reachable from the
  header bar or Ctrl+,.
- WordPress connection settings dialog: site URL and username are stored in
  a plain config file; the Application Password is stored in the Secret
  Service (GNOME Keyring, or the portal equivalent under Flatpak) via `oo7`,
  never written to disk in plain text.

[Unreleased]: https://github.com/linuxundich/blocksmith/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/linuxundich/blocksmith/releases/tag/v0.1.0
