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
