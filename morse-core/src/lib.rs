//! Morse code translator core.
//!
//! Pure, side-effect-free encode/decode logic lives here so it can be
//! unit tested without touching the terminal, audio, or timing.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::LazyLock;

mod alphabets;
pub use alphabets::Alphabet;

/// One Morse "unit" in milliseconds. A dot is 1 unit, a dash is 3 units.
pub const UNIT_MS: u64 = 100;

/// Smallest unit length callers may use, in ms.
pub const MIN_UNIT_MS: u64 = 1;
/// Largest unit length callers may use, in ms (1 minute/unit — already far
/// slower than any practical use). Bounding this keeps a zero/negative/NaN
/// WPM or a huge raw millisecond value from producing a unit length so
/// large that transmitting so much as a single dash sleeps for years, and
/// keeps [`Signal::duration_ms_timed`]'s multiplication safely clear of
/// `u64` overflow.
pub const MAX_UNIT_MS: u64 = 60_000;

/// Convert words-per-minute to a unit length in milliseconds using the
/// standard PARIS-word timing formula (`unit_ms = 1200 / wpm`), the
/// convention used by the ARRL and virtually every CW training program.
/// See <https://morsecode.world/international/timing.html>.
///
/// A non-positive, NaN, or infinite `wpm` (e.g. `0.0`, which divides out
/// to `+inf`) is treated as "as slow as we allow" rather than propagating
/// the non-finite value — a saturating float-to-int cast would otherwise
/// turn `+inf` into `u64::MAX` milliseconds, an effectively infinite hang.
/// The result is always clamped to [`MIN_UNIT_MS`, `MAX_UNIT_MS`].
pub fn wpm_to_unit_ms(wpm: f64) -> u64 {
    if !wpm.is_finite() || wpm <= 0.0 {
        return MAX_UNIT_MS;
    }
    (1200.0 / wpm)
        .round()
        .clamp(MIN_UNIT_MS as f64, MAX_UNIT_MS as f64) as u64
}

/// A single timed event in a transmission: how long to signal "on" for,
/// and the gap that follows it before the next event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Hold the light/tone on for this many ms, then a 1-unit gap.
    Dot,
    /// Hold the light/tone on for this many ms, then a 1-unit gap.
    Dash,
    /// Silent gap between letters (3 units total, 2 already consumed by
    /// the trailing 1-unit symbol gap, so 2 more here).
    LetterGap,
    /// Silent gap between words (7 units total, 4 more after a letter gap).
    WordGap,
}

/// Timing parameters for Farnsworth-style playback: characters (dots,
/// dashes, and the gaps between them) are sent at `char_unit_ms`, while
/// the pauses between letters and words stretch out to `gap_unit_ms`.
/// Setting both fields equal reproduces standard, non-Farnsworth timing.
///
/// This is the internationally recommended way to learn Morse (ARRL /
/// CW Academy): keeping characters at a brisk, natural-sounding speed
/// prevents the "counting dits and dahs" habit that creates a speed
/// plateau, while the extra recognition time between characters/words
/// keeps the *effective* speed low enough for a beginner to follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub char_unit_ms: u64,
    pub gap_unit_ms: u64,
}

impl Timing {
    /// Standard (non-Farnsworth) timing: every unit is the same length.
    pub fn uniform(unit_ms: u64) -> Self {
        Self {
            char_unit_ms: unit_ms,
            gap_unit_ms: unit_ms,
        }
    }

    /// Farnsworth timing from WPM: characters sent at `char_wpm`, with
    /// letter/word gaps stretched to match an effective `effective_wpm`
    /// (which must be `<= char_wpm`, e.g. the ARRL's default 20/5 setting).
    pub fn farnsworth_wpm(char_wpm: f64, effective_wpm: f64) -> Self {
        Self {
            char_unit_ms: wpm_to_unit_ms(char_wpm),
            gap_unit_ms: wpm_to_unit_ms(effective_wpm),
        }
    }
}

impl Signal {
    /// Duration in milliseconds for this event at a single, uniform unit
    /// length. Kept for callers that don't need Farnsworth timing.
    pub fn duration_ms(&self, unit_ms: u64) -> u64 {
        self.duration_ms_timed(Timing::uniform(unit_ms))
    }

