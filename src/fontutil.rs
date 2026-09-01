//! Shared helper for turning a `Pango.FontDescription` (as produced by
//! `Gtk.FontDialogButton`, GNOME's standard font+size picker) into GTK4 CSS
//! declarations - used by both the editor's custom-font override
//! (`appearance.rs`) and the preview's (`preview.rs`).

use gtk4::glib::translate::IntoGlib;
use gtk4::pango;

/// `font-family`/`font-size`/`font-weight`/`font-style` declarations (no
/// selector, no braces) for `desc` - callers wrap this in their own rule,
/// e.g. `format!(".my-class {{ {} }}", fontutil::css_declarations(&desc))`.
pub fn css_declarations(desc: &pango::FontDescription) -> String {
    let mut css = String::new();

    if let Some(family) = desc.family() {
        css.push_str(&format!("font-family: \"{}\"; ", family.replace('"', "")));
    }

    let size_pt = f64::from(desc.size()) / f64::from(pango::SCALE);
    if desc.is_size_absolute() {
        css.push_str(&format!("font-size: {size_pt}px; "));
    } else {
        css.push_str(&format!("font-size: {size_pt}pt; "));
    }

    // Round to the nearest hundred - Pango's named weights (e.g. Book =
    // 380, Semilight = 350) don't all land on CSS's canonical 100-900
    // scale, but GTK's CSS engine only reliably recognizes values on it.
    let weight_num = ((f64::from(desc.weight().into_glib()) / 100.0).round() as i32).clamp(1, 9) * 100;
    css.push_str(&format!("font-weight: {weight_num}; "));

    let style_keyword = match desc.style() {
        pango::Style::Italic => "italic",
        pango::Style::Oblique => "oblique",
        _ => "normal",
    };
    css.push_str(&format!("font-style: {style_keyword};"));

    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_and_size_are_rendered() {
        let desc = pango::FontDescription::from_string("Fira Code 12");
        let css = css_declarations(&desc);
        assert!(css.contains("font-family: \"Fira Code\""), "{css}");
        assert!(css.contains("font-size: 12pt"), "{css}");
        assert!(css.contains("font-weight: 400"), "{css}");
        assert!(css.contains("font-style: normal"), "{css}");
    }

    #[test]
    fn bold_italic_are_rendered() {
        let desc = pango::FontDescription::from_string("Cantarell Bold Italic 11");
        let css = css_declarations(&desc);
        assert!(css.contains("font-weight: 700"), "{css}");
        assert!(css.contains("font-style: italic"), "{css}");
    }
}
