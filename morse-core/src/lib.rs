//! Morse code translator core.
//!
//! Pure, side-effect-free encode/decode logic lives here so it can be
//! unit tested without touching the terminal, audio, or timing.

use std::collections::HashMap;
use std::sync::LazyLock;

/// One Morse "unit" in milliseconds. A dot is 1 unit, a dash is 3 units.
pub const UNIT_MS: u64 = 100;

/// Convert words-per-minute to a unit length in milliseconds using the
/// standard PARIS-word timing formula (`unit_ms = 1200 / wpm`), the
/// convention used by the ARRL and virtually every CW training program.
/// See <https://morsecode.world/international/timing.html>.
pub fn wpm_to_unit_ms(wpm: f64) -> u64 {
    (1200.0 / wpm).round().max(1.0) as u64
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
            Signal::Dash => timing.char_unit_ms * 3,
            Signal::LetterGap => timing.gap_unit_ms * 2,
            Signal::WordGap => timing.gap_unit_ms * 4,
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

static REVERSE_TABLE: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| TABLE.iter().map(|(&c, &code)| (code, c)).collect());

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

/// Split `<NAME>` prosign markers out of already-uppercased text, e.g.
/// `"CQ <AR>"` -> `[Word("CQ"), Prosign("AR")]` per whitespace-separated word.
enum Token<'a> {
    Word(&'a str),
    Prosign(&'a str),
}

fn tokenize(upper: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    for word in upper.split_whitespace() {
        if let Some(name) = word.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
            && PROSIGNS.contains_key(name)
        {
            tokens.push(Token::Prosign(name));
            continue;
        }
        tokens.push(Token::Word(word));
    }
    tokens
}

/// Encode plain text into Morse, e.g. "SOS" -> "... --- ...".
/// Unknown characters are dropped. Words stay separated by " / ".
/// A `<NAME>` token (e.g. `<SK>`, `<AR>`) matching a known prosign is sent
/// as a single fused character instead of being letter-decomposed.
pub fn encode(text: &str) -> String {
    let upper = text.to_uppercase();
    tokenize(&upper)
        .into_iter()
        .map(|token| match token {
            Token::Prosign(name) => PROSIGNS[name].to_string(),
            Token::Word(word) => word
                .chars()
                .filter_map(|c| TABLE.get(&c).copied())
                .collect::<Vec<_>>()
                .join(" "),
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Decode Morse back into text. Letters are space-separated, words
/// separated by "/". e.g. "... --- ..." -> "SOS". A fused code matching a
/// known prosign (no internal spaces) decodes to its `<NAME>` form.
pub fn decode(morse: &str) -> String {
    morse
        .split('/')
        .map(|word| {
            word.split_whitespace()
                .map(|code| match REVERSE_TABLE.get(code) {
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
        .to_string()
}

/// Turn text into a flat sequence of timed [`Signal`]s, ready for a
/// transmitter (visual flasher, audio beeper, etc.) to play back. A
/// `<NAME>` prosign token's symbols are pushed back-to-back with no gap
/// signal between them — exactly like the dots/dashes *within* an ordinary
/// letter — since a transmitter already inserts a fixed 1-unit pause after
/// every dot/dash regardless of Signal boundaries; only one [`Signal::LetterGap`]
/// follows the whole fused prosign, not one per constituent letter.
pub fn build_signal_plan(text: &str) -> Vec<Signal> {
    let upper = text.to_uppercase();
    let tokens = tokenize(&upper);
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
                for ch in word.chars() {
                    if let Some(code) = TABLE.get(&ch) {
                        for symbol in code.chars() {
                            push_symbol(&mut plan, symbol);
                        }
                        plan.push(Signal::LetterGap);
                    }
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