    /// Duration in milliseconds under the given [`Timing`]: dots/dashes
    /// (and the gap between a prosign's fused letters) run at character
    /// speed, while letter and word gaps run at (possibly slower) gap speed.
    pub fn duration_ms_timed(&self, timing: Timing) -> u64 {
        match self {
            Signal::Dot => timing.char_unit_ms,
            // Saturating: callers may build a `Timing` directly (bypassing
            // `wpm_to_unit_ms`'s clamp) with an arbitrary `u64`, and this
            // must never wrap into a tiny, wrong duration or panic.
            Signal::Dash => timing.char_unit_ms.saturating_mul(3),
            Signal::LetterGap => timing.gap_unit_ms.saturating_mul(2),
            Signal::WordGap => timing.gap_unit_ms.saturating_mul(4),
        }
    }

    pub fn is_tone(&self) -> bool {
        matches!(self, Signal::Dot | Signal::Dash)
    }
}

static TABLE: LazyLock<HashMap<char, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ('A', ".-"),
        ('B', "-..."),
        ('C', "-.-."),
        ('D', "-.."),
        ('E', "."),
        ('F', "..-."),
        ('G', "--."),
        ('H', "...."),
        ('I', ".."),
        ('J', ".---"),
        ('K', "-.-"),
        ('L', ".-.."),
        ('M', "--"),
        ('N', "-."),
        ('O', "---"),
        ('P', ".--."),
        ('Q', "--.-"),
        ('R', ".-."),
        ('S', "..."),
        ('T', "-"),
        ('U', "..-"),
        ('V', "...-"),
        ('W', ".--"),
        ('X', "-..-"),
        ('Y', "-.--"),
        ('Z', "--.."),
        ('0', "-----"),
        ('1', ".----"),
        ('2', "..---"),
        ('3', "...--"),
        ('4', "....-"),
        ('5', "....."),
        ('6', "-...."),
        ('7', "--..."),
        ('8', "---.."),
        ('9', "----."),
        ('.', ".-.-.-"),
        (',', "--..--"),
        ('?', "..--.."),
        ('\'', ".----."),
        ('!', "-.-.--"),
        ('/', "-..-."),
        ('(', "-.--."),
        (')', "-.--.-"),
        ('&', ".-..."),
        (':', "---..."),
        (';', "-.-.-."),
        ('=', "-...-"),
        ('+', ".-.-."),
        ('-', "-....-"),
        ('_', "..--.-"),
        ('"', ".-..-."),
        ('$', "...-..-"),
        ('@', ".--.-."),
    ])
});

/// Per-alphabet lookup tables: (encode map, decode map).
///
/// Encode: the ASCII table (so mixed text like "QTH Москва" works), then
/// the alphabet's own letters, which win on any clash (Wabun's 、。（）).
/// Latin also accepts accented extensions. Decode: shared digits and
/// punctuation, Latin letters only for [`Alphabet::Latin`], then the
/// alphabet's letters; the first letter listed for a code wins (so the
/// Russian letter, not the Ukrainian/Bulgarian variant, is decoded).
struct Tables {
    encode: HashMap<char, &'static str>,
    decode: HashMap<&'static str, char>,
}

static TABLES: LazyLock<HashMap<Alphabet, Tables>> = LazyLock::new(|| {
    Alphabet::ALL
        .iter()
        .map(|&alphabet| {
            let mut encode: HashMap<char, &'static str> = TABLE.clone();
            if alphabet == Alphabet::Latin {
                encode.extend(alphabets::LATIN_EXTENSIONS.iter().copied());
            }
            encode.extend(alphabet.letters().iter().copied());

            let mut decode: HashMap<&'static str, char> = TABLE
                .iter()
                .filter(|(c, _)| alphabet == Alphabet::Latin || !c.is_alphabetic())
                .map(|(&c, &code)| (code, c))
                .collect();
            let mut own: HashMap<&'static str, char> = HashMap::new();
            for &(c, code) in alphabet.letters() {
                own.entry(code).or_insert(c);
            }
            decode.extend(own);
            (alphabet, Tables { encode, decode })
        })
        .collect()
});

/// Every table character `c` stands for under `alphabet`, uppercased and
/// normalised (native digits, final forms, voiced kana, Hangul syllables).
fn table_chars(c: char, alphabet: Alphabet) -> Vec<char> {
    if let Some(d) = alphabets::ascii_digit(c) {
        return vec![d];
    }
    c.to_uppercase()
        .flat_map(|u| alphabets::expand(u, alphabet))
        .collect()
}

