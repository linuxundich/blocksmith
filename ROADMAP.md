# Roadmap

A living backlog of candidate next features for Blocksmith, analyzed
2026-09-04 against the app's actual current state (not a re-statement of
old plans - every item below was checked against the code before being
listed as open). Not a commitment or a schedule, just a prioritized list
to pick from in a future session. See [CHANGELOG.md](CHANGELOG.md) for
what's already shipped.

## Quick wins (small scope, low risk)

- ~~**Paste an image from the clipboard.**~~ Done (see CHANGELOG.md) -
  `Ctrl+V` now saves a clipboard image into the article folder and
  inserts it, falling back to normal text paste when there's no image.
- ~~**Drag a local image file into the editor.**~~ Done (see
  CHANGELOG.md) - a `Gtk.DropTarget` on the editor view accepts one or
  more dropped files, same insertion logic as "Bild einfügen"/clipboard
  paste.
- **Reopen the last article on launch.** `recentfiles.rs` (v0.25.0)
  already tracks the most-recently-opened path - starting the app with
  that file already loaded (instead of a blank "Unbenannt" document) is
  now a small addition on top of infrastructure that already exists,
  rather than a new subsystem.
- ~~**Find & replace in the editor.**~~ Done (see CHANGELOG.md) - a Ctrl+F
  bar at the bottom of the editor, backed by GtkSourceView's own
  SearchContext/SearchSettings.
- ~~**`Gtk.ShortcutsWindow`.**~~ Done (see CHANGELOG.md) - "Tastenkürzel" in
  the primary menu, also reachable via Ctrl+?.
- ~~**"Weiterlesen" / more-tag support.**~~ Done (see CHANGELOG.md) - a
  lone `<!--more-->` line now exports as `wp:more`, with a toolbar button
  to insert it.
- ~~**Manage existing categories/tags** (rename/delete) from inside the
  app.~~ Done (see CHANGELOG.md) - a "Kategorien & Tags verwalten" dialog,
  reachable from Artikel-Eigenschaften, lists every term with a real WP
  term id and lets you rename or delete it.

## Moderate scope, higher value

- ~~**Local autosave / crash-recovery.**~~ Done (see CHANGELOG.md) - a
  debounced background snapshot now survives a crash or a forgotten save,
  with a restore-or-discard prompt on next launch.
- ~~**Scheduled publishing.**~~ Done (see CHANGELOG.md) - a `Future`
  status, a publish date/time field in Artikel-Eigenschaften, and a
  "Terminieren" button in the export dialog (refusing to export with no
  valid date, rather than let WordPress silently publish immediately).
- ~~**oEmbed / embed block support.**~~ Done (see CHANGELOG.md) - a bare
  URL alone on its own line exports as a real `wp:embed` block.
- **Multiple WordPress site profiles.** `wpsite.rs` holds exactly one
  site's URL/username - switching projects between two different
  self-hosted WordPress installs currently means re-entering the
  connection by hand each time.
- ~~**Video/audio media support.**~~ Done (see CHANGELOG.md) - local
  video/audio files are inserted the same way as images and export as
  real `wp:video`/`wp:audio` blocks.
- ~~**Image compression/format conversion before upload.**~~ Done (see
  CHANGELOG.md) - an oversized PNG/JPEG is downscaled and re-encoded
  (opaque PNGs converting to JPEG) before upload, via `gdk-pixbuf`.

## Larger / architectural

- **The `.bsm` project-file container** - a self-contained ZIP-based
  format (manifest + article + content + bundled media), with its own
  New/Open/Save/Save-As and a version-migration framework. Already
  explicitly deferred once before (per the original media-management
  spec) in favor of the simpler `.md`-plus-frontmatter approach this app
  still uses today - still valid, still a genuinely large undertaking,
  don't start without an explicit go-ahead.
- ~~**Internationalization (i18n) - infrastructure done, most strings
  still German.**~~ Done (see CHANGELOG.md) - essentially the whole UI is
  now wrapped in `i18n::tr("...")`, with a complete, real English
  translation (`po/en.po`, ~270 strings) proving it end to end. AI prompt
  content and proper nouns deliberately stay German/untranslated by
  design - see `po/README.md`. Still open: wiring a real installed/
  Flatpak build to find its compiled translations at all (`po/README.md`'s
  "Known limitation" - right now only a `cargo run` from source finds
  them).
