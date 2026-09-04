# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.18.0] - 2026-09-04

### Fixed

- Local images referenced in the article (`![alt](photo.png)`) never
  rendered in the preview - the `WebView` had no base URI to resolve a
  relative path against, so it simply couldn't find the file. The preview
  now defaults to searching for it in the article's own folder, updated
  automatically whenever the article is opened, saved for the first time,
  or reset ("Neu"/opened from WordPress).
- Removed "Zurück", "Vor", and "Anhalten" from the preview's right-click
  context menu - navigation history controls that never applied to a
  rendered article preview in the first place.

## [0.17.0] - 2026-09-04

### Added

- Medienverwaltung is now embedded directly in the "Artikel exportieren"
  dialog as a "Medien" tab next to "Vorschau", so alt text, captions, and
  WordPress upload state can be checked and fixed right before publishing,
  not just from the separate Ctrl+Shift+M dialog (which still exists
  unchanged).
- A new image's caption is now seeded from its Markdown bracket text
  (`![Bildunterschrift](bild.png)`) when no explicit `"title"` is present -
  previously the caption stayed empty unless the rarely-used quoted-title
  syntax was used, even though the bracket text is the only description
  most images ever get. The title, when present, still takes priority.

## [0.16.0] - 2026-09-04

### Added

- A second button, "Als Entwurf hochladen", in the "Artikel exportieren"
  dialog alongside "Veröffentlichen" - each sends its own status
  (draft/publish) explicitly, regardless of whatever the separate
  "Artikel-Eigenschaften" dialog's status field currently holds, so
  publishing directly vs. uploading a draft first is now an unambiguous
  choice made right in the publish dialog. Clicking either updates the
  same WordPress post (via its already-tracked id) rather than creating a
  new one, and both buttons are disabled together while a request is in
  flight so they can't race each other.

## [0.15.0] - 2026-09-04

### Fixed