fn codes_for_word(word: &str, alphabet: Alphabet) -> Vec<&'static str> {
    let encode = &TABLES[&alphabet].encode;
    word.chars()
        .flat_map(|c| table_chars(c, alphabet))
        .filter_map(|c| encode.get(&c).copied())
        .collect()
}

/// Procedural signs ("prosigns"): pairs of letters conventionally sent
/// fused together, with no inter-letter gap, and treated by operators as
/// a single procedural signal rather than two letters. Written here in
/// `<NAME>` form (e.g. `<SK>`), the standard on-paper notation for a
/// prosign (normally typeset with an overline). Values are the fused
/// dot/dash string with no separators, matching how the signs sound on
/// the air.
/// **Ambiguity note:** several prosigns reuse the exact fused code of an
/// existing punctuation mark (`AR`/`+`, `AS`/`&`, `BT`/`=`, `KN`/`(`) — this
/// isn't a bug, it's how real Morse works: prosigns were historically
/// assigned by fusing letter pairs that already had a code, rather than
/// inventing new ones. Out of procedural context the two meanings are
/// genuinely indistinguishable on the air; [`decode`] resolves the
/// ambiguity by preferring the punctuation reading, since that's already a
/// complete, unambiguous single-character meaning.
static PROSIGNS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("AR", ".-.-."),   // end of message
        ("AS", ".-..."),   // wait
        ("BK", "-...-.-"), // break (into a contact)
        ("BT", "-...-"),   // new paragraph / break
        ("CT", "-.-.-"),   // start of transmission / commence copying
        ("KN", "-.--."),   // invite a specific station to transmit
        ("SK", "...-.-"),  // end of contact
        ("SN", "...-."),   // understood (also written VE)
    ])
});

static REVERSE_PROSIGNS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| PROSIGNS.iter().map(|(&name, &code)| (code, name)).collect());

