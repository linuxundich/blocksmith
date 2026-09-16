//! The "Gutenberg-Code" tab: a read-only view of the Gutenberg block-comment
//! HTML that `export.rs` would actually send, so it can be inspected
//! without opening the export dialog. Updates on the same debounce as the
//! preview and stats (see `window::wire_live_preview`).

use gtk4::prelude::*;

use crate::media;

pub struct CodeView {
    pub widget: gtk4::Widget,
    buffer: gtk4::TextBuffer,
}

impl CodeView {
    pub fn new() -> Self {
        let buffer = gtk4::TextBuffer::new(None::<&gtk4::TextTagTable>);
        let view = gtk4::TextView::builder()
            .buffer(&buffer)
            .editable(false)
            .monospace(true)
            .wrap_mode(gtk4::WrapMode::WordChar)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(12)
            .right_margin(12)
            .build();
        let scroller = gtk4::ScrolledWindow::builder().child(&view).vexpand(true).build();
        Self {
            widget: scroller.upcast(),
            buffer,
        }
    }

    /// `media` should be freshly reconciled against `markdown` (the caller
    /// already has to do this for the preview pane/Medienverwaltung, so
    /// this doesn't reconcile again itself) - its alt-text/caption edits
    /// are overlaid on top of the parsed blocks (`export::gutenberg_preview_html`),
    /// the same way `run_export` does right before actually publishing, so
    /// this tab shows what would really be sent rather than silently
    /// reverting to whatever's written literally in the Markdown source.
    pub fn update(&self, markdown: &str, media: &[media::MediaItem]) {
        self.buffer.set_text(&crate::export::gutenberg_preview_html(markdown, media));
    }
}
