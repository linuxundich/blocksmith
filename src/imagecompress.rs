//! Downscales/re-encodes a local image before it's *uploaded* to
//! WordPress, if doing so is actually likely to help - an already-small,
//! already-reasonably-sized image is uploaded completely unchanged. Only
//! ever changes the bytes/filename/mime type *sent*; the local file and
//! the article's own `![]()` reference are never touched.
//!
//! Built on `gdk-pixbuf` (already part of the GTK stack this app links
//! against - no new image-codec dependency) rather than a general-purpose
//! Rust image crate.

use gdk_pixbuf::prelude::*;

/// A screenshot saved straight from most tools easily runs several MB and
/// several thousand pixels wide, well past anything a blog article needs,
/// while a small icon or already-optimized photo is usually fine as-is.
/// Below this size, nothing is even decoded.
const SIZE_THRESHOLD_BYTES: usize = 300_000;
/// No side is ever scaled *up* - this only ever shrinks an oversized image
/// down to at most this many pixels on its longer edge.
const MAX_DIMENSION: i32 = 2000;
const JPEG_QUALITY: &str = "82";

pub struct CompressedImage {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: &'static str,
}

/// `bytes`/`filename` unchanged, for every case where compression doesn't
/// apply or didn't help - the single fallback path so every early return
/// below looks the same.
fn unchanged(bytes: &[u8], filename: &str) -> CompressedImage {
    CompressedImage {
        bytes: bytes.to_vec(),
        filename: filename.to_string(),
        mime_type: crate::export::mime_from_extension(filename),
    }
}

pub fn maybe_compress(bytes: &[u8], filename: &str) -> CompressedImage {
    if !should_attempt(bytes, filename) {
        return unchanged(bytes, filename);
    }

    let Some(pixbuf) = decode(bytes) else {
        return unchanged(bytes, filename);
    };
    let (width, height) = (pixbuf.width(), pixbuf.height());
    if width <= 0 || height <= 0 {
        return unchanged(bytes, filename);
    }

    let (target_w, target_h) = scaled_dimensions(width, height, MAX_DIMENSION);
    let scaled = if (target_w, target_h) != (width, height) {
        let Some(scaled) = pixbuf.scale_simple(target_w, target_h, gdk_pixbuf::InterpType::Bilinear) else {
            return unchanged(bytes, filename);
        };
        scaled
    } else {
        pixbuf
    };

    let (type_, options, new_ext, mime_type) = output_format(filename, scaled.has_alpha());
    let Ok(encoded) = scaled.save_to_bufferv(type_, options) else {
        return unchanged(bytes, filename);
    };
    // A resize/re-encode is supposed to be a strict improvement - if it
    // somehow isn't (a tiny image, an already near-optimal encode), sending
    // the original is always at least as good.
    if encoded.len() >= bytes.len() {
        return unchanged(bytes, filename);
    }

    CompressedImage {
        bytes: encoded,
        filename: replace_extension(filename, new_ext),
        mime_type,
    }
}

fn should_attempt(bytes: &[u8], filename: &str) -> bool {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg") && bytes.len() > SIZE_THRESHOLD_BYTES
}

/// A JPEG stays a (re-encoded, usually smaller) JPEG. A PNG with no alpha
/// channel - very often a screenshot or photo saved as PNG, much larger
/// than it needs to be - converts to JPEG; one *with* alpha stays PNG,
/// since JPEG can't represent transparency and silently flattening it
/// would visibly break the image.
fn output_format(filename: &str, has_alpha: bool) -> (&'static str, &'static [(&'static str, &'static str)], &'static str, &'static str) {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    if ext == "jpg" || ext == "jpeg" {
        ("jpeg", &[("quality", JPEG_QUALITY)], "jpg", "image/jpeg")
    } else if has_alpha {
        ("png", &[("compression", "9")], "png", "image/png")
    } else {
        ("jpeg", &[("quality", JPEG_QUALITY)], "jpg", "image/jpeg")
    }
}

fn decode(bytes: &[u8]) -> Option<gdk_pixbuf::Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

/// Scales `(width, height)` down to fit within `max_dimension` on the
/// longer edge, preserving aspect ratio - unchanged if it already fits.
/// Pure and gdk-pixbuf-free, so this - the actual sizing decision - is
/// unit-testable without a real image.
fn scaled_dimensions(width: i32, height: i32, max_dimension: i32) -> (i32, i32) {
    let longest = width.max(height);
    if longest <= max_dimension {
        return (width, height);
    }
    let scale = f64::from(max_dimension) / f64::from(longest);
    let scaled_w = ((f64::from(width) * scale).round() as i32).max(1);
    let scaled_h = ((f64::from(height) * scale).round() as i32).max(1);
    (scaled_w, scaled_h)
}

fn replace_extension(filename: &str, new_ext: &str) -> String {
    match filename.rsplit_once('.') {
        Some((stem, _old_ext)) => format!("{stem}.{new_ext}"),
        None => format!("{filename}.{new_ext}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_dimensions_leaves_an_already_small_image_unchanged() {
        assert_eq!(scaled_dimensions(800, 600, 2000), (800, 600));
    }

    #[test]
    fn scaled_dimensions_shrinks_a_wide_image_preserving_aspect_ratio() {
        assert_eq!(scaled_dimensions(4000, 2000, 2000), (2000, 1000));
    }

    #[test]
    fn scaled_dimensions_shrinks_a_tall_image_preserving_aspect_ratio() {
        assert_eq!(scaled_dimensions(2000, 4000, 2000), (1000, 2000));
    }

    #[test]
    fn should_attempt_skips_small_files() {
        assert!(!should_attempt(&[0u8; 100], "photo.png"));
    }

    #[test]
    fn should_attempt_skips_unrecognized_extensions() {
        assert!(!should_attempt(&[0u8; 1_000_000], "clip.mp4"));
    }

    #[test]
    fn should_attempt_fires_for_a_large_png_or_jpeg() {
        assert!(should_attempt(&[0u8; 1_000_000], "photo.png"));
        assert!(should_attempt(&[0u8; 1_000_000], "photo.JPEG"));
    }

    #[test]
    fn output_format_keeps_jpeg_as_jpeg() {
        let (type_, _, ext, mime) = output_format("photo.jpg", false);
        assert_eq!((type_, ext, mime), ("jpeg", "jpg", "image/jpeg"));
    }

    #[test]
    fn output_format_converts_an_opaque_png_to_jpeg() {
        let (type_, _, ext, mime) = output_format("screenshot.png", false);
        assert_eq!((type_, ext, mime), ("jpeg", "jpg", "image/jpeg"));
    }

    #[test]
    fn output_format_keeps_a_transparent_png_as_png() {
        let (type_, _, ext, mime) = output_format("sticker.png", true);
        assert_eq!((type_, ext, mime), ("png", "png", "image/png"));
    }

    #[test]
    fn replace_extension_swaps_a_known_extension() {
        assert_eq!(replace_extension("photo.png", "jpg"), "photo.jpg");
    }

    #[test]
    fn replace_extension_appends_when_there_is_no_extension() {
        assert_eq!(replace_extension("photo", "jpg"), "photo.jpg");
    }
}