/// Split `<NAME>` prosign markers (any case) out of text, e.g.
/// `"CQ <AR>"` -> `[Word("CQ"), Prosign("AR")]` per whitespace-separated word.
enum Token<'a> {
    Word(&'a str),
    Prosign(&'a str),
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    for word in text.split_whitespace() {
        if let Some(name) = word.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
            && let Some((&key, _)) = PROSIGNS.get_key_value(name.to_uppercase().as_str())
        {
            tokens.push(Token::Prosign(key));
            continue;
        }
        tokens.push(Token::Word(word));
    }
    tokens
}

/// Encode plain text into Morse, e.g. "SOS" -> "... --- ...".
/// The alphabet is detected from the text ([`Alphabet::detect`]); use
/// [`encode_in`] to choose it. Unknown characters are dropped. Words stay
/// separated by " / ". A `<NAME>` token (e.g. `<SK>`, `<AR>`) matching a
/// known prosign is sent as a single fused character instead of being
/// letter-decomposed.
pub fn encode(text: &str) -> String {
    encode_in(text, Alphabet::detect(text))
}

/// [`encode`] with an explicit alphabet. Matters for Arabic vs Persian,
/// which share letters but not codes.
pub fn encode_in(text: &str, alphabet: Alphabet) -> String {
    tokenize(text)
        .into_iter()
        .map(|token| match token {
            Token::Prosign(name) => PROSIGNS[name].to_string(),
            Token::Word(word) => codes_for_word(word, alphabet).join(" "),
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Decode Morse back into Latin text. Letters are space-separated, words
/// separated by "/". e.g. "... --- ..." -> "SOS". A fused code matching a
/// known prosign (no internal spaces) decodes to its `<NAME>` form.
pub fn decode(morse: &str) -> String {
    decode_in(morse, Alphabet::Latin)
}

/// [`decode`] into a chosen alphabet: the same dots and dashes mean
/// different letters in each. Japanese recombines voiced kana (か゛ -> が),
/// Hebrew restores word-final letter forms (מ -> ם at the end of a word);
/// Korean yields jamo, since regrouping them into syllables is ambiguous.
pub fn decode_in(morse: &str, alphabet: Alphabet) -> String {
    let table = &TABLES[&alphabet].decode;
    let text = morse
        .split('/')
        .map(|word| {
            word.split_whitespace()
                .map(|code| match table.get(code) {
                    Some(&c) => c.to_string(),
                    None => match REVERSE_PROSIGNS.get(code) {
                        Some(&name) => format!("<{name}>"),
                        None => String::new(),
                    },
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    match alphabet {
        Alphabet::Japanese => alphabets::compose_kana(&text),
        Alphabet::Hebrew => alphabets::hebrew_final_forms(&text),
        _ => text,
    }
}

/// Turn text into a flat sequence of timed [`Signal`]s, ready for a
/// transmitter (visual flasher, audio beeper, etc.) to play back. A
/// `<NAME>` prosign token's symbols are pushed back-to-back with no gap
/// signal between them — exactly like the dots/dashes *within* an ordinary
/// letter — since a transmitter already inserts a fixed 1-unit pause after
/// every dot/dash regardless of Signal boundaries; only one [`Signal::LetterGap`]
/// follows the whole fused prosign, not one per constituent letter.
pub fn build_signal_plan(text: &str) -> Vec<Signal> {
    build_signal_plan_in(text, Alphabet::detect(text))
}

/// [`build_signal_plan`] with an explicit alphabet (see [`encode_in`]).
pub fn build_signal_plan_in(text: &str, alphabet: Alphabet) -> Vec<Signal> {
    let tokens = tokenize(text);
    let mut plan = Vec::new();
    let n = tokens.len();

    for (i, token) in tokens.into_iter().enumerate() {
        match token {
            Token::Prosign(name) => {
                for symbol in PROSIGNS[name].chars() {
                    push_symbol(&mut plan, symbol);
                }
                plan.push(Signal::LetterGap);
            }
            Token::Word(word) => {
                for code in codes_for_word(word, alphabet) {
                    for symbol in code.chars() {
                        push_symbol(&mut plan, symbol);
                    }
                    plan.push(Signal::LetterGap);
                }
            }
        }
        if i != n - 1 {
            plan.push(Signal::WordGap);
        }
    }
    plan
}

fn push_symbol(plan: &mut Vec<Signal>, symbol: char) {
    match symbol {
        '.' => plan.push(Signal::Dot),
        '-' => plan.push(Signal::Dash),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_sos() {
        assert_eq!(encode("SOS"), "... --- ...");
    }

    #[test]
    fn encodes_multiple_words() {
        assert_eq!(encode("HI THERE"), ".... .. / - .... . .-. .");
    }

    #[test]
    fn decodes_sos() {
        assert_eq!(decode("... --- ..."), "SOS");
    }

    #[test]
    fn decodes_multiple_words() {
        assert_eq!(decode(".... .. / - .... . .-. ."), "HI THERE");
    }

    #[test]
    fn round_trip_is_stable() {
        let original = "RUST LANG 2026";
        let round_tripped = decode(&encode(original));
        assert_eq!(round_tripped, original);
    }

    #[test]
    fn unknown_characters_are_dropped() {
        assert_eq!(encode("A~B"), ".- -...");
    }

    #[test]
    fn signal_plan_for_e_is_a_single_dot_and_letter_gap() {
        let plan = build_signal_plan("E");
        assert_eq!(plan, vec![Signal::Dot, Signal::LetterGap]);
    }

    #[test]
    fn signal_plan_inserts_word_gap_between_words() {
        let plan = build_signal_plan("E E");
        assert_eq!(
            plan,
            vec![
                Signal::Dot,
                Signal::LetterGap,
                Signal::WordGap,
                Signal::Dot,
                Signal::LetterGap
            ]
        );
    }

    #[test]
    fn durations_follow_morse_timing_ratios() {
        assert_eq!(Signal::Dot.duration_ms(100), 100);
        assert_eq!(Signal::Dash.duration_ms(100), 300);
        assert_eq!(Signal::LetterGap.duration_ms(100), 200);
        assert_eq!(Signal::WordGap.duration_ms(100), 400);
    }

    #[test]
    fn wpm_conversion_matches_paris_standard() {
        // 20 WPM is the ARRL/CW-Academy recommended Farnsworth character
        // speed: 1200 / 20 = 60ms per unit.
        assert_eq!(wpm_to_unit_ms(20.0), 60);
        assert_eq!(wpm_to_unit_ms(12.0), 100);
    }

    #[test]
    fn wpm_conversion_clamps_non_positive_and_non_finite_input() {
        // These would otherwise divide out to +/-inf or NaN and, via a
        // saturating float-to-int cast, silently become a u64::MAX-ms
        // ("effectively forever") sleep instead of a bounded one.
        for wpm in [0.0, -0.0, -5.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(wpm_to_unit_ms(wpm), MAX_UNIT_MS, "wpm = {wpm}");
        }
    }

    #[test]
    fn wpm_conversion_clamps_absurdly_high_wpm() {
        assert_eq!(wpm_to_unit_ms(1_000_000.0), MIN_UNIT_MS);
    }

    #[test]
    fn duration_does_not_overflow_on_a_huge_raw_unit_length() {
        let timing = Timing::uniform(u64::MAX);
        assert_eq!(Signal::Dash.duration_ms_timed(timing), u64::MAX);
        assert_eq!(Signal::LetterGap.duration_ms_timed(timing), u64::MAX);
        assert_eq!(Signal::WordGap.duration_ms_timed(timing), u64::MAX);
    }

    #[test]
    fn farnsworth_timing_keeps_characters_fast_and_gaps_slow() {
        // ARRL's classic 20/5 setting: fast characters, slow effective speed.
        let timing = Timing::farnsworth_wpm(20.0, 5.0);
        assert_eq!(Signal::Dot.duration_ms_timed(timing), 60);
        assert_eq!(Signal::Dash.duration_ms_timed(timing), 180);
        // Gaps use the much slower effective-speed unit (1200/5 = 240ms).
        assert_eq!(Signal::LetterGap.duration_ms_timed(timing), 480);
        assert_eq!(Signal::WordGap.duration_ms_timed(timing), 960);
    }

    #[test]
    fn uniform_timing_matches_legacy_duration_ms() {
        let timing = Timing::uniform(100);
        for signal in [
            Signal::Dot,
            Signal::Dash,
            Signal::LetterGap,
            Signal::WordGap,
        ] {
            assert_eq!(signal.duration_ms_timed(timing), signal.duration_ms(100));
        }
    }

    #[test]
    fn encodes_known_prosign() {
        assert_eq!(encode("CQ <AR>"), "-.-. --.- / .-.-.");
    }

    #[test]
    fn decodes_fused_prosign_code() {
        // SK's fused code doesn't collide with any punctuation mark, so it
        // round-trips cleanly (see the collision note below for ones that
        // don't: AR, AS, BT, KN).
        assert_eq!(decode("... --- ... / ...-.-"), "SOS <SK>");
    }

    #[test]
    fn prosign_round_trips() {
        // Only prosigns whose fused code doesn't also spell a punctuation
        // mark can round-trip; see the collision test below for the rest.
        for name in ["BK", "CT", "SK", "SN"] {
            let text = format!("<{name}>");
            assert_eq!(
                decode(&encode(&text)),
                text,
                "prosign {name} did not round-trip"
            );
        }
    }

    #[test]
    fn colliding_prosigns_decode_as_their_punctuation_meaning() {
        // AR/+, AS/&, BT/=, KN/( share an identical fused code — genuinely
        // ambiguous in real Morse. decode() resolves it toward the
        // punctuation reading (see the PROSIGNS ambiguity-note doc comment).
        for (name, punctuation) in [("AR", '+'), ("AS", '&'), ("BT", '='), ("KN", '(')] {
            let text = format!("<{name}>");
            assert_eq!(
                decode(&encode(&text)),
                punctuation.to_string(),
                "prosign {name} should decode as its colliding punctuation mark"
            );
        }
    }

    #[test]
    fn unrecognized_bracket_token_falls_back_to_letters() {
        // <ZZ> isn't a known prosign, so it's treated as a plain word and
        // decomposed letter-by-letter like any other unrecognized token
        // would strip its brackets away (which aren't in the table).
        assert_eq!(encode("<ZZ>"), "--.. --..");
    }

    #[test]
    fn non_latin_alphabets_round_trip() {
        for (text, alphabet) in [
            ("СОС ПРИВЕТ МИР", Alphabet::Cyrillic),
            ("ΚΑΛΗΜΕΡΑ ΚΟΣΜΕ", Alphabet::Greek),
            ("שלום עולם", Alphabet::Hebrew),
            ("سلام عليكم", Alphabet::Arabic),
            ("سلام پدر", Alphabet::Persian),
            ("いろはにほへと", Alphabet::Japanese),
            ("ㅎㅏㄴㄱㅡㄹ", Alphabet::Korean),
        ] {
            assert_eq!(
                decode_in(&encode_in(text, alphabet), alphabet),
                text,
                "{alphabet:?}"
            );
            assert_eq!(Alphabet::detect(text), alphabet);
            assert_eq!(
                encode(text),
                encode_in(text, alphabet),
                "auto-detect for {alphabet:?}"
            );
        }
    }

    #[test]
    fn same_codes_decode_differently_per_alphabet() {
        let morse = ".- / -...";
        assert_eq!(decode_in(morse, Alphabet::Latin), "A B");
        assert_eq!(decode_in(morse, Alphabet::Cyrillic), "А Б");
        assert_eq!(decode_in(morse, Alphabet::Greek), "Α Β");
        assert_eq!(decode_in(morse, Alphabet::Japanese), "い は");
    }

    #[test]
    fn arabic_and_persian_share_letters_but_not_codes() {
        // Kha (U+062E) is --- in Arabic Morse and -..- in Persian Morse.
        assert_eq!(encode_in("خ", Alphabet::Arabic), "---");
        assert_eq!(encode_in("خ", Alphabet::Persian), "-..-");
    }

    #[test]
    fn lowercase_and_final_forms_encode() {
        assert_eq!(encode("привет"), encode("ПРИВЕТ"));
        // Final sigma uppercases to Σ; Hebrew final mem is sent as mem.
        assert_eq!(encode("ς"), encode("Σ"));
        assert_eq!(encode("ם"), encode("מ"));
        assert_eq!(encode("ёж"), encode("ЕЖ"));
        assert_eq!(encode("ά"), encode("Α"));
    }

    #[test]
    fn japanese_voiced_kana_and_katakana() {
        // が is sent as か + dakuten; katakana encodes like hiragana.
        assert_eq!(encode("が"), ".-.. ..");
        assert_eq!(encode("カタカナ"), encode("かたかな"));
        assert_eq!(
            decode_in(&encode("ばんごう"), Alphabet::Japanese),
            "ばんごう"
        );
        assert_eq!(encode("、。"), ".-.-.- .-.-..");
    }

    #[test]
    fn korean_syllables_encode_as_jamo() {
        assert_eq!(encode("한글"), encode("ㅎㅏㄴㄱㅡㄹ"));
        assert_eq!(decode_in(&encode("한글"), Alphabet::Korean), "ㅎㅏㄴㄱㅡㄹ");
    }

    #[test]
    fn native_digits_use_shared_digit_codes() {
        assert_eq!(encode("٣"), encode("3"));
        assert_eq!(decode_in(&encode("٣"), Alphabet::Arabic), "3");
    }

    #[test]
    fn latin_extensions_encode_but_decode_stays_ascii() {
        assert_eq!(encode("Ñ"), "--.--");
        assert_eq!(encode("straße"), encode("STRASSE"));
        // Ŝ shares its code with the SN prosign; decode keeps the prosign.
        assert_eq!(decode("...-."), "<SN>");
    }

    #[test]
    fn mixed_script_text_keeps_latin_letters() {
        assert_eq!(
            encode_in("QTH МОСКВА", Alphabet::Cyrillic),
            format!("{} / {}", encode("QTH"), encode("МОСКВА"))
        );
    }

    #[test]
    fn prosigns_work_in_any_case_and_alphabet() {
        assert_eq!(encode("<sk>"), "...-.-");
        assert_eq!(
            encode_in("МИР <SK>", Alphabet::Cyrillic)
                .rsplit(" / ")
                .next(),
            Some("...-.-")
        );
    }

    #[test]
    fn signal_plan_matches_encoding_for_non_latin_text() {
        let dots = |plan: Vec<Signal>| plan.iter().filter(|s| s.is_tone()).count();
        let symbols = encode("한글")
            .chars()
            .filter(|c| *c == '.' || *c == '-')
            .count();
        assert_eq!(dots(build_signal_plan("한글")), symbols);
    }

    #[test]
    fn signal_plan_fuses_prosign_without_letter_gap() {
        // <AR> = A(.-) + R(.-.) fused: .-.-. — symbols run back-to-back
        // with no gap signal between them (same as within any ordinary
        // multi-symbol letter), and only one LetterGap at the very end.
        let plan = build_signal_plan("<AR>");
        assert_eq!(
            plan,
            vec![
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
                Signal::LetterGap,
            ]
        );
    }
}
