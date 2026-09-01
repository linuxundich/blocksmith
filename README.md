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

- **Split-pane editor** — Markdown editing pane (GtkSourceView, syntax
  highlighting, spell-checking via [`libspelling`](https://gitlab.gnome.org/GNOME/libspelling))
  with a grouped formatting toolbar (cut/copy/paste; bold/italic/
  strikethrough with Ctrl+B/I; heading/quote/code/code block; lists; link
  with Ctrl+K), a debounced live HTML preview kept in scroll-sync with the
  editor (matched by source line, not scroll percentage, so a tall image
  doesn't throw off the sync), and a "Statistik" tab (word/character/
  paragraph counts, estimated reading time) alongside it.
- **Gutenberg block engine** (`crates/gutenberg`) — a standalone, unit-tested
  library that parses Markdown into a block tree and renders it as
  block-comment-annotated HTML (`<!-- wp:paragraph -->...`), independent of
  the GUI.
- **Document model** — per-article frontmatter (title, slug, status,
  categories, tags, featured image, WordPress post id) stored in the `.md`
  file itself, editable via an "Artikel-Eigenschaften" dialog with
  autocomplete for existing WordPress categories/tags.
- **Einstellungen dialog** (`Adw.PreferencesDialog`, Ctrl+,) — currently a
  WordPress-connection page: site URL/username stored in a small config
  file, the Application Password stored in the Secret Service (GNOME
  Keyring) via [`oo7`](https://crates.io/crates/oo7), never written to disk
  in plain text.
- **Publishing** — an "Artikel exportieren" dialog shows the generated
  Gutenberg HTML, then creates/updates the WordPress post via its REST API
  on a background thread, uploading any locally-referenced images to the
  media library and resolving category/tag names to WordPress term ids
  (creating them if they don't exist yet). Once published, the same dialog
  offers a confirmed "Von WordPress löschen" to remove the post again.
- **Flatpak packaging** — manifest, desktop entry, AppStream metainfo, and
  icon under `data/` and `build-aux/flatpak/`.

## Building & running

Requires a Rust toolchain (stable) and the GTK4/libadwaita/GtkSourceView5/
WebKitGTK 6.0/libspelling development packages (available on any recent
GNOME-based Linux distribution). Spell-checking needs at least one hunspell
dictionary installed for it to have anything to check against.

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
