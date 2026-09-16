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
- ~~**Reopen the last article on launch.**~~ Done (see CHANGELOG.md) - a
  plain launch (no file argument) now reopens the most recently opened/
  saved article via `recentfiles::load()`, instead of always starting at
  a blank "Unbenannt" document; `Ctrl+N` still gets to a blank one in one
  step.
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
- ~~**A CI pipeline.**~~ Done (see CHANGELOG.md) -
  `.github/workflows/ci.yml` runs `cargo build`/`test`/`clippy` on every
  push/PR, in an `archlinux:latest` container (not Ubuntu's default
  runner image - its packages lag behind the fairly recent GNOME stack
  this app links against) so dependency versions never need separate
  upkeep.
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

## New candidate features (2026-09-16 analysis)

Grounded in actual gaps found by reading `document.rs`, `wpclient.rs`,
`mediapanel.rs`, `media.rs`, and `crates/gutenberg/src/lib.rs` - not
speculation. The accessibility-focused work done this cycle (real
per-image alt text/captions reaching the published post, hover tooltips,
context-sensitive menus) surfaced a couple of these directly.

### Quick wins

- ~~**The featured image never gets an alt text on WordPress.**~~ Done
  (see CHANGELOG.md) - a new "Alt-Text für Aufmacherbild" field in
  Artikel-Eigenschaften, sent as the resulting attachment's `alt_text` on
  upload, the same way a body image's already was.
- ~~**No warning for an unusually long alt text.**~~ Done (see
  CHANGELOG.md) - a non-blocking warning icon/tooltip once an entry (body
  image, featured image, or the quick-edit dialog) passes WCAG's ~150
  character guidance, matching the Statistik tab's own readability-tips
  pattern.
- ~~**No WordPress `private` post status.**~~ Done (see CHANGELOG.md) - a
  `Private` variant, a `ComboRow` entry, and a "Privat veröffentlichen"
  button in the export dialog, the same shape as `Future`/"Terminieren".

### Moderate scope, higher value

- **Categories are flat - no parent/child hierarchy.**
  `Client::resolve_or_create_term` always creates a new category as
  top-level (`{"name": name}`, no `parent`), and the "Kategorien"
  field is a plain comma-separated text entry with no way to express
  "this one's a child of that one" - even though WordPress categories
  (unlike tags) are genuinely hierarchical, and a site that already
  organizes them that way (e.g. this project's own linuxundich.de, per
  its "Netz-/Politik" category) can't have a new sub-category created
  from inside the app at all.
- **No way to set the post author.** Neither `Frontmatter` nor
  `wpclient.rs` has any concept of a post's `author` - every post is
  always attributed to whichever user the configured Application
  Password belongs to. Only matters on a multi-author site, but there's
  currently no way around it even there.
- **No way to reuse an image already in the WordPress media library.**
  Every image reference has to be a local file - `wpclient.rs` has
  `upload_media` but nothing like `list_media`, so a graphic already
  uploaded once (a shared header image, a recurring banner) can only be
  referenced by re-uploading the local file again, creating a duplicate
  attachment, rather than picking the existing one by browsing the
  library.
- **No control over comment status.** `Frontmatter` has no
  `comment_status` field - there's no way to publish a post with
  comments closed (or reopen them) from inside the app; wp-admin is the
  only way today.

### Larger / architectural

- ~~**More Gutenberg blocks via the existing fenced-block pattern.**~~
  Done (see CHANGELOG.md) - ` ```pullquote ` and ` ```details ` join
  `columns`/`buttons`/`gallery`, same `+++`-split fenced-block mechanism,
  full round-trip both ways.
- ~~**A real preview link for an unpublished draft.**~~ Done (see
  CHANGELOG.md) - a "Vorschau öffnen" button in the export dialog opens
  WordPress's `?preview=true` link in the app's own Browser tab. As
  suspected going in, it only actually shows the live preview if that
  tab's WebKit session already happens to be logged into wp-admin -
  there's no separate authenticated path, so an un-logged-in Browser tab
  just shows a login page instead, which the button's tooltip and a
  status message both call out up front rather than papering over it.

**Why:** same reason as the earlier passes - a periodic fresh look
grounded in the actual code, this time prompted directly by the
accessibility gaps the alt-text/caption work this cycle kept surfacing.