- Publishing/updating an article re-uploaded every local image to
  WordPress's media library from scratch on every single export, creating
  a duplicate attachment each time - the automatic export-time upload path
  and the Medienverwaltung's per-image tracking were two disconnected
  systems, and the former also never sent alt text or caption. Unified:
  export now reconciles against the same tracked media list Medienverwaltung
  uses, and skips uploading an image whose content (SHA-256) still matches
  what's already on the server - "bei Bedarf hochladen", not on every
  export. A changed local image is uploaded as a new attachment (WordPress
  has no way to replace an existing one's file) with the superseded one
  then deleted, and its filename/alt text/caption are always sent together,
  whether the upload happens automatically at export time or manually via
  Medienverwaltung's "Zu WordPress hochladen".

## [0.14.0] - 2026-09-01

### Added

- Featured-image picker in "Artikel-Eigenschaften" - a file-picker button
  next to the field, matching the editor body's "Bild einfügen", instead
  of only being able to type a path by hand.
- Table-insert toolbar button - inserts a minimal 2x2 Markdown table
  template with the first header cell pre-selected. The Gutenberg engine
  already fully supported tables both directions; there was just no
  toolbar shortcut for writing one.
- "Bestehenden Artikel verlinken…" toolbar button - a searchable picker
  listing the site's existing posts by title; picking one inserts a real
  Markdown link (`[title](permalink)`) at the cursor, so cross-linking
  your own articles no longer means copying a URL by hand first.

## [0.13.2] - 2026-09-01

### Fixed

- Images whose filename or path contains a space were silently invisible
  everywhere: not recognized as an image in the preview, not picked up by
  Medienverwaltung, and not converted to a `wp:image` block on export -
  plain Markdown's `![alt](destination)` syntax stops parsing at the first
  unescaped space, so `![](my photo.png)` was just literal text. Fixed by
  wrapping such a destination in `<...>` (equally valid CommonMark, and
  transparently stripped back off by any compliant parser) wherever
  Blocksmith writes an image reference itself: "Bild einfügen"'s file
  picker, and the WordPress-import reverse converter.

## [0.13.1] - 2026-09-01

### Added

- "Bild einfügen…" button in the editor toolbar - opens a native file
  picker (filtered to images) and inserts a real Markdown image reference
  at the cursor, using a path relative to the document's own folder when
  possible. Previously the only way to reference an image was to type its
  filename by hand.
- The Medienverwaltung's caption field is now seeded from the Markdown
  image's optional `"title"` (`![alt](src "title")`) the first time an
  image is seen, the same way alt text is already seeded from the Markdown
  alt - once set, it's no longer overwritten by later Markdown edits, so
  edits made in the dialog itself always win.

## [0.13.0] - 2026-09-01

### Added

- Per-image media management ("Medienverwaltung", Strg+Umschalt+M): every
  image referenced in the article gets its own alt text, caption, and
  WordPress upload state, independent of the Markdown source.
  - Alt text is a three-state value, not a plain on/off: **not yet
    defined** (flagged by the "N von M Bildern haben noch keinen
    Alternativtext" hint), **deliberately empty** (for decorative images -
    an explicit switch, not treated as an error), or **defined text**.
  - Caption is a separate field from alt text - the app never derives one
    from the other.
  - "Zu WordPress hochladen" uploads the image via the real REST API,
    sends the file, filename, alt text and caption, and stores the
    resulting media id/URL so re-opening the article recognizes it as
    already uploaded and never re-uploads by accident. Upload state
    (not uploaded / uploading / uploaded / failed) is shown per image; a
    failed or slow upload never touches the locally held article.
  - Metadata is persisted alongside the rest of the document in the
    existing `.md` frontmatter (a new `media_json` line) - no new file
    format yet; this lays the groundwork for the planned `.bsm` project
    container.

## [0.12.0] - 2026-09-01

### Added

- "Von WordPress öffnen" now groups posts into "Entwürfe" and
  "Veröffentlicht" sections (plus a catch-all "Weitere" for pending/
  scheduled/private posts), instead of one flat list - drafts are shown
  first, since that's usually what you're looking for.

### Fixed

- Scroll-sync froze while scrolling through a long fenced code block:
  the whole block was a single scroll-sync anchor, so the preview stayed
  pinned to the block's first line for however many lines the block
  spanned, only jumping once you scrolled past it entirely. Each line
  inside a code block now gets its own anchor, so the preview tracks
  smoothly through long code samples too.

## [0.11.0] - 2026-09-01

### Added

- Font selection for both the editor and the preview, in "Erscheinungsbild"
  - each gets its own `Gtk.FontDialogButton` (family, size, weight and style
    all in the one native GNOME font picker) with a "reset" button, unset
    by default so the editor keeps the system monospace font and each
    preview style keeps its own typeface until customized.
  - A live sample accompanies each: the editor's is a small read-only
    Markdown source view (reflecting the current color scheme *and* font,
    porting GNOME Builder's `GbpEditoruiPreview` pattern - the same live
    sample it shows in both its "Erscheinungsbild" and "Fonts & Styling"
    pages); the preview's is a small rendered sample article, updating
    live as the style, font, or light/dark mode changes.

## [0.10.3] - 2026-09-01

### Fixed

- The toolbar row above the preview tabs didn't match the one above the
  editor - it used `Adw.HeaderBar` (its own themed background and height)
  while the editor's is a plain `Gtk.Box`, so the two never quite lined up
  (background color, row height, and the separator line beneath each) and
  the mismatch varied by theme/style. Replaced the `Adw.HeaderBar` with a
  plain `Gtk.Box` using the exact same margins as the editor's toolbar, so
  both rows are now visually identical in every theme.

## [0.10.2] - 2026-09-01

### Fixed

- Scroll-sync felt laggy and made the preview jump instead of scroll: the
  editor-to-preview sync was debounced (cancel-and-reschedule on every
  scroll event), which only ever fires once scrolling has stopped - the
  preview sat frozen for the whole scroll gesture, then snapped to the
  final position. Switched to a throttle (fires at once, then at most
  once per ~60ms while scrolling continues, with a trailing call so the
  final position is never dropped), so the preview now visibly tracks the
  editor throughout the scroll instead of only catching up afterward.

## [0.10.1] - 2026-09-01

### Changed

- Moved the preview style picker (Modern/Klassisch/Sepia) out of the
  Vorschau tab and into the "Erscheinungsbild" settings page, in a new
  "Vorschau" group next to the interface and editor color-scheme pickers -
  it's a look-and-feel setting, not something that needs to sit in the tab
  itself. Choosing a style there still applies immediately to an already-
  open preview, the same as the other appearance settings.

## [0.10.0] - 2026-09-01

### Added

- The preview now follows the app's light/dark mode - it previously always
  rendered as plain black-on-white regardless of theme. Colors are baked
  into the generated HTML per render (not left to a `prefers-color-scheme`
  media query), so it updates immediately when the theme changes, even
  without retyping.
- Three selectable preview styles - Modern (the previous look), Klassisch
  (serif, justified, indented paragraphs), and Sepia (warm sepia-toned,
  with its own light and dark variant) - picked from a dropdown above the
  Vorschau tab, persisted across restarts.
- A footer status bar showing word count and estimated reading time for
  the whole article, plus the same two numbers for the current selection
  whenever one is active.

### Fixed

- The right pane's tab switcher only visually grouped the *active* tab
  into a pill shape, leaving the others as loose, ungrouped buttons.
  Switched from `Adw.ViewSwitcher` to `Adw.InlineViewSwitcher`, which
  renders all tabs as one seamless linked pill, and packed it left-aligned
  in its header bar rather than as a centered title widget - this also
  surfaced and fixed a bug where, without an explicit title widget,
  `Adw.HeaderBar` fell back to showing the window's own title ("Blocksmith")
  as a stray extra tab-like segment next to the real ones.

## [0.9.2] - 2026-09-01

### Changed

- The "Erscheinungsbild" page now adopts GNOME Builder's actual
  implementation rather than an approximation of it: the interface-style
  cards use Builder's own bundled preview illustrations
  (`data/icons/appearance-preview/`, CC BY-SA 4.0, see the `ATTRIBUTION.md`
  there) instead of a hand-drawn CSS mockup, and the color-scheme grid uses
  GtkSourceView's `StyleSchemePreview` widget in a `GtkFlowBox` - the same
  widget and layout Builder's own scheme selector uses - filtered to the
  schemes matching the current light/dark mode (a Rust port of Builder's
  `ide_source_style_scheme_is_dark()` heuristic) instead of showing every
  installed scheme at once. The live code-sample preview added in 0.9.1 was
  removed again at the user's request - not something Blocksmith needs.

## [0.9.1] - 2026-09-01

### Fixed

- The "Erscheinungsbild" page's theme picker didn't actually look like GNOME
  Builder's - it used plain text toggle buttons instead of Builder's mini
  window-mockup preview cards, and the color-scheme swatch grid had no live
  code sample above it the way Builder's does. Rebuilt to match: each of
  "Dem System folgen"/"Hell"/"Dunkel" is now a card with a small mockup
  window (header strip, a couple of text-line bars, one accent-colored) -
  "Dem System folgen" shows a light/dark split - with the selected card
  getting an accent-colored border, and a live syntax-highlighted Rust
  sample now sits above the scheme swatch grid, updating immediately as a
  different scheme is picked.

## [0.9.0] - 2026-09-01

### Added

- **API-key verification and model discovery**: entering an API key in
  Einstellungen now calls the provider's `/models` endpoint on a background
  thread (debounced) as soon as you stop typing, reporting "✓ Verbindung
  erfolgreich" (with the number of models found) or the provider's error
  message directly - this call doubles as the model list for the "Modell"
  picker, which is now a searchable dropdown fed from what the account
  actually has access to (cached to disk so it's available offline
  afterward), rather than a free-text field. Ollama gets the same check
  keyed off its base URL instead of a key.
- **Model choice from the Chat tab itself**: a small provider/model row at
  the top of the tab lets you switch models without opening Einstellungen;
  the choice is persisted the same way as the settings picker.
- **Markdown-rendered replies**: the model's answers are now parsed as
  Markdown and shown as Pango markup in the bubble (bold/italic/strikethrough,
  inline code, fenced code blocks, links, headings, lists), instead of raw
  Markdown source; long unbroken tokens (URLs, code) now wrap correctly.
- **AI actions in the editor's context menu**: right-clicking the editor
  now offers "Übersetzen" (into any of a configurable list of target
  languages), "Inhalt prüfen", "Stil & Formatierung prüfen", "Rechtschreibung
  prüfen", "Zeichensetzung prüfen", and "Länge anpassen…" (a small dialog for
  the target word/character count and whether it should be hit exactly or
  approximately). Each sends the current selection - or, if nothing is
  selected, the whole article - to the Chat tab together with a matching
  prompt template, and switches to that tab to show the reply. A "KI-Prompts"
  settings page lets you edit any of these six built-in prompts (resettable
  to their defaults, independently of each other), edit the translation
  language list, and define your own custom prompts (title + template),
  kept in a separate "Eigene Prompts" group so they're never confused with
  the built-ins - both also show up in the context menu, live-updated as
  you edit them without restarting the app.
