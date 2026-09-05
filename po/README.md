# Translating Blocksmith

Blocksmith's source strings are **German** - that's the app's original
language, not a translation of anything - so a `.po` file here translates
*from* German *to* another language, the reverse of the usual gettext
convention where English is the source.

## Current status

Internationalization is set up and working end-to-end (locale detection,
catalog lookup, a real translation), but only applied to a representative
slice of the UI so far - not every string in the app yet:

- `src/window.rs` - the main window's header bar, primary menu, and tab
  titles
- `src/properties.rs` - the "Artikel-Eigenschaften" dialog
- `src/shortcuts.rs` - the "Tastenkürzel" window
- `src/document.rs` - `PostStatus`'s labels (used by `properties.rs`)

**Known limitation:** translations only actually load for a `cargo build`/
`cargo run` from this source tree right now - `src/i18n.rs` points
`bindtextdomain` at `po/locale` via `CARGO_MANIFEST_DIR`, a path baked in
at compile time that only exists on the machine that built the binary.
A real installed/Flatpak build would need the Flatpak manifest's install
step to place compiled `.mo` files under the app's own
`/app/share/locale/<lang>/LC_MESSAGES/blocksmith.mo` and `i18n::init()`
updated to bind there instead (or in addition) - not done yet.

`po/en.po` is a complete, real English translation of exactly those
strings, proving the pipeline works. Converting the rest of the app is the
same mechanical step repeated for each remaining file: wrap a literal in
`i18n::tr("...")` (see `src/i18n.rs`'s doc comment), add the file to
`POTFILES.in`, and re-extract/re-translate as below.

## Adding a translation for a new language

1. Copy the template and start from it:
   ```sh
   msginit --input=po/blocksmith.pot --locale=<lang> --output-file=po/<lang>.po
   ```
   (`<lang>` is a locale code like `fr`, `es`, `pt_BR`.)
2. Edit `po/<lang>.po`, filling in `msgstr` for each `msgid`.
3. `cargo build` (or `cargo run`) automatically compiles it to
   `po/locale/<lang>/LC_MESSAGES/blocksmith.mo` via `build.rs` - nothing
   else to wire up.
4. To see it without changing your desktop's language, run with the
   locale forced for just that one process:
   ```sh
   LANGUAGE=<lang> cargo run
   ```

## Updating `.pot`/existing `.po` files after changing source strings

Re-extract every marked string from the files listed in `POTFILES.in`:

```sh
xgettext --keyword=tr --language=C --from-code=UTF-8 \
  --package-name=Blocksmith --package-version="$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)" \
  --copyright-holder="Christoph Langner" \
  --msgid-bugs-address="https://github.com/linuxundich/blocksmith/issues" \
  -o po/blocksmith.pot $(cat po/POTFILES.in)
```

`--language=C` is deliberate: `xgettext` has no native Rust mode, but
Rust's `"..."` string literals are close enough to C's that this works for
the plain (non-raw, non-byte) strings `tr()` is ever called with. It will
print a harmless warning like `Zeichenkonstante nicht korrekt terminiert`
("unterminated character constant") for every `'static`/named-lifetime
annotation in a scanned file, since the C lexer misreads a lifetime's
leading `'` as starting a char literal - extraction still completes
correctly past it, so this specific warning can be ignored.

Then bring each existing `.po` up to date with any new/changed/removed
`msgid`s, preserving its existing translations:

```sh
msgmerge --update po/en.po po/blocksmith.pot
```

**Important - only strings that appear as a literal argument at the
`tr(...)` call site itself are found.** `xgettext` cannot see through a
variable: `tr(some_variable)` extracts nothing, even if `some_variable`
only ever holds one of a few known literals (e.g. from a `match`). Always
call `tr("the literal text")` directly at each place it's needed, even if
that means calling it once per `match` arm instead of once on a shared
variable afterward - see `document.rs`'s `PostStatus::label()` and
`shortcuts.rs`'s `group()` for the pattern.

## Validating a translation

```sh
msgfmt --check --statistics -o /dev/null po/<lang>.po
```

Reports syntax errors and how many strings are translated/fuzzy/untranslated.
