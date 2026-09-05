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
- **Drag a local image file into the editor.** Same underlying insert
  logic as "Bild einfügen" and clipboard paste, triggered by a
  `Gtk.DropTarget` on the editor view instead of a button or a keystroke.
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
- **Manage existing categories/tags** (rename/delete) from inside the
  app. Right now taxonomy terms are only ever *read* (autocomplete) or
  *created* (automatically, during publish, for a name that doesn't exist
  yet) - there's no UI to fix a typo in an existing term or remove one.

## Moderate scope, higher value

- ~~**Local autosave / crash-recovery.**~~ Done (see CHANGELOG.md) - a
  debounced background snapshot now survives a crash or a forgotten save,
  with a restore-or-discard prompt on next launch.
- **Scheduled publishing.** `document::PostStatus` only has
  `Draft`/`Pending`/`Publish` - no `Future` status with a date/time
  picker, so a post can't be scheduled for later from inside the app even
  though WordPress itself supports it.
- **oEmbed / embed block support.** A bare URL on its own line (YouTube,
  Twitter/X, etc.) is exactly how WordPress's own editor already expects
  an embed to be written - `crates/gutenberg` currently has no `wp:embed`
  block at all, so such a URL only ever survives as a plain link inside a
  paragraph. Very natural fit for how people already write Markdown.
- **Multiple WordPress site profiles.** `wpsite.rs` holds exactly one
  site's URL/username - switching projects between two different
  self-hosted WordPress installs currently means re-entering the
  connection by hand each time.
- **Video/audio media support.** `media.rs`/Medienverwaltung and the
  Gutenberg engine only know about images; there's no upload workflow or
  dedicated block for either, so a `<video>`/`<audio>` tag only survives
  as an opaque `wp:html` passthrough.
- **Image compression/format conversion before upload.** A large,
  unoptimized screenshot uploads exactly as-is today; a "downscale/
  convert to WebP before sending" step (even just above some size
  threshold) would be a meaningful, low-effort quality-of-life win, and
  the app already owns the whole image-upload pipeline (`media.rs`,
  `sync_uploads`) this would slot into.

## Larger / architectural

- **The `.bsm` project-file container** - a self-contained ZIP-based
  format (manifest + article + content + bundled media), with its own
  New/Open/Save/Save-As and a version-migration framework. Already
  explicitly deferred once before (per the original media-management
  spec) in favor of the simpler `.md`-plus-frontmatter approach this app
  still uses today - still valid, still a genuinely large undertaking,
  don't start without an explicit go-ahead.
- **Internationalization (i18n).** Every UI string is hardcoded German
  text throughout the whole codebase; making the app translatable
  (`gettext` + a `po/` directory - planned in the very first project
  outline, never started) is a real, substantial step, only worth taking
  if this is ever meant to reach a non-German-speaking audience.
- **A CI pipeline.** There is no `.github/workflows` (or any other CI) at
  all right now - `cargo build`/`test`/`clippy` only ever run locally,
  by hand, before a commit. Not a user-facing feature, but real
  protection against regressions that this project's pace of change
  would benefit from.
- **Distraction-free / focus writing mode.** Hide the toolbar, header
  bar, and right-hand pane down to just the editor text, for long-form
  drafting. A bigger UI undertaking than the "collapse the preview pane"
  toggle already shipped in v0.24.0, since it would need to also fold
  away the toolbar/header, not just the right pane.

## Deliberately not recommended

- **Multi-window support** - this app is built around one document, one
  site connection, and a lot of shared app-level state (chat history,
  cached terms, keyring-backed credentials); multi-window would touch
  nearly everything for a use case ("edit two articles side by side")
  the recent-files list and quick save/reopen already cover reasonably
  well.
- **A plugin/extension system** - over-engineering for a personal-scale
  tool with one active user; revisit only if that changes.
