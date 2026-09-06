//! Document statistics shown in the right pane's "Statistik" tab, computed
//! straight from the Markdown source text.

use gtk4::prelude::*;

use crate::i18n::tr;

#[derive(Clone, Copy)]
pub struct Stats {
    pub words: usize,
    pub chars_with_spaces: usize,
    pub chars_without_spaces: usize,
    pub paragraphs: usize,
    pub reading_minutes: usize,
    /// German-adapted Flesch reading-ease score (Amstad's formula), 0-100,
    /// higher meaning easier to read - `None` for an empty document, where
    /// the underlying words-per-sentence/syllables-per-word ratios are
    /// undefined rather than meaningfully zero.
    pub readability_score: Option<f64>,
}

pub fn compute(markdown: &str) -> Stats {
    let words = markdown.split_whitespace().count();
    let paragraphs = markdown.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).count();
    let reading_minutes = if words == 0 { 0 } else { ((words as f64 / 200.0).ceil() as usize).max(1) };
    Stats {
        words,
        chars_with_spaces: markdown.chars().count(),
        chars_without_spaces: markdown.chars().filter(|c| !c.is_whitespace()).count(),
        paragraphs,
        reading_minutes,
        readability_score: readability_score(markdown, words),
    }
}

/// Amstad's German adaptation of the Flesch reading-ease formula: `180 -
/// ASL - (58.5 * ASW)`, where ASL is average sentence length (words per
/// sentence) and ASW is average syllables per word - the same 0-100 scale
/// as the original English Flesch score (higher = easier), just
/// recalibrated for German's longer average word length. Sentence and
/// syllable counting are both simple heuristics (counting `.`/`!`/`?` and
/// vowel-group runs respectively) rather than true linguistic analysis -
/// good enough for a rough "is this getting hard to read" signal, not a
/// precise measurement.
fn readability_score(markdown: &str, words: usize) -> Option<f64> {
    if words == 0 {
        return None;
    }
    let sentences = markdown.chars().filter(|c| matches!(c, '.' | '!' | '?')).count().max(1);
    let syllables: usize = markdown.split_whitespace().map(count_syllables).sum();
    let avg_sentence_length = words as f64 / sentences as f64;
    let avg_syllables_per_word = syllables as f64 / words as f64;
    Some((180.0 - avg_sentence_length - 58.5 * avg_syllables_per_word).clamp(0.0, 100.0))
}

/// Counts vowel-group runs in `word` as a proxy for syllables (e.g.
/// "Lesbarkeit" -> Le-s-ba-r-kei-t counts 3 vowel groups: e, a, ei) -
/// a common heuristic for languages without irregular silent vowels, and
/// close enough for German. Always at least 1, so an all-consonant token
/// (an abbreviation, a stray punctuation-only "word") doesn't zero out
/// the average.
fn count_syllables(word: &str) -> usize {
    let mut count = 0;
    let mut prev_is_vowel = false;
    for ch in word.to_lowercase().chars() {
        let is_vowel = matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'ä' | 'ö' | 'ü');
        if is_vowel && !prev_is_vowel {
            count += 1;
        }
        prev_is_vowel = is_vowel;
    }
    count.max(1)
}

/// The short qualitative label shown next to the numeric score, using the
/// same bucket boundaries as the standard Flesch-Reading-Ease scale.
fn readability_label(score: f64) -> String {
    match score {
        s if s >= 90.0 => tr("Sehr leicht"),
        s if s >= 70.0 => tr("Leicht"),
        s if s >= 50.0 => tr("Mittel"),
        s if s >= 30.0 => tr("Schwer"),
        _ => tr("Sehr schwer"),
    }
}

fn readability_display(score: Option<f64>) -> String {
    match score {
        Some(score) => tr("{label} ({score})").replace("{label}", &readability_label(score)).replace("{score}", &score.round().to_string()),
        None => "–".to_string(),
    }
}

pub struct StatsView {
    pub widget: gtk4::Widget,
    words: gtk4::Label,
    chars_with_spaces: gtk4::Label,
    chars_without_spaces: gtk4::Label,
    paragraphs: gtk4::Label,
    reading_minutes: gtk4::Label,
    readability: gtk4::Label,
}

