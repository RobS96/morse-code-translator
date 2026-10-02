//! Non-Latin Morse alphabets and the accented-Latin extensions.
//!
//! Every table here was parsed mechanically (not transcribed from memory)
//! from Wikipedia's "Morse code for non-Latin alphabets", "Wabun code" and
//! "Morse code" articles on 2026-09-25, which in turn cite ITU-R M.1677-1
//! and, for Korean, the Republic of Korea's 무선국의 운용에 대한 규정
//! (2025-08-12, 별표1). The Ukrainian table names its own sources. No table
//! maps two letters to the same code.
//!
//! Encoding is script-aware because some scripts share codepoints but not
//! codes: Arabic and Persian both use U+062E (خ), sent `---` in Arabic Morse
//! but `-..-` in Persian Morse, and Russian and Ukrainian both use U+0418
//! (И), sent `..` in Russian Morse but `-.--` in Ukrainian Morse. Decoding
//! is always ambiguous without an alphabet, since `.-` is A, А, Α, א, ا, い
//! or ㅗ depending on the table.

/// A Morse alphabet: which table letters are encoded with and decoded to.
///
/// Digits and punctuation from the international table are shared by every
/// alphabet (Japanese overrides a few punctuation codes with its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alphabet {
    /// International (ITU) Latin, plus accented-letter extensions: all of
    /// them on encode, one letter per unambiguous code on decode.
    Latin,
    /// Russian national standard, plus Bulgarian Ъ and, so that they are
    /// not dropped, the Ukrainian letters І and Є (sent as И and Э) and Ї.
    /// Ъ is sent as Ь and read from `--.--`. Ukrainian text has a table of
    /// its own, [`Alphabet::Ukrainian`].
    Cyrillic,
    /// Ukrainian national table: Є, І and Ї, and И on the code Russian
    /// Morse gives Ы. Ґ is sent as Г. Russian Ы, Э, Ъ and Ё have no code.
    Ukrainian,
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
    pub const ALL: [Alphabet; 9] = [
        Alphabet::Latin,
        Alphabet::Cyrillic,
        Alphabet::Ukrainian,
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
            Alphabet::Ukrainian => "ukrainian",
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
            Alphabet::Ukrainian => "Українська",
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
            "cyrillic" | "russian" | "ru" | "bg" => Alphabet::Cyrillic,
            "ukrainian" | "uk" | "українська" => Alphabet::Ukrainian,
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
    /// Persian-only letter (پ چ ژ گ ک ی). Cyrillic-script text is treated as
    /// Ukrainian only if it contains a Ukrainian-only letter (І Ї Є Ґ) and
    /// no Russian-only one (Ы Э Ъ Ё), in either case; with neither, or with
    /// both, it is Cyrillic. Text with no letter of a recognised script is
    /// Latin. CJK punctuation (U+3000..=U+303F, ideographic space included)
    /// is common to several scripts and does not count as a letter.
    pub fn detect(text: &str) -> Alphabet {
        let persian_only = ['پ', 'چ', 'ژ', 'گ', 'ک', 'ی'];
        let ukrainian_only = ['І', 'Ї', 'Є', 'Ґ', 'і', 'ї', 'є', 'ґ'];
        let russian_only = ['Ы', 'Э', 'Ъ', 'Ё', 'ы', 'э', 'ъ', 'ё'];
        for c in text.chars() {
            let a = match c as u32 {
                0x0370..=0x03FF | 0x1F00..=0x1FFF => Alphabet::Greek,
                0x0400..=0x04FF => {
                    // Normalised, so that a decomposed Ё (Е + U+0308) counts.
                    let text = normalize_input(text);
                    let has = |letters: &[char]| text.chars().any(|c| letters.contains(&c));
                    if has(&ukrainian_only) && !has(&russian_only) {
                        Alphabet::Ukrainian
                    } else {
                        Alphabet::Cyrillic
                    }
                }
                0x0590..=0x05FF => Alphabet::Hebrew,
                0x0600..=0x06FF => {
                    if text.chars().any(|c| persian_only.contains(&c)) {
                        Alphabet::Persian
                    } else {
                        Alphabet::Arabic
                    }
                }
                0x3040..=0x30FF | 0xFF08 | 0xFF09 | 0xFF61..=0xFF9F => Alphabet::Japanese,
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
            Alphabet::Ukrainian => UKRAINIAN,
            Alphabet::Greek => GREEK,
            Alphabet::Hebrew => HEBREW,
            Alphabet::Arabic => ARABIC,
            Alphabet::Persian => PERSIAN,
            Alphabet::Japanese => WABUN,
            Alphabet::Korean => KOREAN,
        }
    }

    /// Codes read on decode only, each with the letter it decodes to: a
    /// letter that [`Alphabet::letters`] sends with another letter's code.
    ///
    /// Russian Ъ is `--.--` in the table of Wikipedia's "Russian Morse
    /// code" (read 2026-10-02), a code no other Cyrillic letter has. It is
    /// sent with the code of Ь all the same: "Morse code for non-Latin
    /// alphabets" has no Ъ in the Russian standard and gives Bulgarian Ъ
    /// the code `-..-`.
    pub(crate) fn decode_only(self) -> &'static [(&'static str, char)] {
        match self {
            Alphabet::Cyrillic => &[("--.--", 'Ъ')],
            _ => &[],
        }
    }

    /// Extra characters accepted on encode only, each mapped to the letter
    /// whose code it is sent with (final forms, tonos, Ё, Ґ, small kana...).
    /// A character is looked up as written and, uppercased, once more.
    #[rustfmt::skip]
    pub(crate) fn encode_aliases(self) -> &'static [(char, char)] {
        match self {
            Alphabet::Cyrillic => &[('Ё', 'Е')],
            Alphabet::Ukrainian => &[('Ґ', 'Г')],
            Alphabet::Greek => &[
                ('Ά', 'Α'), ('Έ', 'Ε'), ('Ή', 'Η'), ('Ί', 'Ι'), ('Ό', 'Ο'),
                ('Ύ', 'Υ'), ('Ώ', 'Ω'), ('Ϊ', 'Ι'), ('Ϋ', 'Υ'),
                // Lowercase: these two have no single uppercase letter.
                ('ΐ', 'Ι'), ('ΰ', 'Υ'),
            ],
            Alphabet::Hebrew => &[('ך', 'כ'), ('ם', 'מ'), ('ן', 'נ'), ('ף', 'פ'), ('ץ', 'צ')],
            Alphabet::Arabic => &[('أ', 'ا'), ('إ', 'ا'), ('آ', 'ا'), ('ٱ', 'ا'), ('ى', 'ي'), ('ؤ', 'و'), ('ئ', 'ي'), ('ة', 'ه')],
            Alphabet::Persian => &[('أ', 'ا'), ('إ', 'ا'), ('آ', 'ا'), ('ي', 'ی'), ('ك', 'ک'), ('ئ', 'ی'), ('ؤ', 'و'), ('ة', 'ه'), ('ۀ', 'ه')],
            Alphabet::Japanese => &[
                ('ぁ', 'あ'), ('ぃ', 'い'), ('ぅ', 'う'), ('ぇ', 'え'), ('ぉ', 'お'),
                ('っ', 'つ'), ('ゃ', 'や'), ('ゅ', 'ゆ'), ('ょ', 'よ'), ('ゎ', 'わ'),
                ('(', '（'), (')', '）'), ('（', '（'), ('）', '）'),
            ],
            Alphabet::Latin | Alphabet::Korean => &[],
        }
    }
}

