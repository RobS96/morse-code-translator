//! Morse code translator core.
//!
//! Pure, side-effect-free encode/decode logic lives here so it can be
//! unit tested without touching the terminal, audio, or timing.

use std::collections::HashMap;

/// One Morse "unit" in milliseconds. A dot is 1 unit, a dash is 3 units.
pub const UNIT_MS: u64 = 100;

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

impl Signal {
    /// Duration in milliseconds for this event at the given unit length.
    pub fn duration_ms(&self, unit_ms: u64) -> u64 {
        match self {
            Signal::Dot => unit_ms,
            Signal::Dash => unit_ms * 3,
            Signal::LetterGap => unit_ms * 2,
            Signal::WordGap => unit_ms * 4,
        }
    }

    pub fn is_tone(&self) -> bool {
        matches!(self, Signal::Dot | Signal::Dash)
    }
}

fn table() -> HashMap<char, &'static str> {
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
}

fn reverse_table() -> HashMap<&'static str, char> {
    table().into_iter().map(|(c, code)| (code, c)).collect()
}

/// Encode plain text into Morse, e.g. "SOS" -> "... --- ...".
/// Unknown characters are dropped. Words stay separated by " / ".
pub fn encode(text: &str) -> String {
    let t = table();
    text.to_uppercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter_map(|c| t.get(&c).copied())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Decode Morse back into text. Letters are space-separated, words
/// separated by "/". e.g. "... --- ..." -> "SOS".
pub fn decode(morse: &str) -> String {
    let rt = reverse_table();
    morse
        .split('/')
        .map(|word| {
            word.split_whitespace()
                .filter_map(|code| rt.get(code).copied())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Turn text into a flat sequence of timed [`Signal`]s, ready for a
/// transmitter (visual flasher, audio beeper, etc.) to play back.
pub fn build_signal_plan(text: &str) -> Vec<Signal> {
    let t = table();
    let mut plan = Vec::new();
    let upper = text.to_uppercase();
    let words: Vec<&str> = upper.split_whitespace().collect();

    for (wi, word) in words.iter().enumerate() {
        for ch in word.chars() {
            if let Some(code) = t.get(&ch) {
                for symbol in code.chars() {
                    plan.push(match symbol {
                        '.' => Signal::Dot,
                        '-' => Signal::Dash,
                        _ => continue,
                    });
                }
                plan.push(Signal::LetterGap);
            }
        }
        if wi != words.len() - 1 {
            plan.push(Signal::WordGap);
        }
    }
    plan
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
}
