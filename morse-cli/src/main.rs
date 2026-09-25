#![forbid(unsafe_code)]

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use morse_core::{
    Alphabet, MAX_UNIT_MS, MIN_UNIT_MS, Signal, Timing, UNIT_MS, build_signal_plan_in, decode_in,
    encode_in, wpm_to_unit_ms,
};

fn usage(prog: &str) -> String {
    format!(
        "Morse Code Translator\n\n\
         Usage:\n  \
         {prog} encode <text>            Text -> Morse (supports <SK>, <AR>, ... prosigns)\n  \
         {prog} decode <morse>           Morse -> text (use / between words)\n  \
         {prog} transmit <text> [opts]   Flash + beep the Morse in your terminal\n  \
         {prog} alphabets                List the supported Morse alphabets\n\n\
         Options (encode/decode/transmit):\n  \
         -a, --alphabet <NAME>       latin, cyrillic, greek, hebrew, arabic, persian,\n  \
                                     japanese (Wabun) or korean. Encode/transmit\n  \
                                     detect it from the text when omitted; decode\n  \
                                     defaults to latin (Morse can't be detected).\n\n\
         Transmit options:\n  \
         -u, --unit-ms <MS>          Character unit length in ms (default {UNIT_MS})\n  \
         -g, --gap-unit-ms <MS>      Letter/word gap unit length in ms (default: same as -u)\n  \
         --wpm <N>                   Set character speed from words-per-minute\n  \
         --farnsworth-wpm <N>        Set gap speed from words-per-minute (Farnsworth timing)\n\n\
         Examples:\n  \
         {prog} encode \"SOS\"\n  \
         {prog} encode \"CQ CQ <AR>\"\n  \
         {prog} decode \"... --- ...\"\n  \
         {prog} encode \"привет\"\n  \
         {prog} decode \".--. .-. .. .-- . -\" --alphabet cyrillic\n  \
         {prog} transmit \"HELLO WORLD\" --wpm 20\n  \
         {prog} transmit \"HELLO WORLD\" --wpm 20 --farnsworth-wpm 5\n"
    )
}

fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    args.iter()
        .position(|a| names.contains(&a.as_str()))
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Resolve one side (character or gap) of the transmit timing: the named
/// `--*-wpm` flag takes precedence when given, falling back to the raw
/// `-u`/`-g` millisecond flags, and finally to `default`.
///
/// Every value is validated rather than silently ignored on a parse
/// failure: a fat-fingered `--wpm abc` or an out-of-range `-u
/// 99999999999999` is a usage error, not a value quietly swapped for a
/// default the user didn't ask for.
fn resolve_unit_ms(
    args: &[String],
    wpm_flag: &str,
    raw_flags: &[&str],
    default: u64,
) -> Result<u64, String> {
    if let Some(v) = flag_value(args, &[wpm_flag]) {
        let wpm: f64 = v
            .parse()
            .map_err(|_| format!("{wpm_flag} expects a number, got {v:?}"))?;
        if !wpm.is_finite() || wpm <= 0.0 {
            return Err(format!("{wpm_flag} must be a positive number, got {v:?}"));
        }
        return Ok(wpm_to_unit_ms(wpm));
    }
    if let Some(v) = flag_value(args, raw_flags) {
        let ms: u64 = v
            .parse()
            .map_err(|_| format!("{} expects a whole number of ms, got {v:?}", raw_flags[0]))?;
        if !(MIN_UNIT_MS..=MAX_UNIT_MS).contains(&ms) {
            return Err(format!(
                "{} must be between {MIN_UNIT_MS} and {MAX_UNIT_MS} ms, got {ms}",
                raw_flags[0]
            ));
        }
        return Ok(ms);
    }
    Ok(default)
}

/// Resolve transmit timing from CLI flags: `--wpm`/`--farnsworth-wpm` take
/// precedence when given, falling back to raw `-u`/`-g` millisecond values,
/// and finally to standard (non-Farnsworth) timing at [`UNIT_MS`].
fn resolve_timing(args: &[String]) -> Result<Timing, String> {
    let char_unit_ms = resolve_unit_ms(args, "--wpm", &["-u", "--unit-ms"], UNIT_MS)?;
    let gap_unit_ms = resolve_unit_ms(
        args,
        "--farnsworth-wpm",
        &["-g", "--gap-unit-ms"],
        char_unit_ms,
    )?;

    Ok(Timing {
        char_unit_ms,
        gap_unit_ms,
    })
}

/// Resolve `-a`/`--alphabet`: `Ok(None)` when absent (caller picks the
/// default), a usage error for an unknown name.
fn resolve_alphabet(args: &[String]) -> Result<Option<Alphabet>, String> {
    match flag_value(args, &["-a", "--alphabet"]) {
        None => Ok(None),
        Some(name) => Alphabet::from_name(name).map(Some).ok_or_else(|| {
            let ids: Vec<&str> = Alphabet::ALL.iter().map(|a| a.id()).collect();
            format!(
                "unknown alphabet {name:?}; expected one of: {}",
                ids.join(", ")
            )
        }),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let prog = args.first().map(String::as_str).unwrap_or("morse");

    if args.get(1).map(String::as_str) == Some("alphabets") {
        for a in Alphabet::ALL {
            println!("{:<10} {}", a.id(), a.native_name());
        }
        return ExitCode::SUCCESS;
    }

    let (Some(cmd), Some(arg)) = (args.get(1), args.get(2)) else {
        eprint!("{}", usage(prog));
        return ExitCode::FAILURE;
    };

    let alphabet = match resolve_alphabet(&args) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("morse: {err}\n");
            eprint!("{}", usage(prog));
            return ExitCode::FAILURE;
        }
    };
    let text_alphabet = || alphabet.unwrap_or_else(|| Alphabet::detect(arg));

    match cmd.as_str() {
        "encode" => {
            println!("{}", encode_in(arg, text_alphabet()));
            ExitCode::SUCCESS
        }
        "decode" => {
            println!("{}", decode_in(arg, alphabet.unwrap_or(Alphabet::Latin)));
            ExitCode::SUCCESS
        }
        "transmit" => match resolve_timing(&args) {
            Ok(timing) => {
                transmit(arg, timing, text_alphabet());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("morse: {err}\n");
                eprint!("{}", usage(prog));
                ExitCode::FAILURE
            }
        },
        _ => {
            eprint!("{}", usage(prog));
            ExitCode::FAILURE
        }
    }
}