- **A CI pipeline.** There is no `.github/workflows` (or any other CI) at
  all right now - `cargo build`/`test`/`clippy` only ever run locally,
  by hand, before a commit. Not a user-facing feature, but real
  protection against regressions that this project's pace of change
  would benefit from.
- ~~**Distraction-free / focus writing mode.**~~ Done (see CHANGELOG.md) -
  Ctrl+Shift+F hides the header bar, toolbar, and right-hand pane down to
  just the editor text.

## Deliberately not recommended

- **Multi-window support** - this app is built around one document, one
  site connection, and a lot of shared app-level state (chat history,
  cached terms, keyring-backed credentials); multi-window would touch
  nearly everything for a use case ("edit two articles side by side")
  the recent-files list and quick save/reopen already cover reasonably
  well.
- **A plugin/extension system** - over-engineering for a personal-scale
  tool with one active user; revisit only if that changes.

## New candidate features (2026-09-06 analysis)

The list above is essentially cleared out, so this is a fresh pass -
grounded in actual gaps found by reading `crates/gutenberg/src/lib.rs`,
`src/wpclient.rs`, `src/document.rs`, and `src/stats.rs`, not
speculation.

### Quick wins

- ~~**Post excerpt / meta description field.**~~ Done (see CHANGELOG.md) -
  a new "Auszug / Meta-Beschreibung" field in Artikel-Eigenschaften sends
  WordPress's own `excerpt` field on export and reads it back when opening
  an existing post.
