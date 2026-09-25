//! Non-Latin Morse alphabets and the accented-Latin extensions.
//!
//! Every table here was parsed mechanically (not transcribed from memory)
//! from Wikipedia's "Morse code for non-Latin alphabets", "Wabun code" and
//! "Morse code" articles on 2026-09-25, which in turn cite ITU-R M.1677-1
//! and, for Korean, the Republic of Korea's 무선국의 운용에 대한 규정
//! (2025-08-12, 별표1). No table maps two letters to the same code.
//!
//! Encoding is script-aware because some scripts share codepoints but not
//! codes: Arabic and Persian both use U+062E (خ), sent `---` in Arabic Morse
//! but `-..-` in Persian Morse. Decoding is always ambiguous without an
//! alphabet, since `.-` is A, А, Α, א, ا, い or ㅗ depending on the table.

/// A Morse alphabet: which table letters are encoded with and decoded to.
///
/// Digits and punctuation from the international table are shared by every
/// alphabet (Japanese overrides a few punctuation codes with its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alphabet {
    /// International (ITU) Latin, plus accented-letter extensions on encode.
    Latin,
    /// Russian national standard, plus Ukrainian І/Є/Ї and Bulgarian Ъ.
    Cyrillic,
    Greek,
    Hebrew,
    Arabic,
    Persian,
    /// Wabun code (kana). Hiragana and katakana are both accepted.
    Japanese,
    /// SKATS-era Hangul jamo code. Syllables are split into jamo on encode;
    /// decode yields jamo (composing syllables back is ambiguous).
    Korean,
}

impl Alphabet {
    pub const ALL: [Alphabet; 8] = [
        Alphabet::Latin,
        Alphabet::Cyrillic,
        Alphabet::Greek,
        Alphabet::Hebrew,
        Alphabet::Arabic,
        Alphabet::Persian,
        Alphabet::Japanese,
        Alphabet::Korean,
    ];

