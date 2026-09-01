# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0] - 2026-09-01

### Added

- Scroll-sync between editor and preview (`src/preview.rs`, `wire_scroll_sync`
  in `src/window.rs`): scrolling the editor scrolls the preview to match,
  keyed by *source line* rather than scroll percentage - each rendered
  block carries a `data-line` attribute for its starting Markdown line, so
  a block that renders far taller than its one source line (an image, a
  large table) doesn't throw off the sync the way a naive proportional
  mapping would.
- Spell-checking in the editor via [`libspelling`](https://gitlab.gnome.org/GNOME/libspelling)
  (the GTK4-native successor to gspell, which was never ported to GTK4):
  squiggly-underlines misspelled words and adds correction suggestions to
  the editor's context menu, using the system's hunspell dictionaries.
- "Einstellungen" (settings) dialog: a proper `Adw.PreferencesDialog`
  (`src/settings.rs`) replacing the standalone WordPress-connection dialog,
  which is now one page within it (`connection::build_page`). Reachable
  from the header bar or Ctrl+, (moved off "Artikel-Eigenschaften", which
  now has no reserved accelerator, freeing Ctrl+, for its conventional
  GNOME meaning).

## [0.3.0] - 2026-09-01

Editor ergonomics and distribution: a formatting toolbar, a document
statistics view, category/tag autocomplete, in-app error notifications, and
a working Flatpak package.

### Added

- Grouped formatting toolbar above the editor (`src/formatting.rs`):
  cut/copy/paste, bold/italic/strikethrough, heading/quote/code/code block,
  lists, and link insertion, each button visually joined into its logical
  group ("linked" style). Keyboard shortcuts for bold (Ctrl+B), italic
  (Ctrl+I) and link (Ctrl+K); cut/copy/paste already had GtkSourceView's own
  bindings.
- "Statistik" tab next to "Vorschau" (`src/stats.rs`, `Adw.ViewStack` +
  `Adw.ViewSwitcher`, left-aligned): word/character/paragraph counts and
  estimated reading time, updating live alongside the preview.
- Autocomplete for the categories/tags fields in the properties dialog
  (`src/autocomplete.rs`): fetches existing terms from the configured
  WordPress site in the background and suggests matches in a popover as you
  type after the last comma.
- In-app error notifications (`Adw.ToastOverlay`) for file open/save
  failures, replacing silent `eprintln!` calls that only showed up in a
  terminal the user wasn't looking at.
- Flatpak packaging: manifest (`build-aux/flatpak/`), `.desktop` entry,
  AppStream metainfo, and an app icon under `data/`. Targets
  `org.gnome.Platform` 49, which already bundles GTK4/libadwaita/
  GtkSourceView5/WebKitGTK 6.0, so the only extra SDK piece needed is the
  Rust toolchain extension. Verified with a real `flatpak-builder` build,
  installed and run sandboxed.

## [0.2.0] - 2026-09-01

The app can now actually do the thing it's for: write Markdown, review the
generated Gutenberg HTML, and publish or update a real WordPress post.

### Added

- WordPress REST API client (`src/wpclient.rs`, blocking `ureq`-based):
  create/update posts, upload media, resolve-or-create category/tag terms
  by name, delete a post.
- "Artikel exportieren" dialog (`src/export.rs`): shows the generated
  Gutenberg block HTML before sending, then publishes/updates the post on a
  background thread (so the network round trip never blocks the UI) and
  writes the returned WordPress post id back into the document's
  frontmatter so a later export updates the same post instead of creating a
  duplicate. Reachable from the header bar or Ctrl+Shift+P.
- Local images referenced in the Markdown (`![alt](local/path.png)`) are
  uploaded to the WordPress media library at export time and the generated
  `wp:image` block is rewritten to point at the resulting hosted URL.
- Licensed under GPL-3.0-or-later (`LICENSE`, `license` field in both crates'
  `Cargo.toml`).

### Testing

- `wpclient` and `export` each have an `#[ignore]`d test exercising the full
  flow (category/tag resolution, media upload, create, update, delete)
  against a real, already-configured WordPress site rather than a mock —
  run explicitly with `cargo test -- --ignored`.

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

[Unreleased]: https://github.com/linuxundich/blocksmith/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/linuxundich/blocksmith/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/linuxundich/blocksmith/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/linuxundich/blocksmith/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/linuxundich/blocksmith/releases/tag/v0.1.0