- The Einstellungen button in the header bar now uses the classic hamburger
  icon (`open-menu-symbolic`) instead of a gear.

### Fixed

- `Adw.ExpanderRow` titles (used for the prompt-editor rows) interpret
  their title as Pango markup by default; a literal `&` in a prompt's title
  (e.g. "Stil & Formatierung prüfen") crashed markup parsing with a GTK
  critical. Fixed by building these rows with `use_markup(false)`, since the
  titles are always plain text.

## [0.8.0] - 2026-09-01

### Added

- **Multiple LLM providers for the Chat tab**: alongside Gemini, the chat
  can now use ChatGPT (OpenAI), Claude (Anthropic), or Ollama (self-hosted,
  no API key). `src/gemini.rs` was replaced by `src/llm.rs`, a single
  blocking client (`llm::Client`) that speaks each provider's REST shape
  (Gemini's `generateContent`, OpenAI's `chat/completions`, Anthropic's
  `messages`, Ollama's `api/chat`). The "KI-Chat" settings page gained a
  provider picker; API keys are stored per-provider in the Secret Service
  (`secrets::store_llm_api_key`/`load_llm_api_key`, keyed by provider id),
  and each provider keeps its own model id, with Ollama additionally
  getting a configurable base URL (default `http://localhost:11434`). The
  shared system prompt is unaffected - it still applies regardless of which
  provider is active.
- A **Gutenberg-Code tab** (`src/codeview.rs`) next to "Vorschau", showing
  the exact block-comment-annotated HTML that would be published, updated
  live from the same debounced pipeline as the preview and statistics tabs.
- **GNOME Builder-style appearance settings** (`src/appearance.rs`): a new
  "Erscheinungsbild" page with a light/dark/follow-system toggle
  (`Adw.StyleManager`) and an editor color-scheme picker using
  GtkSourceView's own `StyleSchemeChooserWidget` (the same swatch-grid
  widget Builder itself uses) - both persisted under
  `~/.config/blocksmith/`.

