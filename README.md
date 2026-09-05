# Blocksmith

A GNOME (GTK4 + libadwaita) editor for writing blog articles in Markdown
and exporting them as native WordPress **Gutenberg blocks** — not a single
classic/freeform HTML block, but real, individually editable blocks
(`core/paragraph`, `core/heading`, `core/list`, ...) — to a self-hosted
WordPress site via its REST API.

Split-screen editing: Markdown on the left (GtkSourceView, syntax
highlighting), a live HTML preview on the right (WebKit), with Gutenberg
export running in the background against your own WordPress install using
an [Application Password](https://make.wordpress.org/core/2020/11/05/application-passwords-integration-guide/).

## Status

Functionally complete for its core purpose - write Markdown, review a live
preview, and publish/update a real WordPress post as native Gutenberg
blocks. Implemented so far:

- **Split-pane editor** — the window remembers its size (and whether it was
  maximized) across restarts, always opening with the editor/preview split
  exactly 50/50 regardless of that size; a header-bar toggle button
  collapses the whole right-hand pane for a full-width editor and restores
  it again. A second toggle button next to it (Ctrl+Shift+F) is a
  Fokus-Schreibmodus, additionally hiding the header bar and the editor's
  own formatting toolbar down to just the editor text - `Adw.ToolbarView`'s
  own animated reveal handles the header/status bar, so entering and
  leaving is a smooth slide rather than an abrupt layout jump. A "Zuletzt
  geöffnet" button next to "Öffnen" lists the most
  recently opened/saved articles (most-recent-first) for one-click
  reopening. Markdown editing pane (GtkSourceView, syntax
  highlighting, spell-checking via [`libspelling`](https://gitlab.gnome.org/GNOME/libspelling))
  with a grouped formatting toolbar (cut/copy/paste; bold/italic/
  strikethrough with Ctrl+B/I; heading/quote/code/code block; lists; table;
  link with Ctrl+K; "Bild einfügen" opening a native image file picker
  instead of typing a filename by hand; "Bestehenden Artikel verlinken"
  opening a searchable picker over the site's existing posts and inserting
  a real Markdown link to the one picked; pasting an image straight from the
  clipboard with Ctrl+V - a screenshot, or "Copy Image" from a browser -
  saves it into the article's own folder and inserts it, falling through to
  a normal text paste when there's no image on the clipboard; dragging one or
  more local files from a file manager onto the editor inserts them the same
  way, even in an unsaved article; a "Weiterlesen"
  button inserting WordPress's `<!--more-->` marker, exported as a real
  `wp:more` block rather than generic HTML), a Ctrl+F search-and-replace bar
  sliding up from the bottom of the editor (live match highlighting and
  count, next/previous navigation, replace one or all), a debounced live HTML preview kept in scroll-sync with the
  editor (matched by source line, not scroll percentage, so a tall image
  doesn't throw off the sync), and a footer status bar with word count and
  reading time for the whole article - plus the same two numbers for the
  current selection, whenever one is active. The right pane's tabs (an
  `Adw.InlineViewSwitcher`, rendering all tabs as one seamless linked
  pill, in a toolbar row matching the editor's) are "Vorschau" (follows the app's light/dark
  mode, with a choice of Modern/Klassisch/Sepia typographic styles picked
  in Einstellungen; every image gets small badges in its bottom-right
  corner - an upload arrow once it's on WordPress, "Alt" once its alt text
  is defined, and its file format - in that fixed order whenever more than
  one applies, updated live from Medienverwaltung/the alt-text dialogs, not
  just on the next edit), "Gutenberg-Code" (the exact block HTML that would be published),
  "Statistik" (word/character/paragraph counts, estimated reading time),
  and "Chat" - a writing assistant with message bubbles (replies rendered
  as Markdown), backed by Gemini, ChatGPT, Claude, or Ollama (self-hosted,
  no API key), with a provider/model picker both in the tab itself and in
  Einstellungen. A message typed here gets the editor's current selection -
  or, if nothing's selected, the whole article - appended before it's sent,
  the same rule the context menu's AI actions below already follow, so the
  model always has the article as context without pasting it in by hand.
- **AI actions in the editor's context menu** — right-click the editor for
  "Inhalt prüfen", "Stil & Formatierung prüfen", "Rechtschreibung prüfen",
  "Zeichensetzung prüfen", and "Länge anpassen…"; each sends the selection
  (or the whole article, if nothing's selected) to the Chat tab with a
  matching prompt. All five built-in prompts are editable/resettable in a
  "KI-Prompts" settings page, which also holds your own custom prompts
  (kept separate from the built-ins), reflected in the context menu
  immediately as you edit them.
- **Gutenberg block engine** (`crates/gutenberg`) — a standalone, unit-tested
  library that parses Markdown into a block tree and renders it as
  block-comment-annotated HTML (`<!-- wp:paragraph -->...`), independent of
  the GUI. A local image/video/audio file referenced with `![]()` becomes
  the matching `wp:image`/`wp:video`/`wp:audio` block by its file
  extension, and a bare URL alone on its own line becomes a real
  `wp:embed` block (YouTube, X/Twitter, Vimeo, Instagram, SoundCloud,
  Spotify recognized by name for a nicer immediate block-editor preview;
  any other URL still embeds generically, the same way WordPress's own
  editor falls back to oEmbed discovery for it).
- **Document model** — per-article frontmatter (title, slug, status,
  scheduled publish date/time, categories, tags, featured image, WordPress
  post id) stored in the `.md` file itself, editable via an
  "Artikel-Eigenschaften" dialog with autocomplete for existing WordPress
  categories/tags (backed by an on-disk cache, `src/termcache.rs`,
  refreshed at startup and on demand) and a native file picker for the
  featured image, not just a path field. A "Kategorien & Tags verwalten"
  dialog next to that autocomplete's refresh button lists every existing
  category/tag and lets you rename or permanently delete one straight
  from the app, instead of only ever being able to read or auto-create a
  term.
- **Local autosave / crash-recovery** — while the article has unsaved
  changes, a debounced background snapshot is kept in a local recovery
  slot; if Blocksmith is closed without saving (or crashes), the next
  launch offers to restore it, or discard it.
- **Von WordPress öffnen** (Ctrl+Shift+O) — pick an existing post from the
  configured site, grouped into "Entwürfe" and "Veröffentlicht" (drafts
  first), and edit it as Markdown: `crates/gutenberg`'s reverse converter
  turns its Gutenberg block HTML back into Markdown, categories/tags are
  resolved from ids back to names, and the post's id carries over so
  exporting afterward updates that same post instead of creating a
  duplicate.
- **Medienverwaltung** (Ctrl+Shift+M, also embedded as a "Medien" tab in
  the "Artikel exportieren" dialog next to "Vorschau" so it can be checked
  right before publishing, and reachable per-image via "Alternativtext
  festlegen…" in the editor's right-click context menu) — every image referenced in the
  article gets its own alt text, caption, and WordPress upload state,
  independent of the Markdown source (persisted alongside the rest of the
  document in the frontmatter). Alt text is a three-state value rather
  than a plain on/off: not yet defined (flagged by a non-blocking "N von M
  Bildern haben noch keinen Alternativtext" hint), deliberately left empty
  for decorative images (not treated as an error), or defined text; the
  caption is a separate field, never derived from the alt text, though it
  is seeded the first time an image is seen from the Markdown image's
  optional `"title"` (`![alt](src "title")`) if present, else from its
  bracket text (`![Bildunterschrift](src)`) - most images never get the
  quoted-title form, so the bracket text is usually the only description
  there is. A "Zu WordPress hochladen" button
  per image uploads it via the real REST API and stores the resulting
  media id/URL so re-opening the article recognizes it as already
  uploaded rather than re-uploading it. A "Bild einfügen…" button in the
  editor toolbar opens a native image file picker and inserts a real
  Markdown image reference at the cursor (relative to the document's own
  folder when possible) - previously the only way to add an image
  reference was to type its filename by hand. A "Video/Audio einfügen…"
  button next to it does the same for local video/audio files - the same
  `![]()` reference, just picked from a video/audio file filter instead. A right-click on an image -
  its Markdown line in the editor, or the rendered image itself in the
  Vorschau pane - also offers "KI-Alternativtext generieren…": the active
  KI-Chat provider looks at the real image and proposes an accessible alt
  text at a choice of three detail levels (Standard/Ausführlich/Hohe
  Genauigkeit), shown for review and correction before it's applied
  directly into the same alt-text field, ready for the next upload.
- **Einstellungen dialog** (`Adw.PreferencesDialog`, Ctrl+,) — an
  "Erscheinungsbild" page adopted directly from GNOME Builder's own
  implementation (light/dark/follow-system cards using Builder's bundled
  preview illustrations, and an editor color-scheme grid using
  GtkSourceView's `StyleSchemePreview` widget filtered to schemes matching
  the current light/dark mode, the same widget and filtering Builder uses,
  plus the article preview's own typographic style picker), independent
  font pickers for the editor and the preview (family/size/weight/style,
  each with a live sample and a reset-to-default button), a WordPress-connection page (site
  URL/username in a small config file, the Application Password in the
  Secret Service via [`oo7`](https://crates.io/crates/oo7), never written
  to disk in plain text), a "KI-Chat" page (a provider picker for
  Gemini/ChatGPT/Claude/Ollama, each with its own API key - verified live
  against the provider's API as soon as it's entered, and saved
  automatically once that check succeeds, with no separate save button -
  and a model picker populated from that account's actual available
  models, Ollama additionally getting a configurable base URL; plus a
  fully editable, resettable system prompt shared across all providers),
  and a "KI-Prompts"
  page (the context menu's five built-in prompts and custom prompts - see
  above).
- **Publishing** — an "Artikel exportieren" dialog shows the generated
  Gutenberg HTML, then creates/updates the WordPress post via its REST API
  on a background thread. "Veröffentlichen" and "Als Entwurf hochladen"
  each send their own status explicitly, independent of whatever the
  "Artikel-Eigenschaften" dialog's status field currently holds - so
  publishing directly vs. uploading a draft first is an unambiguous choice
  made right in this dialog, and either one updates the same tracked post
  rather than creating a new one. A third "Terminieren" button appears once
  "Geplant" is picked in Artikel-Eigenschaften with a valid publish
  date/time set there - exporting without one is refused with a clear
  error rather than silently publishing immediately, which is what
  WordPress itself does with a scheduled status and no real future date.
  Category/tag names are resolved to
  WordPress term ids (creating them if they don't exist yet). Locally-referenced images
  are uploaded to the media library "bei Bedarf" (as needed), sharing the
  same tracked media list Medienverwaltung uses: an image whose content
  hash still matches what's already on the server is reused rather than
  re-uploaded, and a changed one is uploaded as a new attachment with the
  superseded one cleaned up automatically, since WordPress can't replace
  an existing attachment's file in place. An oversized PNG/JPEG (above
  ~300 KB) is downscaled to at most 2000px on its longer edge and
  re-encoded before upload - an opaque PNG (no transparency) converts to
  JPEG, since that's usually far smaller for a photo/screenshot saved as
  PNG - via `gdk-pixbuf`, already part of the GTK stack this app links
  against. An already-small image, or a result that somehow isn't smaller
  than the original, is uploaded completely unchanged; only what's *sent*
  is ever affected, never the local file. Once published, the same dialog
  offers a confirmed "Von WordPress löschen" to remove the post again.
- **Primary menu** (the header bar's hamburger button) — "Einstellungen",
  "Tastenkürzel" (a native `Gtk.ShortcutsWindow`, also reachable via
  Ctrl+?), and "Über Blocksmith", the latter a native `Adw.AboutDialog`
  with the version (always in sync with `Cargo.toml`), GPL-3.0-or-later
  license text, issue tracker/repository links, and the full
  `CHANGELOG.md` history as its browsable "Neuigkeiten" release notes.
- **Internationalization** — translatable via GNU gettext (`gettext-rs`).
  Source strings are German (the app's original language); `po/en.po` is a
  complete English translation proving the pipeline works end to end
  (`build.rs` compiles every `po/*.po` into a `.mo` catalog on every
  build, picked up automatically by a `cargo run` from this source tree).
  Only a representative slice of the UI is converted so far - the main
  window's header bar/menu/tabs, "Artikel-Eigenschaften", and
  "Tastenkürzel" - see `po/README.md` for the full translator/contributor
  workflow and how to convert more of the app.
- **Flatpak packaging** — manifest, desktop entry, AppStream metainfo, and
  icon under `data/` and `build-aux/flatpak/`.

## Building & running

Requires a Rust toolchain (stable) and the GTK4/libadwaita/GtkSourceView5/
WebKitGTK 6.0/libspelling development packages (available on any recent
GNOME-based Linux distribution). Spell-checking needs at least one hunspell
dictionary installed for it to have anything to check against. GNU
gettext's `msgfmt` (for compiling `po/*.po` translations - see
`po/README.md`) is optional: `build.rs` only prints a build warning and
skips it if not found, and the app runs fine without it, just always
showing its original German source strings.

```sh
cargo build
cargo run
```

## Testing

```sh
cargo test --workspace
```

A few tests exercise real system services (e.g. the Secret Service via
`oo7`) rather than mocks, and are marked `#[ignore]` so a normal test run
doesn't depend on your desktop's state. Run those explicitly with:

```sh
cargo test --workspace -- --ignored
```

## Packaging (Flatpak)

The manifest at `build-aux/flatpak/de.christophlangner.Blocksmith.json`
targets `org.gnome.Platform` 49, which already bundles GTK4, libadwaita,
GtkSourceView5 and WebKitGTK 6.0 - no extra runtime modules needed, only the
`org.freedesktop.Sdk.Extension.rust-stable` SDK extension for the Rust
toolchain itself:

```sh
flatpak install flathub org.gnome.Platform//49 org.gnome.Sdk//49 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
cd build-aux/flatpak
flatpak-builder --force-clean --user --install build-dir \
  de.christophlangner.Blocksmith.json
```

Builds run fully offline inside the sandbox against vendored crate sources
listed in `cargo-sources.json`. That file is generated from `Cargo.lock` -
regenerate it whenever dependencies change, using the
[flatpak-cargo-generator](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo)
script:

```sh
python3 flatpak-cargo-generator.py ../../Cargo.lock -o cargo-sources.json
```

## Versioning

Blocksmith follows [Semantic Versioning](https://semver.org/). The version
in `Cargo.toml` is the source of truth; see [CHANGELOG.md](CHANGELOG.md) for
what changed in each release. Before `1.0.0`, minor version bumps (`0.x.0`)
may still change the on-disk frontmatter format or other user-facing
behavior — check the changelog when upgrading.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
