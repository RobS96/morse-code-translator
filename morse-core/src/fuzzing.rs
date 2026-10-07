//! Entry points for the libFuzzer targets in `fuzz/` (see `fuzz.yml`).
//!
//! Each takes the raw fuzzer bytes and checks the invariants the rest of
//! the crate relies on. They are public so the fuzz crate can call them,
//! not a stable API: `#[doc(hidden)]`, and they may change between
//! versions.

use crate::{
    Alphabet, MAX_UNIT_MS, MIN_UNIT_MS, Timing, Tone, build_schedule, build_signal_plan_from_morse,
    build_signal_plan_in, decode_in, decode_lossy_report_in, encode_in, encode_lossy_report_in,
    morse_lossy_report, normalize_input, render_schedule, schedule_duration_ms,
};

/// The first byte picks the alphabet; the rest is the text, read lossily.
fn split(data: &[u8]) -> (Alphabet, String) {
    let (first, rest) = data.split_first().map_or((0, data), |(f, r)| (*f, r));
    let alphabet = Alphabet::ALL[usize::from(first) % Alphabet::ALL.len()];
    (alphabet, String::from_utf8_lossy(rest).into_owned())
}

/// Text → Morse → text in one alphabet: `encode_in` writes only dots,
/// dashes and separators; the lossy report's Morse is exactly the
/// encoding; decoding what was encoded, and the report of it, agree;
/// detection and normalisation accept anything.
pub fn text(data: &[u8]) {
    let (alphabet, text) = split(data);
    let _ = Alphabet::detect(&text);
    let _ = normalize_input(&text);
    let encoded = encode_in(&text, alphabet);
    assert!(
        encoded.chars().all(|c| matches!(c, '.' | '-' | ' ' | '/')),
        "encode_in wrote something other than dots, dashes and separators: {encoded:?}"
    );
    let report = encode_lossy_report_in(&text, alphabet);
    assert_eq!(
        report.morse, encoded,
        "encode_lossy_report_in and encode_in disagree"
    );
    let decoded = decode_in(&encoded, alphabet);
    let back = decode_lossy_report_in(&encoded, alphabet);
    assert_eq!(
        back.text, decoded,
        "decode_lossy_report_in and decode_in disagree"
    );
    assert!(
        back.skipped.is_empty(),
        "decoding what encode_in wrote left codes out: {:?}",
        back.skipped
    );
    let _ = build_signal_plan_in(&text, alphabet);
}

/// Morse as written: decoding never panics and its report agrees with
/// it; the Morse a lossy report says is sent keys the same plan as the
/// original, and is itself lossless.
pub fn morse(data: &[u8]) {
    let (alphabet, morse) = split(data);
    let decoded = decode_in(&morse, alphabet);
    let report = decode_lossy_report_in(&morse, alphabet);
    assert_eq!(
        report.text, decoded,
        "decode_lossy_report_in and decode_in disagree"
    );
    let plan = build_signal_plan_from_morse(&morse);
    let sent = morse_lossy_report(&morse);
    assert_eq!(
        build_signal_plan_from_morse(&sent.morse),
        plan,
        "the Morse reported as sent keys a different plan than the input"
    );
    let again = morse_lossy_report(&sent.morse);
    assert!(
        again.skipped.is_empty(),
        "the Morse reported as sent is itself lossy: {:?}",
        again.skipped
    );
    assert_eq!(
        again.morse, sent.morse,
        "the Morse reported as sent is not a fixed point"
    );
}

/// Timing from arbitrary WPM values stays inside the unit bounds, the
/// schedule of any plan has a finite length, and a short one renders to
/// the expected number of samples within the tone's volume.
pub fn schedule(data: &[u8]) {
    let (wpm, rest) = if data.len() >= 16 {
        let (a, b) = data.split_at(16);
        let char_wpm = f64::from_le_bytes(a[..8].try_into().expect("8 bytes"));
        let eff_wpm = f64::from_le_bytes(a[8..].try_into().expect("8 bytes"));
        ((char_wpm, eff_wpm), b)
    } else {
        ((20.0, 5.0), data)
    };
    let timing = Timing::farnsworth_wpm(wpm.0, wpm.1);
    for unit in [timing.char_unit_ms, timing.gap_unit_ms] {
        assert!(
            (MIN_UNIT_MS..=MAX_UNIT_MS).contains(&unit),
            "unit {unit} ms out of bounds"
        );
    }
    assert!(
        timing.gap_unit_ms >= timing.char_unit_ms,
        "Farnsworth gaps shorter than characters"
    );

    let (alphabet, text) = split(rest);
    let plan = build_signal_plan_in(&text, alphabet);
    let schedule = build_schedule(&plan, timing);
    let duration_ms = schedule_duration_ms(&schedule);
    if duration_ms <= 3_000 {
        let tone = Tone {
            sample_rate: 8_000,
            ..Tone::default()
        };
        let samples = render_schedule(&schedule, &tone).expect("a 3 s schedule renders");
        let expected = duration_ms * 8; // 8 kHz: 8 samples per millisecond
        let got = samples.len() as u64;
        assert!(
            got.abs_diff(expected) <= 1,
            "{duration_ms} ms rendered to {got} samples at 8 kHz, expected about {expected}"
        );
        assert!(
            samples
                .iter()
                .all(|s| s.abs() <= tone.volume + f32::EPSILON),
            "a sample exceeds the tone's volume"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few hundred deterministic inputs through each entry point on
    /// stable, so a broken invariant shows up in `cargo test` and not only
    /// under the nightly fuzz job.
    fn inputs() -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = Vec::new();
        let texts = [
            "",
            " ",
            "/",
            "SOS",
            "HI THERE",
            "<SK> SOS<AR>",
            "... --- ...",
            "-- / .-",
            "ÊTRE DON’T ＳＯＳ！",
            "Привет мир",
            "αβγ",
            "رئیس",
            "日本語",
            "\u{FEFF}... --- ...",
            "--.--",
            "- - -",
            "…",
            "a\u{302}",
            "....  ---\t...",
            "-.-.-.-.-.-.-.-.-.-",
        ];
        for (i, t) in texts.iter().enumerate() {
            for a in 0..Alphabet::ALL.len() {
                let mut v = vec![a as u8];
                v.extend_from_slice(t.as_bytes());
                out.push(v.clone());
                // With a WPM prefix for the schedule target.
                let mut w = Vec::new();
                w.extend_from_slice(&(5.0 + i as f64 * 7.0).to_le_bytes());
                w.extend_from_slice(&(i as f64).to_le_bytes());
                w.extend_from_slice(&v);
                out.push(w);
            }
        }
        // Pseudo-random bytes (a small LCG), including non-UTF-8 and NaN WPMs.
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for len in 0..200usize {
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                x = x
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                v.push((x >> 56) as u8);
            }
            out.push(v);
        }
        for special in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1e300, 1e-300] {
            let mut v = Vec::new();
            v.extend_from_slice(&special.to_le_bytes());
            v.extend_from_slice(&special.to_le_bytes());
            v.extend_from_slice(b"\x00PARIS");
            out.push(v);
        }
        out
    }

    #[test]
    fn every_entry_point_holds_on_the_sample_inputs() {
        let inputs = inputs();
        assert!(inputs.len() > 500);
        for data in &inputs {
            text(data);
            morse(data);
            schedule(data);
        }
    }
}