### Fixed

- Scroll-sync between the editor and the preview never actually moved the
  preview. Root cause: `sourceview5::View::iter_at_location()` unreliably
  returns `None` at the buffer's left edge once the line-number gutter is
  shown. Replaced pixel-based line detection on the editor side with a
  fraction-based estimate (`estimate_visible_line`, unit-tested) - correct
  here because editor lines are uniform height, unlike the preview side
  (where the image-height mismatch this feature originally cared about is
  already handled separately via `data-line` snapping).
- Opening an existing WordPress post left its featured image blank on
  re-export. `Frontmatter` gained a `featured_media_id` field (the post's
  *current* featured media id, distinct from `featured_image`'s "upload
  this new local file" meaning) so importing a post and exporting it again
  without touching the featured image keeps the original one instead of
  silently dropping it.
- The placeholder chat tab icon (a nonexistent `chat-symbolic`) is now the
  real `chat-message-new-symbolic`; a full audit of every `-symbolic` icon
  name in the codebase against the installed Adwaita icon theme turned up
  no other placeholders.
- The right-hand tab switcher (Vorschau/Gutenberg-Code/Statistik/Chat) is
  now hosted in a real `Adw.HeaderBar`, the standard GNOME pattern (also
  used by Builder and Text Editor) for a properly grouped pill switcher,
  instead of a loose `Adw.ViewSwitcher` next to the pane.

## [0.7.0] - 2026-09-01

### Added

- Gemini-backed chat in a third "Chat" tab (next to "Vorschau"/"Statistik"),
  with message bubbles (`src/chat.rs`) - user messages right-aligned,
  Gemini's replies left-aligned, styled via libadwaita's named theme
  colors so they adapt to light/dark mode. Sends run on a background
  thread (`src/gemini.rs`, blocking `ureq` client for the Generative
  Language API, same rationale as `wpclient` for not using `reqwest`/
  `tokio`), keeping the full conversation history as context for each
  request.
- A "KI-Chat" page in Einstellungen (`src/chatsettings.rs`): Gemini API key
  (stored in the Secret Service via `secrets.rs`, never in plain text) and
  model id, plus a full editable/resettable system prompt
  (`src/chatconfig.rs`) - auto-saved as you type, with a "Zurücksetzen"
  button (enabled only when the prompt has actually been customized) that
  reverts to the built-in default. That default (`src/default_prompt.rs`)
  is a full editorial style guide for an anonymous German-language Linux/
  Open-Source tech writer, provided by the user.

## [0.6.0] - 2026-09-01

### Added

- Categories/tags cache (`src/termcache.rs`): loaded from disk at startup
  (`~/.cache/blocksmith/terms.json`) so the properties dialog's
  autocomplete has data immediately, refreshed automatically in the
  background at every launch, and refreshable on demand via a button next
  to "Artikel-Eigenschaften". Replaces the previous per-dialog-open fetch
  that started empty every time and was discarded on close.
- "Von WordPress öffnen" (`src/importer.rs`, header bar or Ctrl+Shift+O):
  lists existing posts on the configured site; picking one fetches its
  full content and metadata, resolves category/tag ids back to names, and
  converts the Gutenberg block HTML back to Markdown via the new
  `gutenberg::gutenberg_to_markdown` (the reverse of
  `markdown_to_gutenberg`, `crates/gutenberg/src/reverse.rs`) so the
  article opens as editable Markdown with its `wp_post_id` already set -
  exporting it afterward updates the same post. The reverse converter is a
  hand-rolled block-comment scanner (not a general HTML parser), with unit
  tests round-tripping every block type our forward converter produces,
  plus a live test creating a real post, fetching it back, and converting
  it - verifying actual WordPress storage/serving round-trips cleanly, not
  just in-memory conversion.
- `wpclient` gained `list_posts`, `get_post`, and `get_term_name` to back
  the above.

## [0.5.0] - 2026-09-01

### Added

- "Von WordPress löschen" button in the export dialog, shown once an
  article has a `wp_post_id` (i.e. it's been published/updated at least
  once). Asks for confirmation (`Adw.AlertDialog`, destructive-styled
  "Löschen" response) before permanently deleting the post via
  `wpclient::Client::delete_post`, then clears `wp_post_id` so a later
  export creates a fresh post instead of trying to update a deleted one.

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

[Unreleased]: https://github.com/linuxundich/blocksmith/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/linuxundich/blocksmith/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/linuxundich/blocksmith/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/linuxundich/blocksmith/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/linuxundich/blocksmith/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/linuxundich/blocksmith/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/linuxundich/blocksmith/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/linuxundich/blocksmith/releases/tag/v0.1.0
