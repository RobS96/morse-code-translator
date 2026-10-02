//! Morse code translator core.
//!
//! Pure, side-effect-free encode/decode logic lives here so it can be
//! unit tested without touching the terminal, audio, or timing.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::LazyLock;

mod alphabets;
mod audio;
pub use alphabets::{Alphabet, normalize_input};
pub use audio::{
    MAX_FREQUENCY_HZ, MAX_RENDER_SAMPLES, MAX_SAMPLE_RATE, MIN_FREQUENCY_HZ, MIN_SAMPLE_RATE,
    RenderError, Tone, render_samples, render_schedule, write_wav,
};

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
    /// letter/word gaps stretched so the overall speed is `effective_wpm`
    /// (e.g. the ARRL's default 20/5 setting).
    ///
    /// Uses the ARRL formula: the standard word PARIS is 31 units of
    /// character time plus 19 units of spacing, so the spacing unit is
    /// `(60000 / effective_wpm - 31 * char_unit) / 19` milliseconds. An
    /// `effective_wpm` at or above `char_wpm` gives standard timing.
    pub fn farnsworth_wpm(char_wpm: f64, effective_wpm: f64) -> Self {
        let char_unit_ms = wpm_to_unit_ms(char_wpm);
        let stretched = effective_wpm.is_finite()
            && effective_wpm > 0.0
            && char_wpm.is_finite()
            && effective_wpm < char_wpm;
        let gap_unit_ms = if stretched {
            ((60_000.0 / effective_wpm - 31.0 * char_unit_ms as f64) / 19.0)
                .round()
                .clamp(char_unit_ms as f64, MAX_UNIT_MS as f64) as u64
        } else {
            char_unit_ms
        };
        Self {
            char_unit_ms,
            gap_unit_ms,
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
            // Every symbol is followed by one unit of silence at character
            // speed, so a letter gap adds the rest of its 3 spacing units.
            // Written as 2g + (g - c) so a saturated `3 * g` can't be
            // dragged back down by the subtraction.
            Signal::LetterGap => timing
                .gap_unit_ms
                .saturating_mul(2)
                .saturating_add(timing.gap_unit_ms.saturating_sub(timing.char_unit_ms)),
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

/// Characters every alphabet sends with another character's code, and so
/// never decodes: ITU-R M.1677-1 gives the multiplication sign the code of
/// the letter X.
const ENCODE_ONLY: &[(char, &str)] = &[('×', "-..-")];

/// Per-alphabet lookup tables: (encode map, decode map).
///
/// Encode: the ASCII table (so mixed text like "QTH Москва" works), then
/// the alphabet's own letters, which win on any clash (Wabun's 、。（）).
/// Latin also accepts accented extensions. Decode: the ASCII table, for
/// [`Alphabet::Latin`] one accented letter per extension code that nothing
/// else uses, then the alphabet's letters, which win on any clash: a
/// Latin letter is decoded only where the alphabet has no letter of its
/// own for the code. The first letter listed for a code wins (so
/// [`Alphabet::Cyrillic`] decodes the Russian letter, not the
/// Ukrainian/Bulgarian variant listed after it).
struct Tables {
    encode: HashMap<char, &'static str>,
    decode: HashMap<&'static str, String>,
}

static TABLES: LazyLock<HashMap<Alphabet, Tables>> = LazyLock::new(|| {
    Alphabet::ALL
        .iter()
        .map(|&alphabet| {
            let mut encode: HashMap<char, &'static str> = TABLE.clone();
            encode.extend(ENCODE_ONLY.iter().copied());
            if alphabet == Alphabet::Latin {
                encode.extend(alphabets::LATIN_EXTENSIONS.iter().copied());
            }
            encode.extend(alphabet.letters().iter().copied());

            let mut decode: HashMap<&'static str, String> = TABLE
                .iter()
                .map(|(&c, &code)| (code, c.to_string()))
                .collect();
            if alphabet == Alphabet::Latin {
                decode.extend(
                    alphabets::LATIN_DECODE_EXTENSIONS
                        .iter()
                        .map(|&(code, text)| (code, text.to_string())),
                );
            }
            let mut own: HashMap<&'static str, String> = HashMap::new();
            for &(c, code) in alphabet.letters() {
                own.entry(code).or_insert_with(|| c.to_string());
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
    // ITU-R M.1677-1 (part I, 3.3.1): % has no code and is sent as 0/0.
    if c == '%' {
        return vec!['0', '/', '0'];
    }
    c.to_uppercase()
        .flat_map(|u| alphabets::expand(u, alphabet))
        .collect()
}

/// The codes `word` is sent as. Every input character that has no code,
/// or only part of whose expansion has one, is appended to `skipped`.
fn codes_for_word(word: &str, alphabet: Alphabet, skipped: &mut Vec<char>) -> Vec<&'static str> {
    let encode = &TABLES[&alphabet].encode;
    let mut codes = Vec::new();
    let mut after_digit = false;
    for c in word.chars() {
        let mut table_chars = table_chars(c, alphabet);
        // ITU-R M.1677-1 (part I, 3.3.2): a number is joined to its % by a
        // hyphen, so 2% is sent as 2-0/0 and not as 20/0.
        if c == '%' && after_digit {
            table_chars.insert(0, '-');
        }
        after_digit = c != '%' && table_chars.last().is_some_and(char::is_ascii_digit);
        let before = codes.len();
        codes.extend(table_chars.iter().filter_map(|t| encode.get(t).copied()));
        if codes.len() - before != table_chars.len() || table_chars.is_empty() {
            skipped.push(c);
        }
    }
    codes
}

/// Procedural signs ("prosigns"): letters (usually a pair) conventionally
/// sent fused together, with no inter-letter gap, and treated by operators
/// as a single procedural signal rather than separate letters. Written
/// here in `<NAME>` form (e.g. `<SK>`), the standard on-paper notation for
/// a prosign (normally typeset with an overline). Values are the fused
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
        ("AR", ".-.-."),      // end of message
        ("AS", ".-..."),      // wait
        ("BK", "-...-.-"),    // break (into a contact)
        ("BT", "-...-"),      // new paragraph / break
        ("CT", "-.-.-"),      // start of transmission / commence copying (also written KA)
        ("HH", "........"),   // error: disregard what was just sent
        ("KN", "-.--."),      // invite a specific station to transmit
        ("SK", "...-.-"),     // end of contact (also written VA)
        ("SN", "...-."),      // understood (also written VE)
        ("SOS", "...---..."), // distress
    ])
});

/// Other names a prosign is written under, as `(alias, name in PROSIGNS)`:
/// different letters that fuse into the same code. Accepted on encode;
/// decode always gives the name in [`PROSIGNS`].
const PROSIGN_ALIASES: &[(&str, &str)] = &[("KA", "CT"), ("VA", "SK"), ("VE", "SN")];

/// Length of the longest prosign name or alias, which bounds how far past
/// a `<` [`split_prosigns`] looks for the closing `>`.
const PROSIGN_NAME_MAX: usize = 3;

static REVERSE_PROSIGNS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| PROSIGNS.iter().map(|(&name, &code)| (code, name)).collect());