    /// Stable identifier for CLI flags and config (`latin`, `cyrillic`, ...).
    pub fn id(self) -> &'static str {
        match self {
            Alphabet::Latin => "latin",
            Alphabet::Cyrillic => "cyrillic",
            Alphabet::Greek => "greek",
            Alphabet::Hebrew => "hebrew",
            Alphabet::Arabic => "arabic",
            Alphabet::Persian => "persian",
            Alphabet::Japanese => "japanese",
            Alphabet::Korean => "korean",
        }
    }

    /// The alphabet's name in its own script, for pickers.
    pub fn native_name(self) -> &'static str {
        match self {
            Alphabet::Latin => "Latin",
            Alphabet::Cyrillic => "Кириллица",
            Alphabet::Greek => "Ελληνικά",
            Alphabet::Hebrew => "עברית",
            Alphabet::Arabic => "العربية",
            Alphabet::Persian => "فارسی",
            Alphabet::Japanese => "和文 (Wabun)",
            Alphabet::Korean => "한글",
        }
    }

    /// Parse an [`Alphabet::id`] or a common alias / ISO 639-1 code.
    pub fn from_name(name: &str) -> Option<Alphabet> {
        let n = name.trim().to_lowercase();
        Some(match n.as_str() {
            "latin" | "international" | "itu" | "en" => Alphabet::Latin,
            "cyrillic" | "russian" | "ru" | "uk" | "bg" => Alphabet::Cyrillic,
            "greek" | "el" => Alphabet::Greek,
            "hebrew" | "he" | "iw" => Alphabet::Hebrew,
            "arabic" | "ar" => Alphabet::Arabic,
            "persian" | "farsi" | "fa" => Alphabet::Persian,
            "japanese" | "wabun" | "ja" | "kana" => Alphabet::Japanese,
            "korean" | "hangul" | "ko" => Alphabet::Korean,
            _ => return None,
        })
    }

    /// Guess the alphabet from the first letter of a recognised script.
    /// Arabic-script text is treated as Persian only if it contains a
    /// Persian-only letter (پ چ ژ گ ک ی). Text with no such letter is Latin.
    pub fn detect(text: &str) -> Alphabet {
        let persian_only = ['پ', 'چ', 'ژ', 'گ', 'ک', 'ی'];
        for c in text.chars() {
            let a = match c as u32 {
                0x0370..=0x03FF | 0x1F00..=0x1FFF => Alphabet::Greek,
                0x0400..=0x04FF => Alphabet::Cyrillic,
                0x0590..=0x05FF => Alphabet::Hebrew,
                0x0600..=0x06FF => {
                    if text.chars().any(|c| persian_only.contains(&c)) {
                        Alphabet::Persian
                    } else {
                        Alphabet::Arabic
                    }
                }
                0x3040..=0x30FF | 0x3000..=0x303F | 0xFF08 | 0xFF09 => Alphabet::Japanese,
                0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 => Alphabet::Korean,
                _ => continue,
            };
            return a;
        }
        Alphabet::Latin
    }

    /// Letter table for this alphabet (encode + decode).
    pub(crate) fn letters(self) -> &'static [(char, &'static str)] {
        match self {
            Alphabet::Latin => &[],
            Alphabet::Cyrillic => CYRILLIC,
            Alphabet::Greek => GREEK,
            Alphabet::Hebrew => HEBREW,
            Alphabet::Arabic => ARABIC,
            Alphabet::Persian => PERSIAN,
            Alphabet::Japanese => WABUN,
            Alphabet::Korean => KOREAN,
        }
    }

    /// Extra characters accepted on encode only, each mapped to the letter
    /// whose code it is sent with (final forms, tonos, Ё, small kana...).
    #[rustfmt::skip]
    pub(crate) fn encode_aliases(self) -> &'static [(char, char)] {
        match self {
            Alphabet::Cyrillic => &[('Ё', 'Е')],
            Alphabet::Greek => &[
                ('Ά', 'Α'), ('Έ', 'Ε'), ('Ή', 'Η'), ('Ί', 'Ι'), ('Ό', 'Ο'),
                ('Ύ', 'Υ'), ('Ώ', 'Ω'), ('Ϊ', 'Ι'), ('Ϋ', 'Υ'),
            ],
            Alphabet::Hebrew => &[('ך', 'כ'), ('ם', 'מ'), ('ן', 'נ'), ('ף', 'פ'), ('ץ', 'צ')],
            Alphabet::Arabic => &[('أ', 'ا'), ('إ', 'ا'), ('آ', 'ا'), ('ٱ', 'ا'), ('ى', 'ي'), ('ؤ', 'و'), ('ئ', 'ي')],
            Alphabet::Persian => &[('أ', 'ا'), ('إ', 'ا'), ('آ', 'ا'), ('ي', 'ی'), ('ك', 'ک')],
            Alphabet::Japanese => &[
                ('ぁ', 'あ'), ('ぃ', 'い'), ('ぅ', 'う'), ('ぇ', 'え'), ('ぉ', 'お'),
                ('っ', 'つ'), ('ゃ', 'や'), ('ゅ', 'ゆ'), ('ょ', 'よ'), ('ゎ', 'わ'),
                ('(', '（'), (')', '）'), ('（', '（'), ('）', '）'),
            ],
            Alphabet::Latin | Alphabet::Korean => &[],
        }
    }
}

/// Accented Latin letters (encode only: most share a code, e.g. Ä/Æ/Ą).
#[rustfmt::skip]
pub(crate) const LATIN_EXTENSIONS: &[(char, &str)] = &[
    ('À', ".--.-"), ('Å', ".--.-"), ('Ä', ".-.-"), ('Æ', ".-.-"), ('Ą', ".-.-"),
    ('Ć', "-.-.."), ('Ĉ', "-.-.."), ('Ç', "-.-.."), ('Đ', "..-.."), ('É', "..-.."),
    ('Ę', "..-.."), ('Ð', "..--."), ('È', ".-..-"), ('Ł', ".-..-"), ('Ĝ', "--.-."),
    ('Ĥ', "----"), ('Š', "----"), ('Ĵ', ".---."), ('Ń', "--.--"), ('Ñ', "--.--"),
    ('Ó', "---."), ('Ö', "---."), ('Ø', "---."), ('Ś', "...-..."), ('Ŝ', "...-."),
    ('Þ', ".--.."), ('Ü', "..--"), ('Ŭ', "..--"), ('Ź', "--..-."), ('Ż', "--..-"),
];