impl StatsView {
    pub fn new() -> Self {
        let words = value_label();
        let chars_with_spaces = value_label();
        let chars_without_spaces = value_label();
        let paragraphs = value_label();
        let reading_minutes = value_label();
        let readability = value_label();

        let list = gtk4::Box::builder().orientation(gtk4::Orientation::Vertical).build();
        list.append(&row(&tr("Wörter"), &words));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row(&tr("Zeichen (mit Leerzeichen)"), &chars_with_spaces));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row(&tr("Zeichen (ohne Leerzeichen)"), &chars_without_spaces));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row(&tr("Absätze"), &paragraphs));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row(&tr("Geschätzte Lesezeit"), &reading_minutes));
        list.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        list.append(&row(&tr("Lesbarkeit"), &readability));
        list.add_css_class("boxed-list");

        let clamp = adw::Clamp::builder().maximum_size(420).child(&list).build();
        let scroller = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hexpand(true)
            .vexpand(true)
            .margin_top(18)
            .margin_start(12)
            .margin_end(12)
            .build();

        Self {
            widget: scroller.upcast(),
            words,
            chars_with_spaces,
            chars_without_spaces,
            paragraphs,
            reading_minutes,
            readability,
        }
    }

    pub fn update(&self, markdown: &str) {
        let stats = compute(markdown);
        self.words.set_label(&stats.words.to_string());
        self.chars_with_spaces.set_label(&stats.chars_with_spaces.to_string());
        self.chars_without_spaces.set_label(&stats.chars_without_spaces.to_string());
        self.paragraphs.set_label(&stats.paragraphs.to_string());
        self.reading_minutes.set_label(&tr("{n} min").replace("{n}", &stats.reading_minutes.to_string()));
        self.readability.set_label(&readability_display(stats.readability_score));
    }
}

fn value_label() -> gtk4::Label {
    let label = gtk4::Label::new(Some("0"));
    label.add_css_class("dim-label");
    label
}

fn row(title: &str, value: &gtk4::Label) -> gtk4::Box {
    let title_label = gtk4::Label::builder().label(title).xalign(0.0).hexpand(true).build();
    let row_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();
    row_box.append(&title_label);
    row_box.append(value);
    row_box
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readability_score_is_none_for_an_empty_document() {
        assert_eq!(compute("").readability_score, None);
    }

    #[test]
    fn count_syllables_counts_vowel_group_runs() {
        assert_eq!(count_syllables("Haus"), 1);
        assert_eq!(count_syllables("Lesbarkeit"), 3);
        assert_eq!(count_syllables("xyz"), 1, "an all-consonant token still counts as at least one syllable");
    }

    #[test]
    fn short_simple_sentences_score_higher_than_one_long_complex_sentence() {
        let simple = "Der Hund rennt. Die Katze schläft. Wir gehen raus.";
        let complex = "Der ungewöhnlich lebhafte, außergewöhnlich neugierige Schäferhund rannte, \
             ohne einen einzigen Moment innezuhalten oder sich umzuschauen, unaufhaltsam \
             durch den weitläufigen, dicht bewachsenen Garten seines Nachbarn.";
        let simple_score = compute(simple).readability_score.expect("simple text should have a score");
        let complex_score = compute(complex).readability_score.expect("complex text should have a score");
        assert!(simple_score > complex_score, "expected {simple_score} > {complex_score}");
    }

    #[test]
    fn readability_label_uses_the_standard_flesch_bucket_boundaries() {
        assert_eq!(readability_label(95.0), "Sehr leicht");
        assert_eq!(readability_label(75.0), "Leicht");
        assert_eq!(readability_label(55.0), "Mittel");
        assert_eq!(readability_label(35.0), "Schwer");
        assert_eq!(readability_label(10.0), "Sehr schwer");
    }

    #[test]
    fn readability_display_shows_a_dash_for_no_score() {
        assert_eq!(readability_display(None), "–");
    }

    #[test]
    fn readability_display_combines_label_and_rounded_score() {
        assert_eq!(readability_display(Some(95.4)), "Sehr leicht (95)");
    }
}