/// The fused code of the prosign written `name` (any case, or an alias).
fn prosign_code(name: &str) -> Option<&'static str> {
    let name = name.to_uppercase();
    let name = PROSIGN_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name.as_str(), |&(_, canonical)| canonical);
    PROSIGNS.get(name).copied()
}

/// One piece of a whitespace-separated word: plain text, or the fused
/// code of a `<NAME>` prosign marker.
enum Part<'a> {
    Text(&'a str),
    Prosign(&'static str),
}

/// Split `<NAME>` prosign markers (any case) out of a word, e.g.
/// `"SOS<SK>"` -> `[Text("SOS"), Prosign("...-.-")]`. A `<...>` that names
/// no known prosign stays text.
fn split_prosigns(word: &str) -> Vec<Part<'_>> {
    let mut parts = Vec::new();
    let mut text_start = 0;
    let mut from = 0;
    while let Some(open) = word[from..].find('<').map(|i| from + i) {
        let name_start = open + 1;
        let close = word[name_start..]
            .char_indices()
            .take(PROSIGN_NAME_MAX + 1)
            .find(|&(_, c)| c == '>')
            .map(|(i, _)| name_start + i);
        if let Some(close) = close
            && let Some(code) = prosign_code(&word[name_start..close])
        {
            if text_start < open {
                parts.push(Part::Text(&word[text_start..open]));
            }
            parts.push(Part::Prosign(code));
            text_start = close + 1;
            from = close + 1;
        } else {
            from = name_start;
        }
    }
    if text_start < word.len() {
        parts.push(Part::Text(&word[text_start..]));
    }
    parts
}

/// The Morse words `text` is sent as, each a list of letter codes. A
/// prosign is one fused code within its word. Input is normalised first
/// ([`normalize_input`]); a word none of whose characters has a code is
/// left out entirely rather than sent as an empty word.
fn encode_words(text: &str, alphabet: Alphabet, skipped: &mut Vec<char>) -> Vec<Vec<&'static str>> {
    normalize_input(text)
        .split_whitespace()
        .map(|word| {
            let mut codes = Vec::new();
            for part in split_prosigns(word) {
                match part {
                    Part::Prosign(code) => codes.push(code),
                    Part::Text(text) => codes.extend(codes_for_word(text, alphabet, skipped)),
                }
            }
            codes
        })
        .filter(|codes| !codes.is_empty())
        .collect()
}

/// The result of a lossy encode: the Morse, plus what was left out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeReport {
    /// The Morse output, exactly as [`encode_in`] returns it.
    pub morse: String,
    /// Every input character that has no code in the alphabet and was
    /// dropped, in input order, repeats included. Characters are reported
    /// as they stand after [`normalize_input`]. Whitespace is a separator,
    /// not a dropped character.
    pub skipped: Vec<char>,
}

/// Encode plain text into Morse, e.g. "SOS" -> "... --- ...".
/// The alphabet is detected from the text ([`Alphabet::detect`]); use
/// [`encode_in`] to choose it. Input is normalised with
/// [`normalize_input`]. Unknown characters are dropped, and a word left
/// with no characters is dropped with them; use [`encode_lossy_report`] to
/// learn which. Words stay separated by " / ". A `<NAME>` token (e.g.
/// `<SK>`, `<AR>`) matching a known prosign is sent as a single fused
/// character instead of being letter-decomposed, whether it stands alone
/// or inside a word (`SOS<SK>`).
pub fn encode(text: &str) -> String {
    encode_in(text, Alphabet::detect(text))
}

/// [`encode`] with an explicit alphabet. Matters for Arabic vs Persian and
/// for Russian vs Ukrainian, which share letters but not codes.
pub fn encode_in(text: &str, alphabet: Alphabet) -> String {
    encode_lossy_report_in(text, alphabet).morse
}

/// [`encode`], also reporting the characters that were dropped because
/// they have no Morse code.
pub fn encode_lossy_report(text: &str) -> EncodeReport {
    encode_lossy_report_in(text, Alphabet::detect(text))
}

/// [`encode_lossy_report`] with an explicit alphabet (see [`encode_in`]).
pub fn encode_lossy_report_in(text: &str, alphabet: Alphabet) -> EncodeReport {
    let mut skipped = Vec::new();
    let morse = encode_words(text, alphabet, &mut skipped)
        .iter()
        .map(|codes| codes.join(" "))
        .collect::<Vec<_>>()
        .join(" / ");
    EncodeReport { morse, skipped }
}

/// The result of a lossy decode: the text, plus what was left out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeReport {
    /// The decoded text, exactly as [`decode_in`] returns it.
    pub text: String,
    /// Every code that is neither in the alphabet's table nor a prosign
    /// and was dropped, in input order, repeats included. Codes are
    /// reported as they stand after look-alike symbols are rewritten (see
    /// [`decode`]). Whitespace and "/" are separators, not dropped codes.
    pub skipped: Vec<String>,
}

/// Rewrite the look-alike characters that autocorrect, word processors and
/// other Morse tools put in place of the dots, dashes and word separators
/// [`decode`] reads. No Morse code contains any of them.
fn normalize_morse(morse: &str) -> String {
    let mut out = String::with_capacity(morse.len());
    for c in morse.chars() {
        match c {
            // Middle dot, bullet.
            '\u{00B7}' | '\u{2022}' => out.push('.'),
            // Minus sign, en dash, em dash, underscore.
            '\u{2212}' | '\u{2013}' | '\u{2014}' | '_' => out.push('-'),
            // Horizontal ellipsis.
            '\u{2026}' => out.push_str("..."),
            '|' => out.push('/'),
            _ => out.push(c),
        }
    }
    out
}

/// Decode Morse back into Latin text. Letters are space-separated, words
/// separated by "/". e.g. "... --- ..." -> "SOS". A fused code matching a
/// known prosign (no internal spaces) decodes to its `<NAME>` form.
///
/// Common look-alikes are read as the symbol they stand for: `·` `•` as
/// `.`, `−` `–` `—` `_` as `-`, `…` as `...` and `|` as `/`. Unknown codes
/// are dropped, and a word left with no letters is dropped with them; use
/// [`decode_lossy_report`] to learn which.
pub fn decode(morse: &str) -> String {
    decode_in(morse, Alphabet::Latin)
}