#[rustfmt::skip]
const CYRILLIC: &[(char, &str)] = &[
    ('А', ".-"), ('Б', "-..."), ('В', ".--"), ('Г', "--."), ('Д', "-.."), ('Е', "."),
    ('Ж', "...-"), ('З', "--.."), ('И', ".."), ('Й', ".---"), ('К', "-.-"), ('Л', ".-.."),
    ('М', "--"), ('Н', "-."), ('О', "---"), ('П', ".--."), ('Р', ".-."), ('С', "..."),
    ('Т', "-"), ('У', "..-"), ('Ф', "..-."), ('Х', "...."), ('Ц', "-.-."), ('Ч', "---."),
    ('Ш', "----"), ('Щ', "--.-"), ('Ь', "-..-"), ('Ы', "-.--"), ('Э', "..-.."), ('Ю', "..--"),
    ('Я', ".-.-"), ('Ї', ".---."),
    // Encode-only variants that reuse a Russian code (decode prefers the
    // Russian letter, listed first above): Ukrainian І/Є, Bulgarian Ъ.
    ('І', ".."), ('Є', "..-.."), ('Ъ', "-..-"),
];

#[rustfmt::skip]
const GREEK: &[(char, &str)] = &[
    ('Α', ".-"), ('Β', "-..."), ('Γ', "--."), ('Δ', "-.."), ('Ε', "."), ('Ζ', "--.."),
    ('Η', "...."), ('Θ', "-.-."), ('Ι', ".."), ('Κ', "-.-"), ('Λ', ".-.."), ('Μ', "--"),
    ('Ν', "-."), ('Ξ', "-..-"), ('Ο', "---"), ('Π', ".--."), ('Ρ', ".-."), ('Σ', "..."),
    ('Τ', "-"), ('Υ', "-.--"), ('Φ', "..-."), ('Χ', "----"), ('Ψ', "--.-"), ('Ω', ".--"),
];

#[rustfmt::skip]
const HEBREW: &[(char, &str)] = &[
    ('א', ".-"), ('ב', "-..."), ('ג', "--."), ('ד', "-.."), ('ה', "---"), ('ו', "."),
    ('ז', "--.."), ('ח', "...."), ('ט', "..-"), ('י', ".."), ('כ', "-.-"), ('ל', ".-.."),
    ('מ', "--"), ('נ', "-."), ('ס', "-.-."), ('ע', ".---"), ('פ', ".--."), ('צ', ".--"),
    ('ק', "--.-"), ('ר', ".-."), ('ש', "..."), ('ת', "-"),
];

#[rustfmt::skip]
const ARABIC: &[(char, &str)] = &[
    ('ا', ".-"), ('ب', "-..."), ('ت', "-"), ('ث', "-.-."), ('ج', ".---"), ('ح', "...."),
    ('خ', "---"), ('د', "-.."), ('ذ', "--.."), ('ر', ".-."), ('ز', "---."), ('س', "..."),
    ('ش', "----"), ('ص', "-..-"), ('ض', "...-"), ('ط', "..-"), ('ظ', "-.--"), ('ع', ".-.-"),
    ('غ', "--."), ('ف', "..-."), ('ق', "--.-"), ('ك', "-.-"), ('ل', ".-.."), ('م', "--"),
    ('ن', "-."), ('ه', "..-.."), ('و', ".--"), ('ي', ".."), ('ء', "."),
];

#[rustfmt::skip]
const PERSIAN: &[(char, &str)] = &[
    ('ا', ".-"), ('ب', "-..."), ('پ', ".--."), ('ت', "-"), ('ث', "-.-."), ('ج', ".---"),
    ('چ', "---."), ('ح', "...."), ('خ', "-..-"), ('د', "-.."), ('ذ', "...-"), ('ر', ".-."),
    ('ز', "--.."), ('ژ', "--."), ('س', "..."), ('ش', "----"), ('ص', ".-.-"), ('ض', "..-.."),
    ('ط', "..-"), ('ظ', "-.--"), ('ع', "---"), ('غ', "..--"), ('ف', "..-."), ('ق', "...---"),
    ('ک', "-.-"), ('گ', "--.-"), ('ل', ".-.."), ('م', "--"), ('ن', "-."), ('و', ".--"),
    ('ه', "."), ('ی', ".."),
];

