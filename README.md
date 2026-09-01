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

Early development. Implemented so far:

- **Split-pane editor** — Markdown editing pane + debounced live HTML preview.
- **Gutenberg block engine** (`crates/gutenberg`) — a standalone, unit-tested
  library that parses Markdown into a block tree and renders it as
  block-comment-annotated HTML (`<!-- wp:paragraph -->...`), independent of
  the GUI.
- **Document model** — per-article frontmatter (title, slug, status,
  categories, tags, featured image, WordPress post id) stored in the `.md`
  file itself, editable via an "Artikel-Eigenschaften" dialog.
- **WordPress connection settings** — site URL/username stored in a small
  config file; the Application Password is stored in the Secret Service
  (GNOME Keyring) via [`oo7`](https://crates.io/crates/oo7), never written to
  disk in plain text.

Not yet implemented:

- Actually publishing/updating a post via the WordPress REST API, and
  uploading local images as media.
- Flatpak packaging.

## Building & running

Requires a Rust toolchain (stable) and the GTK4/libadwaita/GtkSourceView5/
WebKitGTK 6.0 development packages (available on any recent GNOME-based
Linux distribution).

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

## Versioning

Blocksmith follows [Semantic Versioning](https://semver.org/). The version
in `Cargo.toml` is the source of truth; see [CHANGELOG.md](CHANGELOG.md) for
what changed in each release. Before `1.0.0`, minor version bumps (`0.x.0`)
may still change the on-disk frontmatter format or other user-facing
behavior — check the changelog when upgrading.