/// Accented Latin letters, for encode. Most share a code (Ä/Æ/Ą), so
/// decode knows only [`LATIN_DECODE_EXTENSIONS`].
#[rustfmt::skip]
pub(crate) const LATIN_EXTENSIONS: &[(char, &str)] = &[
    ('À', ".--.-"), ('Å', ".--.-"), ('Ä', ".-.-"), ('Æ', ".-.-"), ('Ą', ".-.-"),
    ('Ć', "-.-.."), ('Ĉ', "-.-.."), ('Ç', "-.-.."), ('Đ', "..-.."), ('É', "..-.."),
    ('Ę', "..-.."), ('Ð', "..--."), ('È', ".-..-"), ('Ł', ".-..-"), ('Ĝ', "--.-."),
    ('Ĥ', "----"), ('Š', "----"), ('Ĵ', ".---."), ('Ń', "--.--"), ('Ñ', "--.--"),
    ('Ó', "---."), ('Ö', "---."), ('Ø', "---."), ('Ś', "...-..."), ('Ŝ', "...-."),
    ('Þ', ".--.."), ('Ü', "..--"), ('Ŭ', "..--"), ('Ź', "--..-."), ('Ż', "--..-"),
];

/// What an extension code decodes to in the Latin alphabet: one canonical
/// letter where several share the code, and only codes that no ASCII
/// character or prosign uses. `----` is the digraph CH, which has no
/// character of its own. `...-.` (Ŝ) is not here: it is the prosign SN.
#[rustfmt::skip]
pub(crate) const LATIN_DECODE_EXTENSIONS: &[(&str, &str)] = &[
    ("..--", "Ü"), ("---.", "Ö"), (".-.-", "Ä"), ("--.--", "Ñ"), ("----", "CH"),
    (".--.-", "Å"), ("..-..", "É"), ("-.-..", "Ç"), (".-..-", "È"), ("..--.", "Ð"),
    ("--.-.", "Ĝ"), (".---.", "Ĵ"), ("...-...", "Ś"), (".--..", "Þ"), ("--..-.", "Ź"),
    ("--..-", "Ż"),
];