/// Wabun, in iroha order, plus its marks and punctuation.
#[rustfmt::skip]
const WABUN: &[(char, &str)] = &[
    ('い', ".-"), ('ろ', ".-.-"), ('は', "-..."), ('に', "-.-."), ('ほ', "-.."), ('へ', "."),
    ('と', "..-.."), ('ち', "..-."), ('り', "--."), ('ぬ', "...."), ('る', "-.--."), ('を', ".---"),
    ('わ', "-.-"), ('か', ".-.."), ('よ', "--"), ('た', "-."), ('れ', "---"), ('そ', "---."),
    ('つ', ".--."), ('ね', "--.-"), ('な', ".-."), ('ら', "..."), ('む', "-"), ('う', "..-"),
    ('ゐ', ".-..-"), ('の', "..--"), ('お', ".-..."), ('く', "...-"), ('や', ".--"), ('ま', "-..-"),
    ('け', "-.--"), ('ふ', "--.."), ('こ', "----"), ('え', "-.---"), ('て', ".-.--"), ('あ', "--.--"),
    ('さ', "-.-.-"), ('き', "-.-.."), ('ゆ', "-..--"), ('め', "-...-"), ('み', "..-.-"), ('し', "--.-."),
    ('ゑ', ".--.."), ('ひ', "--..-"), ('も', "-..-."), ('せ', ".---."), ('す', "---.-"), ('ん', ".-.-."),
    (DAKUTEN, ".."), (HANDAKUTEN, "..--."), ('ー', ".--.-"),
    ('、', ".-.-.-"), ('。', ".-.-.."), ('（', "-.--.-"), ('）', ".-..-."),
];

pub(crate) const DAKUTEN: char = '゛';
pub(crate) const HANDAKUTEN: char = '゜';

/// Voiced kana → (base kana, mark). Wabun sends these as two characters.
#[rustfmt::skip]
const KANA_VOICED: &[(char, char, char)] = &[
    ('が', 'か', DAKUTEN), ('ぎ', 'き', DAKUTEN), ('ぐ', 'く', DAKUTEN), ('げ', 'け', DAKUTEN), ('ご', 'こ', DAKUTEN),
    ('ざ', 'さ', DAKUTEN), ('じ', 'し', DAKUTEN), ('ず', 'す', DAKUTEN), ('ぜ', 'せ', DAKUTEN), ('ぞ', 'そ', DAKUTEN),
    ('だ', 'た', DAKUTEN), ('ぢ', 'ち', DAKUTEN), ('づ', 'つ', DAKUTEN), ('で', 'て', DAKUTEN), ('ど', 'と', DAKUTEN),
    ('ば', 'は', DAKUTEN), ('び', 'ひ', DAKUTEN), ('ぶ', 'ふ', DAKUTEN), ('べ', 'へ', DAKUTEN), ('ぼ', 'ほ', DAKUTEN),
    ('ぱ', 'は', HANDAKUTEN), ('ぴ', 'ひ', HANDAKUTEN), ('ぷ', 'ふ', HANDAKUTEN), ('ぺ', 'へ', HANDAKUTEN), ('ぽ', 'ほ', HANDAKUTEN),
    ('ゔ', 'う', DAKUTEN),
];

#[rustfmt::skip]
const KOREAN: &[(char, &str)] = &[
    ('ㄱ', ".-.."), ('ㄴ', "..-."), ('ㄷ', "-..."), ('ㄹ', "...-"), ('ㅁ', "--"), ('ㅂ', ".--"),
    ('ㅅ', "--."), ('ㅇ', "-.-"), ('ㅈ', ".--."), ('ㅊ', "-.-."), ('ㅋ', "-..-"), ('ㅌ', "--.."),
    ('ㅍ', "---"), ('ㅎ', ".---"),
    ('ㅏ', "."), ('ㅑ', ".."), ('ㅓ', "-"), ('ㅕ', "..."), ('ㅗ', ".-"), ('ㅛ', "-."),
    ('ㅜ', "...."), ('ㅠ', ".-."), ('ㅡ', "-.."), ('ㅣ', "..-"), ('ㅐ', "--.-"), ('ㅔ', "-.--"),
];