/// Play a text message out as visual flashes + terminal-bell beeps, timed
/// according to standard Morse ratios (dot=1u, dash=3u, gaps 1u/3u/7u for
/// symbol/letter/word) — or, under Farnsworth `timing`, with letter/word
/// gaps stretched independently of character speed.
fn transmit(text: &str, timing: Timing, alphabet: Alphabet) {
    if timing.gap_unit_ms == timing.char_unit_ms {
        println!("Transmitting \"{text}\" @ {}ms/unit\n", timing.char_unit_ms);
    } else {
        println!(
            "Transmitting \"{text}\" @ {}ms/unit (chars), {}ms/unit (gaps, Farnsworth)\n",
            timing.char_unit_ms, timing.gap_unit_ms
        );
    }
    println!("{}", encode_in(text, alphabet));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for signal in build_signal_plan_in(text, alphabet) {
        if signal.is_tone() {
            // \x07 = terminal bell (audible beep in most terminal apps).
            // \x1b[7m..\x1b[0m briefly inverts the colors for a visual flash.
            print!("\x07\x1b[7m  \x1b[0m");
            // Ignore the error: a closed stdout (e.g. piping into `head`)
            // should end the transmission quietly, not panic.
            let _ = out.flush();
            sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
            print!("\r    \r");
            let _ = out.flush();
            // 1-unit gap after every symbol, at character speed.
            sleep(Duration::from_millis(timing.char_unit_ms));
        } else {
            if matches!(signal, Signal::WordGap) {
                println!();
            }
            sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(rest: &[&str]) -> Vec<String> {
        std::iter::once("morse".to_string())
            .chain(rest.iter().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn defaults_to_uniform_unit_ms_with_no_flags() {
        let timing = resolve_timing(&args(&["transmit", "SOS"])).unwrap();
        assert_eq!(timing.char_unit_ms, UNIT_MS);
        assert_eq!(timing.gap_unit_ms, UNIT_MS);
    }

    #[test]
    fn wpm_flag_sets_both_char_and_gap_unit_ms() {
        let timing = resolve_timing(&args(&["transmit", "SOS", "--wpm", "20"])).unwrap();
        assert_eq!(timing.char_unit_ms, 60);
        assert_eq!(timing.gap_unit_ms, 60);
    }

    #[test]
    fn farnsworth_wpm_only_stretches_the_gap_side() {
        let timing = resolve_timing(&args(&[
            "transmit",
            "SOS",
            "--wpm",
            "20",
            "--farnsworth-wpm",
            "5",
        ]))
        .unwrap();
        assert_eq!(timing.char_unit_ms, 60);
        assert_eq!(timing.gap_unit_ms, 240);
    }

    #[test]
    fn zero_wpm_is_a_usage_error_not_a_silent_hang() {
        let err = resolve_timing(&args(&["transmit", "SOS", "--wpm", "0"])).unwrap_err();
        assert!(err.contains("--wpm"), "unexpected error: {err}");
    }

    #[test]
    fn negative_wpm_is_a_usage_error() {
        assert!(resolve_timing(&args(&["transmit", "SOS", "--wpm", "-5"])).is_err());
    }

    #[test]
    fn non_numeric_wpm_is_a_usage_error_not_a_silently_ignored_default() {
        assert!(resolve_timing(&args(&["transmit", "SOS", "--wpm", "fast"])).is_err());
    }

    #[test]
    fn alphabet_flag_parses_names_and_aliases() {
        assert_eq!(resolve_alphabet(&args(&["decode", ".-"])).unwrap(), None);
        assert_eq!(
            resolve_alphabet(&args(&["decode", ".-", "-a", "ru"])).unwrap(),
            Some(Alphabet::Cyrillic)
        );
        assert_eq!(
            resolve_alphabet(&args(&["decode", ".-", "--alphabet", "Wabun"])).unwrap(),
            Some(Alphabet::Japanese)
        );
    }

    #[test]
    fn unknown_alphabet_is_a_usage_error_listing_the_choices() {
        let err = resolve_alphabet(&args(&["decode", ".-", "-a", "klingon"])).unwrap_err();
        assert!(err.contains("cyrillic") && err.contains("korean"), "{err}");
    }

    #[test]
    fn absurdly_large_raw_unit_ms_is_rejected() {
        let err = resolve_timing(&args(&["transmit", "SOS", "-u", "99999999999999"])).unwrap_err();
        assert!(err.contains("-u"), "unexpected error: {err}");
    }
}