/// Precomposed accented Latin letters of the Latin-1 Supplement and Latin
/// Extended-A blocks: (base, combining mark, precomposed), uppercase only,
/// taken from Unicode's canonical decompositions. Letters that have none
/// (Æ, Ð, Ø, Þ, Đ, Ħ, Ł, Ŋ, Œ, Ŧ) are not here.
#[rustfmt::skip]
const LATIN_COMPOSED: &[(char, char, char)] = &[
    ('A', '\u{0300}', 'À'), ('E', '\u{0300}', 'È'), ('I', '\u{0300}', 'Ì'), ('O', '\u{0300}', 'Ò'),
    ('U', '\u{0300}', 'Ù'),
    ('A', '\u{0301}', 'Á'), ('E', '\u{0301}', 'É'), ('I', '\u{0301}', 'Í'), ('O', '\u{0301}', 'Ó'),
    ('U', '\u{0301}', 'Ú'), ('Y', '\u{0301}', 'Ý'), ('C', '\u{0301}', 'Ć'), ('L', '\u{0301}', 'Ĺ'),
    ('N', '\u{0301}', 'Ń'), ('R', '\u{0301}', 'Ŕ'), ('S', '\u{0301}', 'Ś'), ('Z', '\u{0301}', 'Ź'),
    ('A', '\u{0302}', 'Â'), ('E', '\u{0302}', 'Ê'), ('I', '\u{0302}', 'Î'), ('O', '\u{0302}', 'Ô'),
    ('U', '\u{0302}', 'Û'), ('C', '\u{0302}', 'Ĉ'), ('G', '\u{0302}', 'Ĝ'), ('H', '\u{0302}', 'Ĥ'),
    ('J', '\u{0302}', 'Ĵ'), ('S', '\u{0302}', 'Ŝ'), ('W', '\u{0302}', 'Ŵ'), ('Y', '\u{0302}', 'Ŷ'),
    ('A', '\u{0303}', 'Ã'), ('N', '\u{0303}', 'Ñ'), ('O', '\u{0303}', 'Õ'), ('I', '\u{0303}', 'Ĩ'),
    ('U', '\u{0303}', 'Ũ'),
    ('A', '\u{0304}', 'Ā'), ('E', '\u{0304}', 'Ē'), ('I', '\u{0304}', 'Ī'), ('O', '\u{0304}', 'Ō'),
    ('U', '\u{0304}', 'Ū'),
    ('A', '\u{0306}', 'Ă'), ('E', '\u{0306}', 'Ĕ'), ('G', '\u{0306}', 'Ğ'), ('I', '\u{0306}', 'Ĭ'),
    ('O', '\u{0306}', 'Ŏ'), ('U', '\u{0306}', 'Ŭ'),
    ('C', '\u{0307}', 'Ċ'), ('E', '\u{0307}', 'Ė'), ('G', '\u{0307}', 'Ġ'), ('I', '\u{0307}', 'İ'),
    ('Z', '\u{0307}', 'Ż'),
    ('A', '\u{0308}', 'Ä'), ('E', '\u{0308}', 'Ë'), ('I', '\u{0308}', 'Ï'), ('O', '\u{0308}', 'Ö'),
    ('U', '\u{0308}', 'Ü'), ('Y', '\u{0308}', 'Ÿ'),
    ('A', '\u{030A}', 'Å'), ('U', '\u{030A}', 'Ů'),
    ('O', '\u{030B}', 'Ő'), ('U', '\u{030B}', 'Ű'),
    ('C', '\u{030C}', 'Č'), ('D', '\u{030C}', 'Ď'), ('E', '\u{030C}', 'Ě'), ('L', '\u{030C}', 'Ľ'),
    ('N', '\u{030C}', 'Ň'), ('R', '\u{030C}', 'Ř'), ('S', '\u{030C}', 'Š'), ('T', '\u{030C}', 'Ť'),
    ('Z', '\u{030C}', 'Ž'),
    ('C', '\u{0327}', 'Ç'), ('G', '\u{0327}', 'Ģ'), ('K', '\u{0327}', 'Ķ'), ('L', '\u{0327}', 'Ļ'),
    ('N', '\u{0327}', 'Ņ'), ('R', '\u{0327}', 'Ŗ'), ('S', '\u{0327}', 'Ş'), ('T', '\u{0327}', 'Ţ'),
    ('A', '\u{0328}', 'Ą'), ('E', '\u{0328}', 'Ę'), ('I', '\u{0328}', 'Į'), ('U', '\u{0328}', 'Ų'),
];

/// The base letter and combining mark of a precomposed accented Latin
/// letter (uppercase), e.g. Ê -> (E, U+0302). The encoder sends such a
/// letter as its base where the alphabet has no code for the letter
/// itself, and reports the mark, as it does for the decomposed form.
pub(crate) fn latin_base(c: char) -> Option<(char, char)> {
    LATIN_COMPOSED
        .iter()
        .find(|(_, _, composed)| *composed == c)
        .map(|&(base, mark, _)| (base, mark))
}

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
    // With Ї above, they keep Ukrainian letters from being dropped when
    // text is sent with this alphabet: because it was asked for by name, or
    // because the text has Russian-only letters as well. Ukrainian text
    // proper is sent with UKRAINIAN, where И has another code and these
    // letters decode. Ъ is read from a code of its own
    // ([`Alphabet::decode_only`]).
    ('І', ".."), ('Є', "..-.."), ('Ъ', "-..-"),
];