/// [`decode`] into a chosen alphabet: the same dots and dashes mean
/// different letters in each. Japanese recombines voiced kana (か゛ -> が),
/// Hebrew restores word-final letter forms (מ -> ם at the end of a word);
/// Korean yields jamo, since regrouping them into syllables is ambiguous.
/// The code of a Latin letter that the alphabet has no letter of its own
/// for decodes to the Latin letter.
pub fn decode_in(morse: &str, alphabet: Alphabet) -> String {
    decode_lossy_report_in(morse, alphabet).text
}

/// [`decode`], also reporting the codes that were dropped because they
/// are not Morse the Latin alphabet knows.
pub fn decode_lossy_report(morse: &str) -> DecodeReport {
    decode_lossy_report_in(morse, Alphabet::Latin)
}

/// [`decode_lossy_report`] with an explicit alphabet (see [`decode_in`]).
pub fn decode_lossy_report_in(morse: &str, alphabet: Alphabet) -> DecodeReport {
    let table = &TABLES[&alphabet].decode;
    let mut skipped = Vec::new();
    let text = normalize_morse(morse)
        .split('/')
        .map(|word| {
            let mut text = String::new();
            for code in word.split_whitespace() {
                if let Some(letter) = table.get(code) {
                    text.push_str(letter);
                } else if let Some(name) = REVERSE_PROSIGNS.get(code) {
                    text.push_str(&format!("<{name}>"));
                } else {
                    skipped.push(code.to_string());
                }
            }
            text
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text = match alphabet {
        Alphabet::Japanese => alphabets::compose_kana(&text),
        Alphabet::Hebrew => alphabets::hebrew_final_forms(&text),
        _ => text,
    };
    DecodeReport { text, skipped }
}

/// Turn text into a flat sequence of timed [`Signal`]s, ready for a
/// transmitter (visual flasher, audio beeper, etc.) to play back. A
/// `<NAME>` prosign token's symbols are pushed back-to-back with no gap
/// signal between them — exactly like the dots/dashes *within* an ordinary
/// letter — since a transmitter already inserts a fixed 1-unit pause after
/// every dot/dash regardless of Signal boundaries; only one [`Signal::LetterGap`]
/// separates the whole fused prosign from the next letter, not one per
/// constituent letter. [`build_schedule`] lays the plan out in time, with
/// those pauses in place.
pub fn build_signal_plan(text: &str) -> Vec<Signal> {
    build_signal_plan_in(text, Alphabet::detect(text))
}

/// [`build_signal_plan`] with an explicit alphabet (see [`encode_in`]).
/// Words with nothing to send are skipped, so the plan never holds two
/// word gaps in a row or a leading or trailing one. A letter gap follows
/// every letter but the last: the plan ends on the final dot or dash.
pub fn build_signal_plan_in(text: &str, alphabet: Alphabet) -> Vec<Signal> {
    let mut plan = Vec::new();
    for (i, codes) in encode_words(text, alphabet, &mut Vec::new())
        .into_iter()
        .enumerate()
    {
        if i != 0 {
            plan.push(Signal::LetterGap);
            plan.push(Signal::WordGap);
        }
        for (j, code) in codes.into_iter().enumerate() {
            if j != 0 {
                plan.push(Signal::LetterGap);
            }
            for symbol in code.chars() {
                push_symbol(&mut plan, symbol);
            }
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

/// What a [`ScheduleStep`] is: a tone, or one of the three silences that
/// separate tones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Key down: the tone and the lamp are on.
    Tone,
    /// Silence between two symbols of one letter (or of a fused prosign).
    SymbolGap,
    /// Silence between two letters.
    LetterGap,
    /// Silence between two words.
    WordGap,
}

/// One step of a transmission as it is keyed: a tone or a silence, and how
/// long it lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleStep {
    pub kind: StepKind,
    pub duration_ms: u64,
}

impl ScheduleStep {
    pub fn is_tone(&self) -> bool {
        self.kind == StepKind::Tone
    }
}

/// Lay a signal plan out in time: every tone and every silence of the
/// transmission, in order, each with its full duration under `timing`.
///
/// This is the one place the unit of silence after a symbol is added: a
/// [`Signal`]'s own duration leaves it out, so a transmitter that plays a
/// plan signal by signal would have to add it itself. In the schedule a
/// single silence separates each tone from the next, 1 character unit
/// inside a letter and 3 or 7 spacing units between letters or words, and
/// nothing follows a tone that ends the plan. A plan built by hand may
/// start or end with gaps; those are kept as silences of their own.
pub fn build_schedule(plan: &[Signal], timing: Timing) -> Vec<ScheduleStep> {
    let mut schedule: Vec<ScheduleStep> = Vec::with_capacity(plan.len() * 2);
    for (i, signal) in plan.iter().enumerate() {
        let duration_ms = signal.duration_ms_timed(timing);
        if signal.is_tone() {
            schedule.push(ScheduleStep {
                kind: StepKind::Tone,
                duration_ms,
            });
            if i + 1 < plan.len() {
                schedule.push(ScheduleStep {
                    kind: StepKind::SymbolGap,
                    duration_ms: timing.char_unit_ms,
                });
            }
            continue;
        }
        let kind = match signal {
            Signal::WordGap => StepKind::WordGap,
            _ => StepKind::LetterGap,
        };
        // A gap signal widens the silence already running, if there is
        // one: the unit after a symbol, or an earlier gap.
        match schedule.last_mut() {
            Some(silence) if !silence.is_tone() => {
                silence.duration_ms = silence.duration_ms.saturating_add(duration_ms);
                if silence.kind != StepKind::WordGap {
                    silence.kind = kind;
                }
            }
            _ => schedule.push(ScheduleStep { kind, duration_ms }),
        }
    }
    schedule
}

/// Total length of a schedule in milliseconds, saturating rather than
/// overflowing.
pub fn schedule_duration_ms(schedule: &[ScheduleStep]) -> u64 {
    schedule
        .iter()
        .fold(0, |total, step| total.saturating_add(step.duration_ms))
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
    fn unencodable_words_leave_no_empty_word() {
        assert_eq!(encode("A ~ B"), ".- / -...");
        assert_eq!(encode("~ A"), ".-");
        assert_eq!(encode("A ~"), ".-");
        assert_eq!(encode("A ~ ~ B"), ".- / -...");
        assert_eq!(encode("~ ~"), "");
        assert_eq!(decode(&encode("A ~ B")), "A B");
    }

    #[test]
    fn unencodable_words_leave_no_doubled_word_gap() {
        assert_eq!(build_signal_plan("E ~ E"), build_signal_plan("E E"));
        assert_eq!(build_signal_plan("~ E ~"), build_signal_plan("E"));
        assert_eq!(build_signal_plan("~ ~"), vec![]);
        // A prosign next to an unencodable word keeps a single word gap.
        assert_eq!(build_signal_plan("<AR> ~ E"), build_signal_plan("<AR> E"));
    }

    #[test]
    fn lossy_report_lists_dropped_characters_in_order() {
        let report = encode_lossy_report("A~B ~ C#");
        assert_eq!(report.morse, ".- -... / -.-.");
        assert_eq!(report.skipped, vec!['~', '~', '#']);
    }

    #[test]
    fn lossy_report_is_empty_when_nothing_is_dropped() {
        for text in [
            "SOS",
            "CQ CQ <AR>",
            "hello, world!",
            "한글",
            "が",
            "  A  B  ",
        ] {
            let report = encode_lossy_report(text);
            assert_eq!(report.skipped, vec![], "{text:?}");
            assert_eq!(report.morse, encode(text));
        }
    }

    #[test]
    fn lossy_report_covers_brackets_and_foreign_letters() {
        // An unknown <NAME> is an ordinary word whose brackets have no code.
        assert_eq!(encode_lossy_report("<ZZ>").skipped, vec!['<', '>']);
        // Cyrillic has no code in the Latin alphabet, and vice versa Latin
        // letters are accepted everywhere.
        let report = encode_lossy_report_in("Я A", Alphabet::Latin);
        assert_eq!(report.morse, ".-");
        assert_eq!(report.skipped, vec!['Я']);
        assert_eq!(
            encode_lossy_report_in("Я A", Alphabet::Cyrillic).skipped,
            vec![]
        );
    }

    #[test]
    fn lossy_report_names_uncovered_combining_marks() {
        // Decomposed accented Latin is not normalised: the base letter is
        // sent and the mark is reported.
        let report = encode_lossy_report("N\u{0303}");
        assert_eq!(report.morse, "-.");
        assert_eq!(report.skipped, vec!['\u{0303}']);
    }

    #[test]
    fn decomposed_input_encodes_like_precomposed() {
        for (decomposed, precomposed) in [
            ("и\u{0306}", "й"),
            ("Е\u{0308}Ж", "ЁЖ"),
            ("і\u{0308}", "ї"),
            ("か\u{3099}", "が"),
            ("ハ\u{309A}", "パ"),
            ("ｶﾞｲｼﾞﾝ", "ガイジン"),
            ("\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}", "한글"),
        ] {
            assert_eq!(
                encode(decomposed),
                encode(precomposed),
                "{decomposed:?} vs {precomposed:?}"
            );
            assert_eq!(encode_lossy_report(decomposed).skipped, vec![]);
            assert_eq!(
                build_signal_plan(decomposed),
                build_signal_plan(precomposed)
            );
        }
        // Й has its own code; without normalisation this was И + a drop.
        assert_eq!(encode("и\u{0306}"), ".---");
    }

    #[test]
    fn hebrew_final_form_round_trips_before_punctuation() {
        for text in ["שלום.", "שלום, עולם!", "(שלום)", "מלך?"] {
            assert_eq!(
                decode_in(&encode_in(text, Alphabet::Hebrew), Alphabet::Hebrew),
                text
            );
        }
    }

    #[test]
    fn signal_plan_for_e_is_a_single_dot() {
        let plan = build_signal_plan("E");
        assert_eq!(plan, vec![Signal::Dot]);
    }

    #[test]
    fn signal_plan_inserts_word_gap_between_words() {
        let plan = build_signal_plan("E E");
        assert_eq!(
            plan,
            vec![Signal::Dot, Signal::LetterGap, Signal::WordGap, Signal::Dot]
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
        // ARRL spacing unit: (60000/5 - 31*60) / 19 = 534ms (rounded).
        assert_eq!(timing.gap_unit_ms, 534);
        // A letter gap is 3 spacing units, 60ms of which the preceding
        // symbol's own trailing silence already supplied.
        assert_eq!(Signal::LetterGap.duration_ms_timed(timing), 3 * 534 - 60);
        assert_eq!(Signal::WordGap.duration_ms_timed(timing), 4 * 534);
    }

    #[test]
    fn farnsworth_paris_takes_one_effective_word_period() {
        // PARIS plus its word gap is 31 character units and 19 spacing
        // units; at 20/5 that must last 60s / 5 WPM = 12s.
        let timing = Timing::farnsworth_wpm(20.0, 5.0);
        let total = 31 * timing.char_unit_ms + 19 * timing.gap_unit_ms;
        assert!(total.abs_diff(12_000) <= 19, "total was {total}ms");
    }

    #[test]
    fn farnsworth_never_shortens_gaps_below_standard() {
        assert_eq!(Timing::farnsworth_wpm(20.0, 20.0), Timing::uniform(60));
        assert_eq!(Timing::farnsworth_wpm(20.0, 40.0), Timing::uniform(60));
        assert_eq!(Timing::farnsworth_wpm(20.0, f64::NAN), Timing::uniform(60));
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

    /// Shorthand for a schedule step.
    fn step(kind: StepKind, duration_ms: u64) -> ScheduleStep {
        ScheduleStep { kind, duration_ms }
    }

    #[test]
    fn schedule_separates_tones_by_exactly_one_silence() {
        let timing = Timing::uniform(60);
        // A is dot dash: one character unit of silence between them.
        assert_eq!(
            build_schedule(&build_signal_plan("A"), timing),
            vec![
                step(StepKind::Tone, 60),
                step(StepKind::SymbolGap, 60),
                step(StepKind::Tone, 180),
            ]
        );
        // Letters are 3 units apart and words 7, each as a single silence.
        assert_eq!(
            build_schedule(&build_signal_plan("EE E"), timing),
            vec![
                step(StepKind::Tone, 60),
                step(StepKind::LetterGap, 180),
                step(StepKind::Tone, 60),
                step(StepKind::WordGap, 420),
                step(StepKind::Tone, 60),
            ]
        );
        for text in ["SOS", "CQ CQ <AR>", "한글", "PARIS PARIS"] {
            let schedule = build_schedule(&build_signal_plan(text), timing);
            assert!(schedule.len() % 2 == 1, "{text:?}");
            for (i, step) in schedule.iter().enumerate() {
                assert_eq!(step.is_tone(), i % 2 == 0, "{text:?}: step {i}");
            }
        }
        assert_eq!(build_schedule(&[], timing), vec![]);
    }

    #[test]
    fn schedule_stretches_only_letter_and_word_gaps_under_farnsworth() {
        let timing = Timing::farnsworth_wpm(20.0, 5.0);
        assert_eq!(
            build_schedule(&build_signal_plan("AE E"), timing),
            vec![
                step(StepKind::Tone, 60),
                step(StepKind::SymbolGap, 60),
                step(StepKind::Tone, 180),
                step(StepKind::LetterGap, 3 * 534),
                step(StepKind::Tone, 60),
                step(StepKind::WordGap, 7 * 534),
                step(StepKind::Tone, 60),
            ]
        );
    }

    /// How long `text` takes from its first tone to the first tone of a
    /// word sent after it: the text plus one word gap.
    fn word_period_ms(text: &str, timing: Timing) -> u64 {
        let schedule = build_schedule(&build_signal_plan(&format!("{text} E")), timing);
        let (last_tone, rest) = schedule.split_last().expect("a schedule");
        assert!(last_tone.is_tone());
        schedule_duration_ms(rest)
    }

    #[test]
    fn schedule_for_paris_lasts_one_word_period() {
        // PARIS and its word gap are 50 units: 60s / 20 WPM = 3000 ms.
        assert_eq!(word_period_ms("PARIS", Timing::uniform(60)), 3000);
        assert_eq!(
            word_period_ms("PARIS", Timing::uniform(wpm_to_unit_ms(20.0))),
            3000
        );
        // At 20/5 Farnsworth the same word lasts 60s / 5 WPM = 12s, less
        // the rounding of the spacing unit over its 19 spacing units.
        let farnsworth = word_period_ms("PARIS", Timing::farnsworth_wpm(20.0, 5.0));
        assert_eq!(farnsworth, 31 * 60 + 19 * 534);
        assert!(farnsworth.abs_diff(12_000) <= 19, "{farnsworth}ms");
        // Without a following word, the schedule stops at the last tone:
        // 43 units.
        let alone = build_schedule(&build_signal_plan("PARIS"), Timing::uniform(60));
        assert_eq!(schedule_duration_ms(&alone), 43 * 60);
        assert!(alone.last().is_some_and(ScheduleStep::is_tone));
    }

    #[test]
    fn schedule_keeps_the_gaps_of_a_hand_built_plan() {
        let timing = Timing::uniform(10);
        // Leading and trailing gaps are silences of their own; the unit
        // after a tone is only added when something follows the tone.
        assert_eq!(
            build_schedule(
                &[
                    Signal::LetterGap,
                    Signal::Dot,
                    Signal::WordGap,
                    Signal::Dash,
                    Signal::LetterGap,
                ],
                timing
            ),
            vec![
                step(StepKind::LetterGap, 20),
                step(StepKind::Tone, 10),
                step(StepKind::WordGap, 50),
                step(StepKind::Tone, 30),
                step(StepKind::LetterGap, 30),
            ]
        );
        // A word gap stays a word gap whatever is merged into it.
        assert_eq!(
            build_schedule(
                &[Signal::Dot, Signal::WordGap, Signal::LetterGap, Signal::Dot],
                timing
            )[1],
            step(StepKind::WordGap, 70)
        );
    }

    #[test]
    fn schedule_does_not_overflow_on_a_huge_raw_unit_length() {
        let schedule = build_schedule(&build_signal_plan("EE E"), Timing::uniform(u64::MAX));
        assert_eq!(schedule.len(), 5);
        assert!(schedule.iter().all(|step| step.duration_ms == u64::MAX));
        assert_eq!(schedule_duration_ms(&schedule), u64::MAX);
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
        for name in ["BK", "CT", "HH", "SK", "SN", "SOS"] {
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
            ("ПРИВІТ СВІТЕ ЇЖАК ЄДНІСТЬ", Alphabet::Ukrainian),
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
    fn ukrainian_and_russian_share_letters_but_not_codes() {
        // И is `-.--` in Ukrainian Morse, the code Russian Morse gives Ы,
        // and `..` in Russian Morse, the code Ukrainian Morse gives І.
        assert_eq!(encode_in("И", Alphabet::Ukrainian), "-.--");
        assert_eq!(encode_in("И", Alphabet::Cyrillic), "..");
        assert_eq!(encode("ПРИВІТ"), ".--. .-. -.-- .-- .. -");
        assert_eq!(
            encode_in("ПРИВІТ", Alphabet::Cyrillic),
            ".--. .-. .. .-- .. -"
        );
        for (code, ukrainian, russian) in [
            ("..", "І", "И"),
            ("-.--", "И", "Ы"),
            ("..-..", "Є", "Э"),
            (".---.", "Ї", "Ї"),
            ("--.", "Г", "Г"),
            ("-..-", "Ь", "Ь"),
        ] {
            assert_eq!(decode_in(code, Alphabet::Ukrainian), ukrainian, "{code}");
            assert_eq!(decode_in(code, Alphabet::Cyrillic), russian, "{code}");
        }
        // Every other letter the two have in common is sent the same way.
        let shared = "АБВГДЕЖЗЙКЛМНОПРСТУФХЦЧШЩЬЮЯЇ";
        assert_eq!(
            encode_in(shared, Alphabet::Ukrainian),
            encode_in(shared, Alphabet::Cyrillic)
        );
    }

    #[test]
    fn text_without_ukrainian_letters_keeps_the_russian_codes() {
        assert_eq!(encode("привет"), ".--. .-. .. .-- . -");
        assert_eq!(encode("ПРИВЕТ МИР"), ".--. .-. .. .-- . - / -- .. .-.");
        assert_eq!(encode("ЭТО МЫ"), "..-.. - --- / -- -.--");
        assert_eq!(
            decode_in(".--. .-. .. .-- . -", Alphabet::Cyrillic),
            "ПРИВЕТ"
        );
        for text in ["привет", "QTH МОСКВА", "ёж", "София", "ЫІ"] {
            assert_eq!(encode(text), encode_in(text, Alphabet::Cyrillic), "{text}");
            assert_eq!(
                build_signal_plan(text),
                build_signal_plan_in(text, Alphabet::Cyrillic),
                "{text}"
            );
        }
    }

    #[test]
    fn signal_plan_follows_the_detected_ukrainian_alphabet() {
        assert_eq!(
            build_signal_plan("привіт"),
            build_signal_plan_in("ПРИВІТ", Alphabet::Ukrainian)
        );
        // И alone: dash dot dash dash.
        assert_eq!(
            build_signal_plan_in("и", Alphabet::Ukrainian),
            vec![Signal::Dash, Signal::Dot, Signal::Dash, Signal::Dash]
        );
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
        assert_eq!(encode_in("、。", Alphabet::Japanese), ".-.-.- .-.-..");
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
    fn latin_extensions_encode_and_a_prosign_code_stays_a_prosign() {
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
        // multi-symbol letter), and one LetterGap before the next letter.
        let plan = build_signal_plan("<AR>E");
        assert_eq!(
            plan,
            vec![
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
                Signal::LetterGap,
                Signal::Dot,
            ]
        );
    }

    #[test]
    fn signal_plan_never_ends_with_a_gap() {
        for text in ["E", "E E", "SOS <SK>", "E ~", "~ E ~", "한글", "שלום", ""] {
            let plan = build_signal_plan(text);
            assert!(
                plan.last().is_none_or(Signal::is_tone),
                "{text:?}: plan ends with {:?}",
                plan.last()
            );
        }
        // Gaps sit between letters and between words, nowhere else.
        assert_eq!(
            build_signal_plan("EE E"),
            vec![
                Signal::Dot,
                Signal::LetterGap,
                Signal::Dot,
                Signal::LetterGap,
                Signal::WordGap,
                Signal::Dot,
            ]
        );
    }

    #[test]
    fn decode_accepts_look_alike_dots_dashes_and_separators() {
        // Middle dot and bullet; minus sign, en dash, em dash and
        // underscore; ellipsis; vertical bar.
        assert_eq!(decode("··· −−− ···"), "SOS");
        assert_eq!(decode("••• ––– •••"), "SOS");
        assert_eq!(decode("… ——— …"), "SOS");
        assert_eq!(decode("._ _..."), "AB");
        assert_eq!(decode(".- | -..."), "A B");
        assert_eq!(decode(".-|-..."), "A B");
        assert_eq!(decode_in("·−", Alphabet::Cyrillic), "А");
    }

    #[test]
    fn decode_leaves_no_double_space_for_an_empty_or_unknown_word() {
        assert_eq!(decode(".- // -..."), "A B");
        assert_eq!(decode(".- / / -..."), "A B");
        assert_eq!(decode("/ .- /"), "A");
        assert_eq!(decode(".- / ..--..-- / -..."), "A B");
        assert_eq!(decode("//"), "");
        assert_eq!(decode_in("-- // --", Alphabet::Hebrew), "ם ם");
    }

    #[test]
    fn accented_latin_round_trips_through_its_canonical_letter() {
        assert_eq!(decode(&encode("MÜNCHEN ÑU")), "MÜNCHEN ÑU");
        for (code, text) in [
            ("..--", "Ü"),
            ("---.", "Ö"),
            (".-.-", "Ä"),
            ("--.--", "Ñ"),
            ("----", "CH"),
            (".--.-", "Å"),
            ("..-..", "É"),
            ("-.-..", "Ç"),
            (".-..-", "È"),
            ("..--.", "Ð"),
            ("--.-.", "Ĝ"),
            (".---.", "Ĵ"),
            ("...-...", "Ś"),
            (".--..", "Þ"),
            ("--..-.", "Ź"),
            ("--..-", "Ż"),
        ] {
            assert_eq!(decode(code), text, "{code}");
            assert_eq!(decode(&encode(text)), text, "{text}");
        }
        // Letters that share a code come back as the canonical one.
        assert_eq!(decode(&encode("ŬØÆŃŠÀĘ")), "ÜÖÄÑCHÅÉ");
        // The extension codes are Latin's only: other alphabets keep
        // their own letters for them.
        assert_eq!(decode_in("..--", Alphabet::Cyrillic), "Ю");
        assert_eq!(decode_in("----", Alphabet::Greek), "Χ");
    }

    #[test]
    fn latin_letters_without_a_native_code_decode_as_themselves() {
        for (text, alphabet) in [
            ("JUV", Alphabet::Greek),
            ("FVXY", Alphabet::Hebrew),
            ("P", Alphabet::Arabic),
        ] {
            assert_eq!(
                decode_in(&encode_in(text, alphabet), alphabet),
                text,
                "{alphabet:?}"
            );
        }
        // A code the alphabet has a letter for still decodes to that letter.
        assert_eq!(decode_in("--.-", Alphabet::Greek), "Ψ");
        assert_eq!(decode_in(".-", Alphabet::Hebrew), "א");
    }

    #[test]
    fn cyrillic_and_ukrainian_have_a_letter_for_every_latin_code() {
        // So the Latin fallback never applies in either: each of the 26
        // Latin letters' codes decodes to a Cyrillic letter.
        for alphabet in [Alphabet::Cyrillic, Alphabet::Ukrainian] {
            for latin in 'A'..='Z' {
                let decoded = decode_in(TABLE[&latin], alphabet);
                assert!(
                    decoded
                        .chars()
                        .all(|c| ('\u{0400}'..='\u{04FF}').contains(&c)),
                    "{alphabet:?}: {latin} decoded to {decoded}"
                );
                assert_eq!(decoded.chars().count(), 1, "{alphabet:?}: {latin}");
            }
        }
    }

    #[test]
    fn native_letters_decode_exactly_as_listed() {
        // Whatever else a decode table falls back to, the first letter an
        // alphabet lists for a code is what that code decodes to.
        for alphabet in Alphabet::ALL {
            let table = &TABLES[&alphabet].decode;
            let mut seen = std::collections::HashSet::new();
            for &(letter, code) in alphabet.letters() {
                if seen.insert(code) {
                    assert_eq!(table[code], letter.to_string(), "{alphabet:?}: {code}");
                }
            }
        }
    }

    #[test]
    fn prosign_inside_a_word_is_sent_fused_within_that_word() {
        assert_eq!(encode("SOS<SK>"), "... --- ... ...-.-");
        assert_eq!(encode_lossy_report("SOS<SK>").skipped, vec![]);
        assert_eq!(encode("<ar>K"), ".-.-. -.-");
        assert_eq!(encode("A<BT>B<SK>"), ".- -...- -... ...-.-");
        // What decode writes inline encodes back to the same Morse.
        let morse = "... --- ... ...-.-";
        assert_eq!(decode(morse), "SOS<SK>");
        assert_eq!(encode(&decode(morse)), morse);
        // An unknown or unterminated name is still letters, with the
        // brackets reported.
        let report = encode_lossy_report("A<ZZ>B");
        assert_eq!(report.morse, ".- --.. --.. -...");
        assert_eq!(report.skipped, vec!['<', '>']);
        let report = encode_lossy_report("A<<SK>");
        assert_eq!(report.morse, ".- ...-.-");
        assert_eq!(report.skipped, vec!['<']);
        assert_eq!(encode("<SK"), "... -.-");
        assert_eq!(encode("SK>"), "... -.-");
        // One letter gap separates the prosign from its neighbour.
        assert_eq!(
            build_signal_plan("E<AR>"),
            vec![
                Signal::Dot,
                Signal::LetterGap,
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
                Signal::Dash,
                Signal::Dot,
            ]
        );
    }

    #[test]
    fn distress_and_error_prosigns() {
        assert_eq!(encode("<SOS>"), "...---...");
        assert_eq!(encode("<HH>"), "........");
        assert_eq!(decode("...---..."), "<SOS>");
        assert_eq!(decode("........"), "<HH>");
        // Sent as letters, SOS keeps its letter gaps.
        assert_eq!(encode("SOS"), "... --- ...");
    }

    #[test]
    fn prosign_aliases_encode_like_the_canonical_name() {
        for (alias, name) in [("VE", "SN"), ("KA", "CT"), ("VA", "SK")] {
            let canonical = format!("<{name}>");
            for written in [format!("<{alias}>"), format!("<{}>", alias.to_lowercase())] {
                assert_eq!(encode(&written), encode(&canonical), "{written}");
                assert_eq!(encode_lossy_report(&written).skipped, vec![]);
                assert_eq!(decode(&encode(&written)), canonical, "{written}");
            }
            // The alias is the same sign: its letters, fused, are the code.
            let fused: String = alias.chars().map(|c| encode(&c.to_string())).collect();
            assert_eq!(fused, encode(&canonical), "{alias}");
        }
    }

    #[test]
    fn multiplication_sign_is_sent_as_x() {
        let report = encode_lossy_report("3×4");
        assert_eq!(report.morse, "...-- -..- ....-");
        assert_eq!(report.skipped, vec![]);
        assert_eq!(decode(&report.morse), "3X4");
        assert_eq!(encode_in("×", Alphabet::Cyrillic), "-..-");
    }

    #[test]
    fn percent_is_sent_as_zero_fraction_bar_zero() {
        // ITU-R M.1677-1 part I, 3.3: % is sent 0/0, and a number is
        // joined to it by a hyphen (2% is 2-0/0, not 20/0).
        assert_eq!(encode("%"), encode("0/0"));
        assert_eq!(encode("2%"), encode("2-0/0"));
        assert_eq!(encode("12.5%"), encode("12.5-0/0"));
        assert_eq!(encode("٥%"), encode("5-0/0"));
        assert_eq!(encode("2 %"), encode("2 0/0"));
        assert_eq!(encode("A%"), encode("A0/0"));
        assert_eq!(encode("%%"), encode("0/00/0"));
        assert_eq!(encode_lossy_report("2% A%").skipped, vec![]);
        assert_eq!(decode(&encode("50%")), "50-0/0");
        assert_eq!(build_signal_plan("2%"), build_signal_plan("2-0/0"));
        assert_eq!(
            encode_in("2%", Alphabet::Cyrillic),
            encode_in("2-0/0", Alphabet::Cyrillic)
        );
    }

    #[test]
    fn arabic_teh_marbuta_is_sent_as_heh() {
        let report = encode_lossy_report_in("مدرسة", Alphabet::Arabic);
        assert_eq!(report.morse, encode_in("مدرسه", Alphabet::Arabic));
        assert_eq!(report.skipped, vec![]);
        assert_eq!(encode("ة"), "..-..");
    }

    #[test]
    fn zero_width_non_joiner_is_ignored() {
        let text = "می\u{200C}خواهم";
        assert_eq!(Alphabet::detect(text), Alphabet::Persian);
        let report = encode_lossy_report(text);
        assert_eq!(report.skipped, vec![]);
        // Not a letter, and not a word break either.
        assert_eq!(report.morse, encode("میخواهم"));
        assert!(!report.morse.contains('/'));
        let report = encode_lossy_report("A\u{200C}B \u{200C}");
        assert_eq!(report.morse, ".- -...");
        assert_eq!(report.skipped, vec![]);
        assert_eq!(build_signal_plan(text), build_signal_plan("میخواهم"));
    }

    #[test]
    fn latin_text_with_cjk_punctuation_keeps_latin_codes() {
        // An ideographic space is a word break, not a reason to send the
        // brackets with their Wabun codes.
        assert_eq!(encode("HELLO\u{3000}(1)"), encode("HELLO (1)"));
        assert_eq!(
            encode("こんにちは。"),
            encode_in("こんにちは。", Alphabet::Japanese)
        );
    }

    #[test]
    fn decode_report_lists_unknown_codes_in_order() {
        let report = decode_lossy_report("... --- ... ..--..-- / hello / .- ..--..--");
        assert_eq!(report.text, "SOS A");
        assert_eq!(report.skipped, vec!["..--..--", "hello", "..--..--"]);
        // Codes are reported as read, after look-alikes are rewritten.
        assert_eq!(decode_lossy_report("·−·−·−·−").skipped, vec![".-.-.-.-"]);
    }

    #[test]
    fn decode_report_is_empty_when_every_code_is_recognised() {
        for (morse, alphabet) in [
            ("... --- ...", Alphabet::Latin),
            ("... --- ... / ...-.-", Alphabet::Latin),
            ("··· −−− ··· | ..--", Alphabet::Latin),
            ("", Alphabet::Latin),
            (" / ", Alphabet::Latin),
            (".--. .-. .. .-- . -", Alphabet::Cyrillic),
            (".--. .-. -.-- .-- .. -", Alphabet::Ukrainian),
            (".-.. ..", Alphabet::Japanese),
        ] {
            let report = decode_lossy_report_in(morse, alphabet);
            assert_eq!(report.skipped, Vec::<String>::new(), "{morse:?}");
            assert_eq!(report.text, decode_in(morse, alphabet));
        }
        assert_eq!(decode_lossy_report("...").text, decode("..."));
    }

    #[test]
    fn decode_report_depends_on_the_alphabet() {
        // `..--` is Ü in Latin and Ю in Cyrillic, and nothing in Greek.
        assert_eq!(decode_lossy_report("..--").skipped, Vec::<String>::new());
        assert_eq!(
            decode_lossy_report_in("..--", Alphabet::Cyrillic).skipped,
            Vec::<String>::new()
        );
        let report = decode_lossy_report_in(".- ..--", Alphabet::Greek);
        assert_eq!(report.text, "Α");
        assert_eq!(report.skipped, vec!["..--"]);
    }

    #[test]
    fn every_code_is_only_dots_and_dashes() {
        // So none of the look-alikes decode rewrites can be part of a code.
        let is_code = |code: &str| !code.is_empty() && code.chars().all(|s| s == '.' || s == '-');
        for alphabet in Alphabet::ALL {
            let tables = &TABLES[&alphabet];
            for (c, code) in &tables.encode {
                assert!(is_code(code), "{alphabet:?}: {c} -> {code}");
            }
            for code in tables.decode.keys() {
                assert!(is_code(code), "{alphabet:?}: {code}");
            }
        }
        for (name, code) in PROSIGNS.iter() {
            assert!(is_code(code), "<{name}>: {code}");
        }
    }

    #[test]
    fn latin_decode_extensions_use_codes_nothing_else_does() {
        let mut seen = std::collections::HashSet::new();
        for &(code, text) in alphabets::LATIN_DECODE_EXTENSIONS {
            assert!(seen.insert(code), "{code} is listed twice");
            assert!(
                !TABLE.values().any(|used| *used == code),
                "{text}: {code} is in the ASCII table"
            );
            assert!(
                !REVERSE_PROSIGNS.contains_key(code),
                "{text}: {code} is a prosign"
            );
            // The code is one an accented letter is sent with: the letter
            // itself, except for the digraph CH.
            assert!(
                alphabets::LATIN_EXTENSIONS
                    .iter()
                    .any(|&(_, extension)| extension == code),
                "{text}: {code}"
            );
            if text != "CH" {
                assert_eq!(encode(text), code, "{text}");
            }
        }
    }

    #[test]
    fn prosign_names_fuse_into_their_codes() {
        let fused = |name: &str| -> String { name.chars().map(|c| TABLE[&c]).collect() };
        for (&name, &code) in PROSIGNS.iter() {
            assert_eq!(fused(name), code, "<{name}>");
            assert!(name.len() <= PROSIGN_NAME_MAX, "<{name}>");
        }
        // No two names share a code, so decode's choice of name is fixed.
        assert_eq!(REVERSE_PROSIGNS.len(), PROSIGNS.len());
        for &(alias, name) in PROSIGN_ALIASES {
            assert!(!PROSIGNS.contains_key(alias), "<{alias}>");
            assert_eq!(fused(alias), PROSIGNS[name], "<{alias}> is not <{name}>");
            assert!(alias.len() <= PROSIGN_NAME_MAX, "<{alias}>");
        }
    }

    #[test]
    fn every_decodable_letter_round_trips() {
        // Everything a decode table can produce must encode back to the
        // code it came from, and decode to itself again. The exceptions
        // are by design: Hebrew writes כ מ נ פ צ in their final form at
        // the end of a word, and a lone letter is a word; CH is sent as
        // the two letters C and H, not with its own code.
        let expected: &[(Alphabet, &str, &str, &str)] = &[
            (Alphabet::Latin, "----", "CH", "-.-. ...."),
            (Alphabet::Hebrew, "-.-", "כ", "ך"),
            (Alphabet::Hebrew, "--", "מ", "ם"),
            (Alphabet::Hebrew, "-.", "נ", "ן"),
            (Alphabet::Hebrew, ".--.", "פ", "ף"),
            (Alphabet::Hebrew, ".--", "צ", "ץ"),
        ];
        let mut found: Vec<(Alphabet, &str, &str, String)> = Vec::new();
        let mut checked = 0;
        for alphabet in Alphabet::ALL {
            let mut entries: Vec<(&str, &str)> = TABLES[&alphabet]
                .decode
                .iter()
                .map(|(&code, text)| (code, text.as_str()))
                .collect();
            entries.sort();
            for (code, text) in entries {
                checked += 1;
                let encoded = encode_in(text, alphabet);
                let decoded = decode_in(&encoded, alphabet);
                if encoded != code {
                    found.push((alphabet, code, text, encoded));
                } else if decoded != text {
                    found.push((alphabet, code, text, decoded));
                }
                // The code itself always decodes to the table entry or,
                // in Hebrew, its final form.
                let direct = decode_in(code, alphabet);
                assert!(
                    direct == text || alphabet == Alphabet::Hebrew,
                    "{alphabet:?}: {code} decoded to {direct}, not {text}"
                );
            }
        }
        let expected: Vec<(Alphabet, &str, &str, String)> = expected
            .iter()
            .map(|&(alphabet, code, text, result)| (alphabet, code, text, result.to_string()))
            .collect();
        let sort = |mut list: Vec<(Alphabet, &'static str, &'static str, String)>| {
            list.sort_by(|a, b| (a.0.id(), a.1).cmp(&(b.0.id(), b.1)));
            list
        };
        assert_eq!(sort(found), sort(expected));
        // 54 shared characters in each of the 9 alphabets, at the least.
        assert_eq!(Alphabet::ALL.len(), 9);
        assert!(checked >= 9 * 54, "only {checked} entries checked");
    }

    #[test]
    fn no_scalar_value_panics_in_any_alphabet() {
        // Every scalar value in one text, so that marks and joiners meet a
        // preceding character, and the basic plane (which holds every
        // table) again with each character a word of its own.
        let all: String = ('\0'..=char::MAX).collect();
        let words: String = ('\0'..='\u{FFFF}').flat_map(|c| [c, ' ']).collect();
        // Accented-Latin codes with no canonical letter to decode to.
        let undecoded = [
            "-.-..", "..--.", ".-..-", "--.-.", ".---.", "...-...", ".--..", "--..-.", "--..-",
        ];
        std::thread::scope(|scope| {
            for alphabet in Alphabet::ALL {
                let (all, words) = (&all, &words);
                scope.spawn(move || {
                    for text in [all, words] {
                        let report = encode_lossy_report_in(text, alphabet);
                        assert!(
                            report
                                .morse
                                .chars()
                                .all(|s| matches!(s, '.' | '-' | ' ' | '/')),
                            "{alphabet:?}"
                        );
                        // Whatever encode sends, decode recognises.
                        let mut skipped = decode_lossy_report_in(&report.morse, alphabet).skipped;
                        skipped.retain(|code| {
                            alphabet != Alphabet::Latin || !undecoded.contains(&code.as_str())
                        });
                        assert_eq!(skipped, Vec::<String>::new(), "{alphabet:?}");
                        // Text handed to decode by mistake is survivable.
                        decode_lossy_report_in(text, alphabet);
                    }
                    build_signal_plan_in(words, alphabet);
                });
            }
        });
    }
}
