# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
