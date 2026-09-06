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

- **Broken-link checker.** Nothing in the app ever validates a URL - not
  Markdown links, not `wp:embed` sources. A "check links" action (in the
  export dialog, alongside the existing Medienverwaltung tab) that HEADs
  every unique link in the article and flags anything returning 4xx/5xx
  or timing out would catch real pre-publish mistakes (typo'd URLs,
  since-deleted pages) that currently ship silently.
- **Gutenberg block coverage: Columns, Buttons, Gallery.** The `Block`
  enum covers paragraph/heading/list/quote/code/image/video/audio/embed/
  separator/table plus a raw-HTML catch-all - genuinely common Gutenberg
  blocks like side-by-side columns, a call-to-action button, or a photo
  gallery have no representation at all and would either fall through to
  `wp:html` or not round-trip cleanly. Since Markdown has no native
  syntax for these, this needs a deliberate syntax choice (e.g. a fenced
  block like ` ```columns ` or a shortcode-style marker) - a real design
  decision to make before implementing, not just a mechanical addition.
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
- **SEO plugin field support (Yoast/RankMath).** No `meta` fields are
  ever sent in a post payload - a focus keyword, custom SEO title, and
  meta description for whichever SEO plugin the target site runs would
  need per-plugin field-name knowledge and is only valuable to a site
  that actually has one of those plugins active, unlike the plain
  `excerpt` quick win above.

**Why:** same reason as the original 2026-09-04 analysis - a periodic
fresh look grounded in the actual code, not a re-statement of ideas
already shipped or already rejected.
