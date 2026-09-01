//! The "Gutenberg-Code" tab: a read-only view of the Gutenberg block-comment
//! HTML that `export.rs` would actually send, so it can be inspected
//! without opening the export dialog. Updates on the same debounce as the
//! preview and stats (see `window::wire_live_preview`).

use gtk4::prelude::*;

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

    pub fn update(&self, markdown: &str) {
        self.buffer.set_text(&gutenberg::markdown_to_gutenberg(markdown));
    }
}