- ~~**Readability hint in the Statistik tab.**~~ Done (see CHANGELOG.md) -
  a German-adapted Flesch reading-ease score (Amstad's formula) plus a
  qualitative label, computed from average sentence length and average
  syllables per word.

### Moderate scope, higher value

- ~~**Broken-link checker.**~~ Done (see CHANGELOG.md) - a "Links" tab in
  the export dialog scans every unique URL in the article (Markdown
  links/images plus bare-URL `wp:embed` lines) and HEADs each one
  on-demand, flagging anything outside 2xx/3xx or timing out.
- ~~**Gutenberg block coverage: Columns, Buttons, Gallery.**~~ Done (see
  CHANGELOG.md) - fenced code blocks with a special "language" tag
  (` ```columns `, ` ```buttons `, ` ```gallery `) since Markdown has no
  native syntax for these; full round-trip both ways, and local images
  inside a columns/gallery block now participate in Medienverwaltung's
  alt-text/upload tracking like any other image.
- **Edit WordPress Pages, not just Posts.** `wpclient.rs` is hardcoded to
  the `posts` endpoint throughout; there's no `pages` support and no
  concept of post type in `Frontmatter` at all. A post-type picker in
  Artikel-Eigenschaften (defaulting to "Artikel", same as today) that
  swaps the REST endpoint would open the app to static/about-style pages
  without disrupting the current posts-only flow.

### Larger / architectural

- **Revision-conflict awareness.** Re-exporting an already-published post
  always overwrites it outright - there's no check for whether the post
  changed on the server since it was last fetched (e.g. someone edited it
  live in wp-admin in the meantime). WordPress's REST API exposes
  revisions; comparing the fetched-at content hash against the current
  server content before an update, and warning rather than silently
  clobbering, would close a real (if rare) data-loss risk.
- **Paste rich text as Markdown.** Clipboard paste today only special-
  cases an image (see `wire_paste_image_shortcut`); pasting formatted
  text copied from Google Docs, Word, or a webpage lands as plain,
  unformatted text or raw HTML, not Markdown. Detecting an HTML clipboard
  format and converting it to Markdown on paste would make drafting from
  outside sources far less lossy - a genuinely bigger feature (HTML→MD
  conversion, GTK clipboard format negotiation) than anything above.
- ~~**SEO plugin field support (RankMath).**~~ Done (see CHANGELOG.md) -
  a "RankMath SEO" group in Artikel-Eigenschaften (SEO-Titel,
  SEO-Beschreibung, Fokus-Keyword) sends RankMath's own post meta keys on
  export and reads them back on import. Scoped to RankMath specifically,
  per the user's request - Yoast's equivalent fields use different meta
  key names and would need their own, separate mapping if ever wanted.

**Why:** same reason as the original 2026-09-04 analysis - a periodic
fresh look grounded in the actual code, not a re-statement of ideas
already shipped or already rejected.

## GNOME/Linux-specific candidate features (2026-09-06 analysis)

The lists above are about the editor/WordPress side. This pass asked a
different question: which platform integrations is Blocksmith missing
that a GNOME/Flatpak-native app is expected to have? Checked against
`data/de.christophlangner.Blocksmith.desktop`,
`build-aux/flatpak/de.christophlangner.Blocksmith.json`, and `src/i18n.rs`
- not speculation.

### Quick wins

- ~~**`.desktop` file has no `MimeType=`.**~~ Done (see CHANGELOG.md) -
  `MimeType=text/markdown;` plus `Gio::ApplicationFlags::HANDLES_OPEN`
  and a `Gio::Application::connect_open` handler, so double-clicking a
  `.md` file (or "Open With" → Blocksmith) in Nautilus works, loading into
  the already-running window rather than spawning a second one.
- ~~**Opened articles never reach `Gio::RecentManager`.**~~ Done (see
  CHANGELOG.md) - opening or saving a file now also registers it with
  `Gtk.RecentManager` (GTK4 kept the type in the `gtk` namespace, not
  `gio`), alongside `recentfiles.rs`'s own private list.
- ~~**No desktop notification for a background action finishing.**~~ Done
  (see CHANGELOG.md) - a new `notify.rs` sends a `Gio::Notification` when
  publishing, an image upload, or a link check finishes while the app has
  no focused window, gated so it doesn't double-announce something
  already visible in an open dialog.

### Moderate scope, higher value

- **Flatpak sandbox is wider than the app needs.** The manifest's
  `finish-args` includes `--filesystem=host:rw` - full read/write access
  to the entire home directory (and beyond) with no portal in between.
  Every file operation in the app already goes through `gtk4::FileDialog`
  (confirmed via grep - `window.rs`, `properties.rs`), which is portal-
  backed and sandbox-safe on its own; the broad `host:rw` grant looks like
  a leftover from before that, not something the app's actual file access
  pattern requires. Narrowing or dropping it would make Blocksmith an
  honestly-sandboxed Flatpak instead of one that only nominally is - this
  is also a concrete blocker Flathub's own review process flags for new
  submissions.
- **A GNOME Shell search provider.** Implementing the
  `org.gnome.Shell.SearchProvider2` D-Bus interface over the same data
  `recentfiles.rs` already tracks would let a partial article title typed
  into the Activities Overview jump straight to opening that file in
  Blocksmith - a small D-Bus service on top of existing state, not a new
  subsystem, and the kind of integration that makes a GNOME app feel like
  it belongs on the desktop rather than being "a Linux port."
- **A `~/Templates` entry for Nautilus's "New Document."** Nautilus's
  right-click "New Document" submenu is populated straight from files
  placed in `~/Templates`; shipping a `.md` template there (with the
  standard frontmatter block already filled in) would let a new article
  be started from the Files app directly, without opening Blocksmith
  first.

### Larger / architectural

- ~~**Adaptive/narrow-width layout via libadwaita breakpoints.**~~ Done
  (see CHANGELOG.md) - an `Adw.Breakpoint` at ~700sp switches the
  editor+preview split from a side-by-side `Gtk.Paned` to a single pane
  at a time (an `Adw.InlineViewSwitcher`, matching the sidebar's own tab
  switcher) via `Adw.MultiLayoutView`/`Adw.LayoutSlot`, and reverts
  automatically above that width. Getting this to actually fire on
  resize (rather than being permanently blocked by the wide layout's own
  minimum size) also required making the `Gtk.Paned` and both
  `Adw.ViewStack`s properly shrinkable/non-homogeneous, and wrapping the
  formatting toolbar in a horizontally-scrolling container so nothing
  becomes unreachable at narrow widths.

**Why:** the earlier 2026-09-04/2026-09-06 passes were both scoped to the
editor/WordPress domain; this pass asked specifically what's missing on
the "feels native on GNOME/Linux" axis instead, per the user's request.