/// Katakana → hiragana (same kana, the table is keyed on hiragana).
fn to_hiragana(c: char) -> char {
    match c as u32 {
        0x30A1..=0x30F6 => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

/// Normalise one input character into the table characters it is sent as.
/// Returns an empty vec for characters with no code in `alphabet`.
pub(crate) fn expand(c: char, alphabet: Alphabet) -> Vec<char> {
    match alphabet {
        Alphabet::Japanese => {
            let h = to_hiragana(c);
            if let Some(&(_, base, mark)) = KANA_VOICED.iter().find(|(v, _, _)| *v == h) {
                return vec![base, mark];
            }
            vec![alias(h, alphabet)]
        }
        Alphabet::Korean => decompose_hangul(c).unwrap_or_else(|| vec![c]),
        _ => vec![alias(c, alphabet)],
    }
}

fn alias(c: char, alphabet: Alphabet) -> char {
    alphabet
        .encode_aliases()
        .iter()
        .find(|(from, _)| *from == c)
        .map_or(c, |&(_, to)| to)
}

/// Recombine a base kana followed by ゛/゜ into the voiced kana, for decode.
pub(crate) fn compose_kana(text: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        if (c == DAKUTEN || c == HANDAKUTEN)
            && let Some(&prev) = out.last()
            && let Some(&(voiced, _, _)) = KANA_VOICED
                .iter()
                .find(|(_, base, mark)| *base == prev && *mark == c)
        {
            *out.last_mut().expect("checked non-empty") = voiced;
            continue;
        }
        out.push(c);
    }
    out.into_iter().collect()
}

/// Hebrew writes כ מ נ פ צ in a distinct final form at the end of a word.
/// Morse sends both forms with one code, so restore them on decode.
pub(crate) fn hebrew_final_forms(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars: Vec<char> = word.chars().collect();
            if let Some(last) = chars.last_mut()
                && let Some(&(final_form, _)) = Alphabet::Hebrew
                    .encode_aliases()
                    .iter()
                    .find(|(_, base)| base == last)
            {
                *last = final_form;
            }
            chars.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// Hangul syllable decomposition (Unicode §3.12), mapping each conjoining
// jamo index to the compatibility jamo sequence Korean Morse sends. Double
// and compound jamo are sent as their component letters.
#[rustfmt::skip]
const INITIALS: [&str; 19] = [
    "ㄱ", "ㄱㄱ", "ㄴ", "ㄷ", "ㄷㄷ", "ㄹ", "ㅁ", "ㅂ", "ㅂㅂ", "ㅅ", "ㅅㅅ", "ㅇ", "ㅈ", "ㅈㅈ",
    "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ",
];
#[rustfmt::skip]
const MEDIALS: [&str; 21] = [
    "ㅏ", "ㅐ", "ㅑ", "ㅑㅣ", "ㅓ", "ㅔ", "ㅕ", "ㅕㅣ", "ㅗ", "ㅗㅏ", "ㅗㅐ", "ㅗㅣ", "ㅛ", "ㅜ",
    "ㅜㅓ", "ㅜㅔ", "ㅜㅣ", "ㅠ", "ㅡ", "ㅡㅣ", "ㅣ",
];
#[rustfmt::skip]
const FINALS: [&str; 28] = [
    "", "ㄱ", "ㄱㄱ", "ㄱㅅ", "ㄴ", "ㄴㅈ", "ㄴㅎ", "ㄷ", "ㄹ", "ㄹㄱ", "ㄹㅁ", "ㄹㅂ", "ㄹㅅ", "ㄹㅌ",
    "ㄹㅍ", "ㄹㅎ", "ㅁ", "ㅂ", "ㅂㅅ", "ㅅ", "ㅅㅅ", "ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ",
];
/// Compatibility jamo that are themselves double/compound letters.
#[rustfmt::skip]
const COMPOUND_JAMO: &[(char, &str)] = &[
    ('ㄲ', "ㄱㄱ"), ('ㄸ', "ㄷㄷ"), ('ㅃ', "ㅂㅂ"), ('ㅆ', "ㅅㅅ"), ('ㅉ', "ㅈㅈ"),
    ('ㄳ', "ㄱㅅ"), ('ㄵ', "ㄴㅈ"), ('ㄶ', "ㄴㅎ"), ('ㄺ', "ㄹㄱ"), ('ㄻ', "ㄹㅁ"), ('ㄼ', "ㄹㅂ"),
    ('ㄽ', "ㄹㅅ"), ('ㄾ', "ㄹㅌ"), ('ㄿ', "ㄹㅍ"), ('ㅀ', "ㄹㅎ"), ('ㅄ', "ㅂㅅ"),
    ('ㅒ', "ㅑㅣ"), ('ㅖ', "ㅕㅣ"), ('ㅘ', "ㅗㅏ"), ('ㅙ', "ㅗㅐ"), ('ㅚ', "ㅗㅣ"), ('ㅝ', "ㅜㅓ"),
    ('ㅞ', "ㅜㅔ"), ('ㅟ', "ㅜㅣ"), ('ㅢ', "ㅡㅣ"),
];

fn decompose_hangul(c: char) -> Option<Vec<char>> {
    let cp = c as u32;
    if (0xAC00..=0xD7A3).contains(&cp) {
        let s = cp - 0xAC00;
        let (l, v, t) = (
            (s / 588) as usize,
            ((s % 588) / 28) as usize,
            (s % 28) as usize,
        );
        return Some(
            INITIALS[l]
                .chars()
                .chain(MEDIALS[v].chars())
                .chain(FINALS[t].chars())
                .collect(),
        );
    }
    COMPOUND_JAMO
        .iter()
        .find(|(j, _)| *j == c)
        .map(|(_, parts)| parts.chars().collect())
}

/// Digits written in Arabic-Indic / Extended Arabic-Indic (Persian) and
/// full-width forms, mapped to ASCII so they use the shared digit codes.
pub(crate) fn ascii_digit(c: char) -> Option<char> {
    let cp = c as u32;
    let d = match cp {
        0x0660..=0x0669 => cp - 0x0660,
        0x06F0..=0x06F9 => cp - 0x06F0,
        0xFF10..=0xFF19 => cp - 0xFF10,
        _ => return None,
    };
    char::from_digit(d, 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn decodable_letters_have_unique_codes_per_alphabet() {
        for a in Alphabet::ALL {
            let mut seen = HashSet::new();
            for (c, code) in a.letters() {
                // Cyrillic's trailing encode-only variants intentionally
                // reuse Russian codes; everything else must be unique.
                if a == Alphabet::Cyrillic && ['І', 'Є', 'Ъ'].contains(c) {
                    continue;
                }
                assert!(seen.insert(*code), "{a:?}: duplicate code {code} at {c}");
            }
        }
    }

    #[test]
    fn codes_are_only_dots_and_dashes() {
        let all = Alphabet::ALL
            .iter()
            .flat_map(|a| a.letters())
            .chain(LATIN_EXTENSIONS);
        for (c, code) in all {
            assert!(
                !code.is_empty() && code.chars().all(|s| s == '.' || s == '-'),
                "{c}: {code}"
            );
        }
    }

    #[test]
    fn detect_picks_the_script() {
        assert_eq!(Alphabet::detect("hello"), Alphabet::Latin);
        assert_eq!(Alphabet::detect("привет"), Alphabet::Cyrillic);
        assert_eq!(Alphabet::detect("γειά"), Alphabet::Greek);
        assert_eq!(Alphabet::detect("שלום"), Alphabet::Hebrew);
        assert_eq!(Alphabet::detect("سلام"), Alphabet::Arabic);
        assert_eq!(Alphabet::detect("پدر"), Alphabet::Persian);
        assert_eq!(Alphabet::detect("カタカナ"), Alphabet::Japanese);
        assert_eq!(Alphabet::detect("한글"), Alphabet::Korean);
        assert_eq!(Alphabet::detect("123 ..."), Alphabet::Latin);
    }

    #[test]
    fn hangul_decomposes_into_sent_jamo() {
        // 한 = ㅎ + ㅏ + ㄴ; 괜 = ㄱ + ㅗㅐ + ㄴ; 까 = ㄱㄱ + ㅏ.
        assert_eq!(decompose_hangul('한').unwrap(), vec!['ㅎ', 'ㅏ', 'ㄴ']);
        assert_eq!(
            decompose_hangul('괜').unwrap(),
            vec!['ㄱ', 'ㅗ', 'ㅐ', 'ㄴ']
        );
        assert_eq!(decompose_hangul('까').unwrap(), vec!['ㄱ', 'ㄱ', 'ㅏ']);
    }

    #[test]
    fn voiced_kana_split_and_recompose() {
        assert_eq!(expand('ガ', Alphabet::Japanese), vec!['か', DAKUTEN]);
        assert_eq!(expand('ぱ', Alphabet::Japanese), vec!['は', HANDAKUTEN]);
        assert_eq!(compose_kana("か゛は゜"), "がぱ");
        // A stray mark after a kana with no voiced form stays as-is.
        assert_eq!(compose_kana("な゛"), "な゛");
    }

    #[test]
    fn native_digits_map_to_ascii() {
        assert_eq!(ascii_digit('٣'), Some('3'));
        assert_eq!(ascii_digit('۷'), Some('7'));
        assert_eq!(ascii_digit('９'), Some('9'));
        assert_eq!(ascii_digit('x'), None);
    }
}