/// Ukrainian, in alphabet order: the regulation ("Регламент") column of the
/// alphabet table in Ukrainian Wikipedia's «Абетка Морзе», but for Ї. It
/// agrees with English Wikipedia's "Morse code for non-Latin alphabets"
/// (both as of 2026-10-02): Є where Russian Morse has Э, І where it has И,
/// И on the code of Russian Ы, and Ї in addition.
///
/// - Ї is `.---.`, as English Wikipedia and the other column of the
///   Ukrainian table give it. The regulation column sends Ї with І's code,
///   `..`, which could never decode back to Ї; `.---.` round-trips, and is
///   what [`CYRILLIC`] sends Ї as too.
/// - Ґ has Г's code, `--.`. It is encode-only (an
///   [`Alphabet::encode_aliases`] entry), and `--.` decodes to Г.
/// - The Russian letters Ы, Э, Ъ and Ё are not in the Ukrainian alphabet
///   and have no code here, not even another letter's: they are left out
///   and reported like any other character without a code.
#[rustfmt::skip]
const UKRAINIAN: &[(char, &str)] = &[
    ('А', ".-"), ('Б', "-..."), ('В', ".--"), ('Г', "--."), ('Д', "-.."), ('Е', "."),
    ('Є', "..-.."), ('Ж', "...-"), ('З', "--.."), ('И', "-.--"), ('І', ".."), ('Ї', ".---."),
    ('Й', ".---"), ('К', "-.-"), ('Л', ".-.."), ('М', "--"), ('Н', "-."), ('О', "---"),
    ('П', ".--."), ('Р', ".-."), ('С', "..."), ('Т', "-"), ('У', "..-"), ('Ф', "..-."),
    ('Х', "...."), ('Ц', "-.-."), ('Ч', "---."), ('Ш', "----"), ('Щ', "--.-"), ('Ь', "-..-"),
    ('Ю', "..--"), ('Я', ".-.-"),
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

/// The letter `c` is sent as, if it is one of the alphabet's
/// [`Alphabet::encode_aliases`]; otherwise `c` itself.
pub(crate) fn alias(c: char, alphabet: Alphabet) -> char {
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
/// Morse sends both forms with one code, so restore them on decode. The
/// word's last letter counts even when punctuation follows it ("שלום."),
/// but not when a digit or another letter does.
pub(crate) fn hebrew_final_forms(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars: Vec<char> = word.chars().collect();
            if let Some(last) = chars.iter_mut().rev().find(|c| c.is_alphanumeric())
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

/// Full-width forms of the half-width block U+FF61..=U+FF9F, in order.
const HALFWIDTH_KANA: &str = "。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテト\
                              ナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜";

/// Decomposed Cyrillic letters: (base, combining mark, precomposed).
#[rustfmt::skip]
const CYRILLIC_COMPOSED: &[(char, char, char)] = &[
    ('И', '\u{0306}', 'Й'), ('и', '\u{0306}', 'й'),
    ('Е', '\u{0308}', 'Ё'), ('е', '\u{0308}', 'ё'),
    ('І', '\u{0308}', 'Ї'), ('і', '\u{0308}', 'ї'),
];

/// The voiced form of kana `base` (hiragana or katakana) under `mark`
/// (゛ or ゜), in the same syllabary as `base`.
fn voiced_kana(base: char, mark: char) -> Option<char> {
    let hiragana = to_hiragana(base);
    let &(voiced, _, _) = KANA_VOICED
        .iter()
        .find(|(_, b, m)| *b == hiragana && *m == mark)?;
    if hiragana == base {
        Some(voiced)
    } else {
        char::from_u32(voiced as u32 + 0x60)
    }
}

/// Rewrite the decomposed and compatibility forms that keyboards, file
/// names and copy-paste commonly produce into the characters the Morse
/// tables are keyed on. The encoder applies this to all input; it is a
/// hand-written subset of Unicode normalisation, not NFC/NFKC.
///
/// Covered:
/// - Cyrillic И/и + U+0306 -> Й/й, Е/е + U+0308 -> Ё/ё and
///   І/і + U+0308 -> Ї/ї.
/// - Kana followed by a combining (semi-)voiced mark (U+3099, U+309A) ->
///   the precomposed kana (か + U+3099 -> が). Where no precomposed kana
///   exists the mark becomes the spacing mark (゛ or ゜), which Wabun sends
///   as a character of its own.
/// - Half-width katakana and punctuation (U+FF61..=U+FF9F) -> full-width,
///   with half-width voiced marks combined the same way (ｶﾞ -> ガ).
/// - Modern Hangul conjoining jamo (initials U+1100..=U+1112, medials
///   U+1161..=U+1175, finals U+11A8..=U+11C2) -> compatibility jamo, with
///   double and compound jamo written as their component letters, exactly
///   as precomposed syllables are sent.
/// - Invisible format characters are removed, being neither letters nor
///   word breaks: the zero-width non-joiner (U+200C), which Persian writes
///   inside words, and the rest of U+200B..=U+200F (zero-width space and
///   joiner, directional marks), the byte order mark (U+FEFF), the soft
///   hyphen (U+00AD), directional embeddings and overrides
///   (U+202A..=U+202E), and the word joiner, invisible operators and
///   directional isolates (U+2060..=U+2069).
/// - Typographic punctuation -> the ASCII character that has the code:
///   `‘` `’` `ʼ` -> `'`, `“` `”` `„` -> `"`, `‐` `‑` `–` `—` `−` -> `-`,
///   `…` -> `...`, and Arabic `؟` `،` `؛` -> `?` `,` `;`.
/// - Full-width forms (U+FF01..=U+FF5E) -> ASCII, where the encoder reads
///   the ASCII character: letters, digits, punctuation with a code, `%`
///   and the `<` `>` of a prosign. Full-width brackets stay as they are,
///   being characters of the Wabun table.
///
/// Not covered (such characters pass through unchanged, and the encoder
/// drops and reports whatever has no code):
/// - Any other combining mark: decomposed accented Latin (N + U+0303),
///   Greek tonos, Hebrew points, Arabic vowel marks.
/// - Archaic conjoining jamo and the fillers U+115F/U+1160.
/// - Ligatures and presentation forms.
pub fn normalize_input(text: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        // A (semi-)voiced mark that follows its kana: the combining marks,
        // and the half-width spacing marks (ﾞ ﾟ). Full-width spacing marks
        // are already what the table holds.
        let mark = match cp {
            0x3099 | 0xFF9E => Some(DAKUTEN),
            0x309A | 0xFF9F => Some(HANDAKUTEN),
            _ => None,
        };
        if let Some(mark) = mark {
            if let Some(last) = out.last_mut()
                && let Some(voiced) = voiced_kana(*last, mark)
            {
                *last = voiced;
            } else {
                out.push(mark);
            }
            continue;
        }
        match cp {
            0x0306 | 0x0308 => {
                if let Some(last) = out.last_mut()
                    && let Some(&(_, _, composed)) = CYRILLIC_COMPOSED
                        .iter()
                        .find(|(base, mark, _)| base == last && *mark == c)
                {
                    *last = composed;
                } else {
                    out.push(c);
                }
            }
            0xFF61..=0xFF9D => out.push(
                HALFWIDTH_KANA
                    .chars()
                    .nth((cp - 0xFF61) as usize)
                    .unwrap_or(c),
            ),
            0x1100..=0x1112 => out.extend(INITIALS[(cp - 0x1100) as usize].chars()),
            0x1161..=0x1175 => out.extend(MEDIALS[(cp - 0x1161) as usize].chars()),
            // FINALS[0] is "no final consonant", so U+11A8 is index 1.
            0x11A8..=0x11C2 => out.extend(FINALS[(cp - 0x11A8 + 1) as usize].chars()),
            _ if is_ignorable(c) => {}
            0x2018 | 0x2019 | 0x02BC => out.push('\''),
            0x201C..=0x201E => out.push('"'),
            0x2010 | 0x2011 | 0x2013 | 0x2014 | 0x2212 => out.push('-'),
            0x2026 => out.extend(['.'; 3]),
            0x061F => out.push('?'),
            0x060C => out.push(','),
            0x061B => out.push(';'),
            0xFF01..=0xFF5E => out.push(narrow(c).unwrap_or(c)),
            _ => out.push(c),
        }
    }
    out.into_iter().collect()
}

/// Invisible format characters, which are neither letters nor word breaks
/// and are ignored on encode and decode alike: the soft hyphen, zero-width
/// spaces and joiners, directional marks, embeddings, overrides and
/// isolates, the word joiner and the byte order mark.
pub(crate) fn is_ignorable(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2069 | 0xFEFF
    )
}

/// The ASCII character the full-width form `c` (U+FF01..=U+FF5E) is
/// written for, if the encoder reads that character: a letter, a digit,
/// punctuation the international table has, `%`, or the `<` `>` of a
/// prosign. `None` for anything else, which is then reported as typed,
/// and for the full-width brackets, which Wabun has codes of its own for.
fn narrow(c: char) -> Option<char> {
    if c == '（' || c == '）' {
        return None;
    }
    let ascii = char::from_u32((c as u32).checked_sub(0xFEE0)?)?;
    let read = ascii.is_ascii_alphanumeric()
        || matches!(ascii, '%' | '<' | '>')
        || crate::TABLE.contains_key(&ascii);
    read.then_some(ascii)
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
    fn decode_only_codes_are_no_letters_code() {
        let mut checked = 0;
        for a in Alphabet::ALL {
            for &(code, letter) in a.decode_only() {
                assert!(
                    code.chars().all(|s| s == '.' || s == '-'),
                    "{letter}: {code}"
                );
                // No letter of the alphabet, shared character or prosign
                // already decodes from the code...
                assert!(a.letters().iter().all(|(_, c)| *c != code), "{code}");
                assert!(crate::TABLE.values().all(|c| *c != code), "{code}");
                assert!(crate::PROSIGNS.values().all(|c| *c != code), "{code}");
                // ...and the letter is one the alphabet sends another way.
                assert!(a.letters().iter().any(|(l, _)| *l == letter), "{letter}");
                checked += 1;
            }
        }
        assert_eq!(Alphabet::Cyrillic.decode_only(), &[("--.--", 'Ъ')]);
        assert_eq!(checked, 1);
        // `--.--` is free in the Ukrainian table too, which has no Ъ.
        assert!(UKRAINIAN.iter().all(|(_, code)| *code != "--.--"));
    }

    #[test]
    fn latin_composed_letters_are_uppercase_with_an_ascii_base() {
        let mut seen = HashSet::new();
        for &(base, mark, composed) in LATIN_COMPOSED {
            assert!(base.is_ascii_uppercase(), "{composed}");
            assert!(('\u{0300}'..='\u{036F}').contains(&mark), "{composed}");
            assert!(('\u{00C0}'..='\u{017F}').contains(&composed), "{composed}");
            assert!(composed.is_uppercase(), "{composed}");
            assert!(seen.insert(composed), "{composed} is listed twice");
            assert_eq!(latin_base(composed), Some((base, mark)));
        }
        // Every accented letter with an extension code is either here or
        // has no decomposition into a base letter and a mark.
        let undecomposed = ['Æ', 'Đ', 'Ð', 'Ł', 'Ø', 'Þ'];
        for (letter, _) in LATIN_EXTENSIONS {
            assert_eq!(
                latin_base(*letter).is_none(),
                undecomposed.contains(letter),
                "{letter}"
            );
        }
        for c in ['E', 'e', 'ê', 'Œ', 'Ħ', 'Я', '\u{0302}'] {
            assert_eq!(latin_base(c), None, "{c}");
        }
        assert_eq!(latin_base('Ê'), Some(('E', '\u{0302}')));
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
    fn cyrillic_text_is_ukrainian_with_a_ukrainian_letter_and_no_russian_one() {
        for text in [
            "привіт",
            "ПРИВІТ",
            "Київ",
            "їжак",
            "Є",
            "ґанок",
            "Ґ",
            "QTH Київ",
            "і\u{0308}жак",
        ] {
            assert_eq!(Alphabet::detect(text), Alphabet::Ukrainian, "{text}");
        }
        // Neither group of letters: Ukrainian words spelt only with letters
        // Russian has too are not told apart.
        for text in ["привет", "МОСКВА", "добрий день", "София"] {
            assert_eq!(Alphabet::detect(text), Alphabet::Cyrillic, "{text}");
        }
        // A Russian-only letter, with or without a Ukrainian-only one.
        for text in ["это", "ЁЖ", "съезд", "мы", "България"] {
            assert_eq!(Alphabet::detect(text), Alphabet::Cyrillic, "{text}");
        }
        for text in ["Київ это", "ЫІ", "ґ ъ", "їжак ёж", "і е\u{0308}ж"] {
            assert_eq!(Alphabet::detect(text), Alphabet::Cyrillic, "{text}");
        }
        // The first letter of a recognised script still picks the script.
        assert_eq!(Alphabet::detect("γειά і"), Alphabet::Greek);
        assert_eq!(Alphabet::detect("і"), Alphabet::Ukrainian);
    }

    #[test]
    fn names_and_aliases_parse() {
        for a in Alphabet::ALL {
            assert_eq!(Alphabet::from_name(a.id()), Some(a));
        }
        for name in ["uk", "ukrainian", "українська", "Українська", " UK "] {
            assert_eq!(
                Alphabet::from_name(name),
                Some(Alphabet::Ukrainian),
                "{name}"
            );
        }
        for name in ["ru", "russian", "cyrillic", "bg"] {
            assert_eq!(
                Alphabet::from_name(name),
                Some(Alphabet::Cyrillic),
                "{name}"
            );
        }
        assert_eq!(Alphabet::from_name("klingon"), None);
    }

    #[test]
    fn ids_and_native_names_are_distinct() {
        let ids: HashSet<&str> = Alphabet::ALL.iter().map(|a| a.id()).collect();
        let names: HashSet<&str> = Alphabet::ALL.iter().map(|a| a.native_name()).collect();
        assert_eq!(ids.len(), Alphabet::ALL.len());
        assert_eq!(names.len(), Alphabet::ALL.len());
        assert_eq!(Alphabet::Ukrainian.id(), "ukrainian");
        assert_eq!(Alphabet::Ukrainian.native_name(), "Українська");
    }

    #[test]
    fn ukrainian_alphabet_encodes_and_decodes_letter_for_letter() {
        let letters = "АБВГДЕЄЖЗИІЇЙКЛМНОПРСТУФХЦЧШЩЬЮЯ";
        let codes = ".- -... .-- --. -.. . ..-.. ...- --.. -.-- .. .---. .--- -.- .-.. -- -. --- \
                     .--. .-. ... - ..- ..-. .... -.-. ---. ---- --.- -..- ..-- .-.-";
        assert_eq!(crate::encode_in(letters, Alphabet::Ukrainian), codes);
        assert_eq!(crate::decode_in(codes, Alphabet::Ukrainian), letters);
        assert_eq!(
            crate::encode_in(&letters.to_lowercase(), Alphabet::Ukrainian),
            codes
        );
    }

    #[test]
    fn ukrainian_ghe_with_upturn_is_sent_as_ghe() {
        let report = crate::encode_lossy_report_in("ґанок Ґ", Alphabet::Ukrainian);
        assert_eq!(
            report.morse,
            crate::encode_in("ГАНОК Г", Alphabet::Ukrainian)
        );
        assert_eq!(report.skipped, vec![]);
        assert_eq!(
            crate::decode_in(&report.morse, Alphabet::Ukrainian),
            "ГАНОК Г"
        );
    }

    #[test]
    fn russian_only_letters_have_no_code_in_ukrainian() {
        let report = crate::encode_lossy_report_in("ЫЭЪЁ ыэъё Я", Alphabet::Ukrainian);
        assert_eq!(report.morse, ".-.-");
        assert_eq!(report.skipped, vec!['Ы', 'Э', 'Ъ', 'Ё', 'ы', 'э', 'ъ', 'ё']);
        // Decomposed Ё is composed first, and is no more Ukrainian for it.
        let report = crate::encode_lossy_report_in("Е\u{0308}", Alphabet::Ukrainian);
        assert_eq!(report.morse, "");
        assert_eq!(report.skipped, vec!['Ё']);
    }

    #[test]
    fn cyrillic_still_sends_ukrainian_letters_with_russian_codes() {
        let report = crate::encode_lossy_report_in("ІЄЇ", Alphabet::Cyrillic);
        assert_eq!(report.morse, ".. ..-.. .---.");
        assert_eq!(report.skipped, vec![]);
        assert_eq!(crate::decode_in(&report.morse, Alphabet::Cyrillic), "ИЭЇ");
    }

    #[test]
    fn cjk_punctuation_alone_does_not_select_japanese() {
        // U+3000..=U+303F is shared by Chinese, Japanese and Korean text,
        // and an ideographic space turns up in otherwise Latin text.
        assert_eq!(Alphabet::detect("HELLO\u{3000}(WORLD)"), Alphabet::Latin);
        assert_eq!(Alphabet::detect("、。"), Alphabet::Latin);
        assert_eq!(Alphabet::detect("привет\u{3000}мир"), Alphabet::Cyrillic);
        // Kana still decide, wherever the punctuation sits.
        assert_eq!(Alphabet::detect("こんにちは。"), Alphabet::Japanese);
        assert_eq!(Alphabet::detect("「こんにちは」"), Alphabet::Japanese);
        assert_eq!(Alphabet::detect("\u{3000}한글"), Alphabet::Korean);
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
    fn hebrew_final_form_applies_to_the_last_letter_of_a_word() {
        assert_eq!(hebrew_final_forms("שלומ"), "שלום");
        assert_eq!(hebrew_final_forms("שלומ."), "שלום.");
        assert_eq!(hebrew_final_forms("שלומ?!"), "שלום?!");
        assert_eq!(hebrew_final_forms("(מלכ) שלומ, עולמ"), "(מלך) שלום, עולם");
        // Only the last letter changes, and only if it has a final form.
        assert_eq!(hebrew_final_forms("ממ."), "מם.");
        assert_eq!(hebrew_final_forms("מה."), "מה.");
        // A letter followed by a digit is not word-final.
        assert_eq!(hebrew_final_forms("מ3"), "מ3");
        assert_eq!(hebrew_final_forms("..."), "...");
        assert_eq!(hebrew_final_forms(""), "");
    }

    #[test]
    fn normalize_composes_decomposed_cyrillic() {
        assert_eq!(normalize_input("И\u{0306}и\u{0306}"), "Йй");
        assert_eq!(normalize_input("Е\u{0308}е\u{0308}"), "Ёё");
        assert_eq!(normalize_input("І\u{0308}і\u{0308}"), "Її");
        // The marks only compose with those letters.
        assert_eq!(normalize_input("А\u{0306}"), "А\u{0306}");
        assert_eq!(normalize_input("И\u{0308}"), "И\u{0308}");
        assert_eq!(normalize_input("\u{0306}"), "\u{0306}");
    }

    #[test]
    fn normalize_composes_kana_with_combining_marks() {
        assert_eq!(normalize_input("か\u{3099}"), "が");
        assert_eq!(normalize_input("は\u{309A}"), "ぱ");
        assert_eq!(normalize_input("カ\u{3099}ホ\u{309A}"), "ガポ");
        assert_eq!(normalize_input("ウ\u{3099}"), "ヴ");
        // No precomposed form: the mark becomes a spacing mark, which
        // Wabun sends as its own character.
        assert_eq!(normalize_input("な\u{3099}"), "な゛");
        assert_eq!(normalize_input("か\u{309A}"), "か゜");
        assert_eq!(normalize_input("\u{3099}"), "゛");
        // Spacing marks typed as such are left alone.
        assert_eq!(normalize_input("か゛"), "か゛");
    }

    #[test]
    fn normalize_widens_half_width_katakana() {
        assert_eq!(HALFWIDTH_KANA.chars().count(), 0xFF9F - 0xFF61 + 1);
        assert_eq!(normalize_input("ｱｲｳｴｵ"), "アイウエオ");
        assert_eq!(normalize_input("ｦﾝｯｰ"), "ヲンッー");
        assert_eq!(normalize_input("ｶﾞｷﾞﾊﾟ"), "ガギパ");
        assert_eq!(normalize_input("ﾅﾞ"), "ナ゛");
        assert_eq!(normalize_input("｡､"), "。、");
        assert_eq!(normalize_input("\u{FF61}"), "\u{3002}");
        assert_eq!(normalize_input("\u{FF9D}"), "\u{30F3}");
        assert_eq!(Alphabet::detect("ｶﾀｶﾅ"), Alphabet::Japanese);
    }

    #[test]
    fn normalize_maps_conjoining_jamo_to_compatibility_jamo() {
        // 한글 in conjoining jamo (what NFD produces).
        assert_eq!(
            normalize_input("\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}"),
            "ㅎㅏㄴㄱㅡㄹ"
        );
        // First and last of each modern range.
        assert_eq!(normalize_input("\u{1100}\u{1112}"), "ㄱㅎ");
        assert_eq!(normalize_input("\u{1161}\u{1175}"), "ㅏㅣ");
        assert_eq!(normalize_input("\u{11A8}\u{11C2}"), "ㄱㅎ");
        // Double and compound jamo become their component letters.
        assert_eq!(normalize_input("\u{1101}\u{116A}\u{11B9}"), "ㄱㄱㅗㅏㅂㅅ");
        // Archaic jamo and fillers are not covered.
        assert_eq!(normalize_input("\u{1113}\u{1160}"), "\u{1113}\u{1160}");
        assert_eq!(normalize_input("\u{11A7}\u{11C3}"), "\u{11A7}\u{11C3}");
    }

    #[test]
    fn normalize_drops_zero_width_non_joiners() {
        assert_eq!(normalize_input("می\u{200C}خواهم"), "میخواهم");
        assert_eq!(normalize_input("\u{200C}A\u{200C}\u{200C}B\u{200C}"), "AB");
    }

    #[test]
    fn normalize_drops_invisible_format_characters() {
        // Byte order mark, soft hyphen, zero-width space and joiner,
        // directional marks, embeddings and isolates, word joiner.
        assert_eq!(normalize_input("\u{FEFF}SOS"), "SOS");
        assert_eq!(normalize_input("CO\u{00AD}OP"), "COOP");
        assert_eq!(normalize_input("A\u{200B}\u{200D}B"), "AB");
        assert_eq!(normalize_input("\u{200F}שלום\u{200E}"), "שלום");
        assert_eq!(normalize_input("\u{202B}שלום\u{202C}"), "שלום");
        assert_eq!(normalize_input("\u{2067}سلام\u{2069}"), "سلام");
        assert_eq!(normalize_input("A\u{2060}B"), "AB");
        // Both ends of each range, and nothing either side of them.
        for c in "\u{200B}\u{200F}\u{202A}\u{202E}\u{2060}\u{2069}".chars() {
            let text = format!("A{c}B");
            assert_eq!(normalize_input(&text), "AB", "U+{:04X}", c as u32);
        }
        for c in "\u{00AC}\u{00AE}\u{200A}\u{2029}\u{202F}\u{205F}\u{206A}\u{FEFE}".chars() {
            let text = format!("A{c}B");
            assert_eq!(normalize_input(&text), text, "U+{:04X}", c as u32);
        }
        // A joiner between a kana and its combining mark does not keep
        // them apart.
        assert_eq!(normalize_input("か\u{200D}\u{3099}"), "が");
        assert_eq!(normalize_input("И\u{200B}\u{0306}"), "Й");
    }

    #[test]
    fn normalize_maps_typographic_punctuation_to_ascii() {
        assert_eq!(normalize_input("DON\u{2019}T"), "DON'T");
        assert_eq!(normalize_input("\u{2018}A\u{2019} \u{02BC}"), "'A' '");
        assert_eq!(
            normalize_input("\u{201C}A\u{201D} \u{201E}B\u{201C}"),
            "\"A\" \"B\""
        );
        assert_eq!(
            normalize_input("\u{2010}\u{2011}\u{2013}\u{2014}\u{2212}"),
            "-----"
        );
        assert_eq!(normalize_input("A\u{2026}B"), "A...B");
        assert_eq!(normalize_input("\u{061F}\u{060C}\u{061B}"), "?,;");
        // Each of them becomes a character the international table has.
        for c in "'\"-.?,;".chars() {
            assert!(crate::TABLE.contains_key(&c), "{c}");
        }
    }

    #[test]
    fn normalize_narrows_full_width_forms() {
        assert_eq!(
            normalize_input("ＳＯＳ　ｓｏｓ　０１９"),
            "SOS\u{3000}sos\u{3000}019"
        );
        assert_eq!(normalize_input("\u{FF21}\u{FF3A}\u{FF41}\u{FF5A}"), "AZaz");
        // Punctuation with a code, and what the encoder reads without
        // one: % and the brackets of a prosign.
        assert_eq!(
            normalize_input("．，？＇！／＆：；＝＋－＿＂＄＠"),
            ".,?'!/&:;=+-_\"$@"
        );
        assert_eq!(normalize_input("＜ＳＫ＞　５０％"), "<SK>\u{3000}50%");
        // The rest would be left out either way, and is reported as typed.
        assert_eq!(
            normalize_input("＃＊［＼］＾｀｛｜｝～"),
            "＃＊［＼］＾｀｛｜｝～"
        );
        // Full-width brackets are characters of the Wabun table.
        assert_eq!(normalize_input("（）"), "（）");
        assert_eq!(normalize_input("\u{FF00}\u{FF5F}"), "\u{FF00}\u{FF5F}");
    }

    #[test]
    fn every_conjoining_jamo_matches_its_precomposed_syllable() {
        // Composing L+V(+T) arithmetically must give the same letters as
        // mapping the conjoining jamo one by one.
        for l in 0..19u32 {
            for v in 0..21u32 {
                for t in 0..28u32 {
                    let syllable = char::from_u32(0xAC00 + (l * 21 + v) * 28 + t).unwrap();
                    let mut jamo = String::new();
                    jamo.push(char::from_u32(0x1100 + l).unwrap());
                    jamo.push(char::from_u32(0x1161 + v).unwrap());
                    if t != 0 {
                        jamo.push(char::from_u32(0x11A7 + t).unwrap());
                    }
                    let expected: String =
                        decompose_hangul(syllable).unwrap().into_iter().collect();
                    assert_eq!(normalize_input(&jamo), expected, "{syllable}");
                }
            }
        }
    }

    #[test]
    fn normalize_leaves_other_text_alone() {
        for text in [
            "",
            "SOS",
            "привет мир",
            "שלום",
            "こんにちは",
            "한글",
            "N\u{0303}",
        ] {
            assert_eq!(normalize_input(text), text);
        }
    }

    #[test]
    fn letter_codes_shared_with_punctuation_are_the_documented_ones() {
        // decode_in prefers the alphabet's own letter when its code is also
        // a shared punctuation mark's or a prosign's. Each such collision is
        // accepted and listed in the README; a table change that adds or
        // removes one must update both. No letter may share a digit's code.
        #[rustfmt::skip]
        let expected: &[(Alphabet, &[(char, &str)])] = &[(
            Alphabet::Japanese,
            &[
                ('る', "("), ('る', "<KN>"), ('お', "&"), ('お', "<AS>"), ('さ', "<CT>"),
                ('め', "="), ('め', "<BT>"), ('も', "/"), ('ん', "+"), ('ん', "<AR>"),
                ('、', "."), ('（', ")"), ('）', "\""),
            ],
        )];
        let mut shared: Vec<(char, &str)> =
            crate::TABLE.iter().map(|(&c, &code)| (c, code)).collect();
        shared.sort();
        let mut prosigns: Vec<(&str, &str)> = crate::PROSIGNS
            .iter()
            .map(|(&name, &code)| (name, code))
            .collect();
        prosigns.sort();

        for a in Alphabet::ALL {
            let mut found: Vec<(char, String)> = Vec::new();
            let mut seen = HashSet::new();
            for (letter, code) in a.letters() {
                if !seen.insert(*code) {
                    continue; // encode-only variant; never decoded
                }
                for (c, _) in shared.iter().filter(|(_, shared_code)| shared_code == code) {
                    assert!(
                        !c.is_ascii_digit(),
                        "{a:?}: {letter} shares {code} with digit {c}"
                    );
                    if !c.is_alphabetic() {
                        found.push((*letter, c.to_string()));
                    }
                }
                for (name, _) in prosigns.iter().filter(|(_, p)| p == code) {
                    found.push((*letter, format!("<{name}>")));
                }
            }
            let documented: Vec<(char, String)> = expected
                .iter()
                .filter(|(alphabet, _)| *alphabet == a)
                .flat_map(|(_, pairs)| pairs.iter())
                .map(|(letter, other)| (*letter, other.to_string()))
                .collect();
            assert_eq!(found, documented, "{a:?}");
        }
    }

    #[test]
    fn colliding_codes_decode_as_the_letter() {
        assert_eq!(crate::decode_in(".-.-.", Alphabet::Japanese), "ん");
        assert_eq!(crate::decode_in(".-.-.-", Alphabet::Japanese), "、");
        assert_eq!(crate::decode_in("-.-.-", Alphabet::Japanese), "さ");
        assert_eq!(crate::decode_in(".-.-.", Alphabet::Cyrillic), "+");
        assert_eq!(crate::decode_in("-.-.-", Alphabet::Cyrillic), "<CT>");
    }

    #[test]
    fn native_digits_map_to_ascii() {
        assert_eq!(ascii_digit('٣'), Some('3'));
        assert_eq!(ascii_digit('۷'), Some('7'));
        assert_eq!(ascii_digit('９'), Some('9'));
        assert_eq!(ascii_digit('x'), None);
    }
}
