#![forbid(unsafe_code)]

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use morse_core::{
    Alphabet, MAX_UNIT_MS, MIN_UNIT_MS, Signal, Timing, UNIT_MS, build_signal_plan_in, decode_in,
    encode_lossy_report_in, wpm_to_unit_ms,
};

fn usage(prog: &str) -> String {
    format!(
        "Morse Code Translator\n\n\
         Usage:\n  \
         {prog} encode <text>            Text -> Morse (supports <SK>, <AR>, ... prosigns)\n  \
         {prog} decode <morse>           Morse -> text (use / between words)\n  \
         {prog} transmit <text> [opts]   Flash + beep the Morse in your terminal\n  \
         {prog} alphabets                List the supported Morse alphabets\n\n\
         Options may come before or after the text.\n\n\
         Options (encode/decode/transmit):\n  \
         -a, --alphabet <NAME>       latin, cyrillic, greek, hebrew, arabic, persian,\n  \
         {pad:28}japanese (Wabun) or korean. Encode/transmit\n  \
         {pad:28}detect it from the text when omitted; decode\n  \
         {pad:28}defaults to latin (Morse can't be detected).\n\n\
         Transmit options:\n  \
         -u, --unit-ms <MS>          Character unit length in ms (default {UNIT_MS})\n  \
         -g, --gap-unit-ms <MS>      Letter/word gap unit length in ms (default: same as -u)\n  \
         --wpm <N>                   Set character speed from words-per-minute\n  \
         --farnsworth-wpm <N>        Set effective overall speed (Farnsworth timing);\n  \
         {pad:28}must not exceed the character speed\n\n\
         Other options:\n  \
         -h, --help                  Show this help\n  \
         -V, --version               Show the version\n\n\
         Characters with no Morse code are left out; encode and transmit name\n\
         them in a warning on stderr.\n\n\
         Examples:\n  \
         {prog} encode \"SOS\"\n  \
         {prog} encode \"CQ CQ <AR>\"\n  \
         {prog} decode \"... --- ...\"\n  \
         {prog} encode \"привет\"\n  \
         {prog} decode \".--. .-. .. .-- . -\" --alphabet cyrillic\n  \
         {prog} transmit \"HELLO WORLD\" --wpm 20\n  \
         {prog} transmit --wpm 20 --farnsworth-wpm 5 \"HELLO WORLD\"\n",
        pad = ""
    )
}

/// Options that take a value, as `(canonical name, accepted spellings)`.
const VALUE_FLAGS: &[(&str, &[&str])] = &[
    ("--alphabet", &["-a", "--alphabet"]),
    ("--unit-ms", &["-u", "--unit-ms"]),
    ("--gap-unit-ms", &["-g", "--gap-unit-ms"]),
    ("--wpm", &["--wpm"]),
    ("--farnsworth-wpm", &["--farnsworth-wpm"]),
];

fn value_flag(arg: &str) -> Option<&'static str> {
    VALUE_FLAGS
        .iter()
        .find(|(_, spellings)| spellings.contains(&arg))
        .map(|(name, _)| *name)
}

fn is_option(arg: &str) -> bool {
    value_flag(arg).is_some() || matches!(arg, "-h" | "--help" | "-V" | "--version")
}

/// What the command line asked for.
#[derive(Debug, PartialEq)]
enum Invocation {
    Help,
    Version,
    Alphabets,
    Run(Box<Request>),
}

#[derive(Debug, PartialEq)]
enum Command {
    Encode,
    Decode,
    Transmit,
}

/// A parsed `encode`/`decode`/`transmit` command line. Option values are
/// kept as written, paired with the spelling used, for error messages.
#[derive(Debug, PartialEq)]
struct Request {
    command: Command,
    text: String,
    alphabet: Option<(String, String)>,
    unit_ms: Option<(String, String)>,
    gap_unit_ms: Option<(String, String)>,
    wpm: Option<(String, String)>,
    farnsworth_wpm: Option<(String, String)>,
}

/// Parse the arguments after the program name.
///
/// Only an exact option name (or `--name=value`) is an option, and options
/// may come before or after the text. Anything else is positional, because
/// Morse text such as `-... ---` or `--` starts with a dash too. An option
/// that takes a value claims the next argument unless that is itself an
/// option, so `--wpm -5` reaches the range check while a forgotten value
/// (`--wpm --alphabet latin`, or `--wpm` last) is reported as missing.
/// When an option is repeated the last value wins.
fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut positionals: Vec<&str> = Vec::new();
    let mut values: Vec<(&'static str, String, String)> = Vec::new();
    let mut version = false;

    let mut it = args.iter().map(String::as_str);
    while let Some(arg) = it.next() {
        if matches!(arg, "-h" | "--help") {
            return Ok(Invocation::Help);
        }
        if matches!(arg, "-V" | "--version") {
            version = true;
        } else if let Some(name) = value_flag(arg) {
            match it.next() {
                Some(value) if !is_option(value) => {
                    values.push((name, arg.to_string(), value.to_string()));
                }
                _ => return Err(format!("{arg} expects a value")),
            }
        } else if let Some((flag, value)) = arg.split_once('=')
            && flag.starts_with("--")
            && let Some(name) = value_flag(flag)
        {
            if value.is_empty() {
                return Err(format!("{flag} expects a value"));
            }
            values.push((name, flag.to_string(), value.to_string()));
        } else {
            positionals.push(arg);
        }
    }
    if version {
        return Ok(Invocation::Version);
    }

    let mut positionals = positionals.into_iter();
    let command = match positionals.next() {
        None => return Err("missing command".to_string()),
        Some("encode") => Command::Encode,
        Some("decode") => Command::Decode,
        Some("transmit") => Command::Transmit,
        Some("alphabets") => {
            return match positionals.next() {
                None => Ok(Invocation::Alphabets),
                Some(extra) => Err(format!("unexpected argument {extra:?}")),
            };
        }
        Some(other) => return Err(format!("unknown command {other:?}")),
    };
    let Some(text) = positionals.next() else {
        return Err("missing text to translate".to_string());
    };
    if let Some(extra) = positionals.next() {
        return Err(format!(
            "unexpected argument {extra:?} (quote text that contains spaces)"
        ));
    }

    let value = |name: &str| {
        values
            .iter()
            .rev()
            .find(|(n, _, _)| *n == name)
            .map(|(_, spelling, value)| (spelling.clone(), value.clone()))
    };
    Ok(Invocation::Run(Box::new(Request {
        command,
        text: text.to_string(),
        alphabet: value("--alphabet"),
        unit_ms: value("--unit-ms"),
        gap_unit_ms: value("--gap-unit-ms"),
        wpm: value("--wpm"),
        farnsworth_wpm: value("--farnsworth-wpm"),
    })))
}

/// Validate a `--*wpm` value: a positive, finite number.
fn parse_wpm((flag, v): &(String, String)) -> Result<f64, String> {
    let wpm: f64 = v
        .parse()
        .map_err(|_| format!("{flag} expects a number, got {v:?}"))?;
    if !wpm.is_finite() || wpm <= 0.0 {
        return Err(format!("{flag} must be a positive number, got {v:?}"));
    }
    Ok(wpm)
}

/// Validate a raw `-u`/`-g` unit length in milliseconds.
fn parse_unit_ms((flag, v): &(String, String)) -> Result<u64, String> {
    let ms: u64 = v
        .parse()
        .map_err(|_| format!("{flag} expects a whole number of ms, got {v:?}"))?;
    if !(MIN_UNIT_MS..=MAX_UNIT_MS).contains(&ms) {
        return Err(format!(
            "{flag} must be between {MIN_UNIT_MS} and {MAX_UNIT_MS} ms, got {ms}"
        ));
    }
    Ok(ms)
}

/// Resolve transmit timing: `--wpm`/`--farnsworth-wpm` take precedence
/// when given, falling back to raw `-u`/`-g` millisecond values, and
/// finally to standard (non-Farnsworth) timing at [`UNIT_MS`].
///
/// Every value given is validated rather than silently ignored: a
/// fat-fingered `--wpm abc` or an out-of-range `-u 99999999999999` is a
/// usage error, not a value quietly swapped for a default the user didn't
/// ask for. So is a `--farnsworth-wpm` above the character speed, which
/// could only mean gaps shorter than standard.
fn resolve_timing(request: &Request) -> Result<Timing, String> {
    let wpm = request.wpm.as_ref().map(parse_wpm).transpose()?;
    let unit_ms = request.unit_ms.as_ref().map(parse_unit_ms).transpose()?;
    let gap_ms = request
        .gap_unit_ms
        .as_ref()
        .map(parse_unit_ms)
        .transpose()?;
    let effective_wpm = request.farnsworth_wpm.as_ref().map(parse_wpm).transpose()?;

    let char_unit_ms = wpm.map(wpm_to_unit_ms).or(unit_ms).unwrap_or(UNIT_MS);

    // `--farnsworth-wpm` is an effective overall speed, not a unit length,
    // so the spacing unit comes from the ARRL formula.
    if let Some(effective_wpm) = effective_wpm {
        let char_wpm = wpm.unwrap_or(1200.0 / char_unit_ms as f64);
        if effective_wpm > char_wpm {
            let char_wpm = format!("{char_wpm:.2}");
            let char_wpm = char_wpm.trim_end_matches('0').trim_end_matches('.');
            return Err(format!(
                "--farnsworth-wpm ({effective_wpm}) must not exceed the character speed \
                 ({char_wpm} WPM)"
            ));
        }
        return Ok(Timing {
            char_unit_ms,
            gap_unit_ms: Timing::farnsworth_wpm(char_wpm, effective_wpm).gap_unit_ms,
        });
    }

    Ok(Timing {
        char_unit_ms,
        gap_unit_ms: gap_ms.unwrap_or(char_unit_ms),
    })
}

/// Resolve `-a`/`--alphabet`: `Ok(None)` when absent (caller picks the
/// default), a usage error for an unknown name.
fn resolve_alphabet(request: &Request) -> Result<Option<Alphabet>, String> {
    match &request.alphabet {
        None => Ok(None),
        Some((_, name)) => Alphabet::from_name(name).map(Some).ok_or_else(|| {
            let ids: Vec<&str> = Alphabet::ALL.iter().map(|a| a.id()).collect();
            format!(
                "unknown alphabet {name:?}; expected one of: {}",
                ids.join(", ")
            )
        }),
    }
}

/// One-line warning naming each distinct dropped character once, or
/// `None` if nothing was dropped.
fn dropped_warning(skipped: &[char]) -> Option<String> {
    let mut distinct: Vec<char> = Vec::new();
    for &c in skipped {
        if !distinct.contains(&c) {
            distinct.push(c);
        }
    }
    if distinct.is_empty() {
        return None;
    }
    let list: Vec<String> = distinct
        .iter()
        .map(|c| format!("{c:?} (U+{:04X})", *c as u32))
        .collect();
    Some(format!(
        "morse: warning: left out {} with no Morse code: {}",
        if skipped.len() == 1 {
            "1 character".to_string()
        } else {
            format!("{} characters", skipped.len())
        },
        list.join(", ")
    ))
}

/// Encode `text`, warning on stderr about anything that was dropped.
fn encode_with_warning(text: &str, alphabet: Alphabet) -> String {
    let report = encode_lossy_report_in(text, alphabet);
    if let Some(warning) = dropped_warning(&report.skipped) {
        eprintln!("{warning}");
    }
    report.morse
}

fn usage_error(prog: &str, err: &str) -> ExitCode {
    eprintln!("morse: {err}\n");
    eprint!("{}", usage(prog));
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let prog = args.first().map(String::as_str).unwrap_or("morse");
    let rest = args.get(1..).unwrap_or_default();

    if rest.is_empty() {
        eprint!("{}", usage(prog));
        return ExitCode::FAILURE;
    }
    let request = match parse_args(rest) {
        Ok(Invocation::Help) => {
            print!("{}", usage(prog));
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("morse {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Alphabets) => {
            for a in Alphabet::ALL {
                println!("{:<10} {}", a.id(), a.native_name());
            }
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Run(request)) => request,
        Err(err) => return usage_error(prog, &err),
    };

    let alphabet = match resolve_alphabet(&request) {
        Ok(a) => a,
        Err(err) => return usage_error(prog, &err),
    };
    let text = request.text.as_str();
    let text_alphabet = || alphabet.unwrap_or_else(|| Alphabet::detect(text));

    match request.command {
        Command::Encode => println!("{}", encode_with_warning(text, text_alphabet())),
        Command::Decode => println!("{}", decode_in(text, alphabet.unwrap_or(Alphabet::Latin))),
        Command::Transmit => match resolve_timing(&request) {
            Ok(timing) => transmit(text, timing, text_alphabet()),
            Err(err) => return usage_error(prog, &err),
        },
    }
    ExitCode::SUCCESS
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
    println!("{}", encode_with_warning(text, alphabet));

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
        rest.iter().map(|s| s.to_string()).collect()
    }

    fn request(rest: &[&str]) -> Request {
        match parse_args(&args(rest)) {
            Ok(Invocation::Run(request)) => *request,
            other => panic!("expected a command, got {other:?}"),
        }
    }

    fn timing(rest: &[&str]) -> Result<Timing, String> {
        resolve_timing(&request(rest))
    }

    #[test]
    fn defaults_to_uniform_unit_ms_with_no_flags() {
        let timing = timing(&["transmit", "SOS"]).unwrap();
        assert_eq!(timing.char_unit_ms, UNIT_MS);
        assert_eq!(timing.gap_unit_ms, UNIT_MS);
    }

    #[test]
    fn wpm_flag_sets_both_char_and_gap_unit_ms() {
        let timing = timing(&["transmit", "SOS", "--wpm", "20"]).unwrap();
        assert_eq!(timing.char_unit_ms, 60);
        assert_eq!(timing.gap_unit_ms, 60);
    }

    #[test]
    fn raw_unit_flags_set_char_and_gap_separately() {
        let timing = timing(&["transmit", "SOS", "-u", "50", "-g", "200"]).unwrap();
        assert_eq!(timing.char_unit_ms, 50);
        assert_eq!(timing.gap_unit_ms, 200);
    }

    #[test]
    fn farnsworth_wpm_only_stretches_the_gap_side() {
        let timing = timing(&["transmit", "SOS", "--wpm", "20", "--farnsworth-wpm", "5"]).unwrap();
        assert_eq!(timing.char_unit_ms, 60);
        // ARRL spacing unit for 20/5, not a bare 1200/5.
        assert_eq!(timing.gap_unit_ms, 534);
    }

    #[test]
    fn flags_may_come_before_or_after_the_text() {
        let after = request(&["transmit", "SOS", "--wpm", "20", "-a", "latin"]);
        for order in [
            &["--wpm", "20", "-a", "latin", "transmit", "SOS"][..],
            &["transmit", "--wpm", "20", "-a", "latin", "SOS"][..],
            &["transmit", "--wpm", "20", "SOS", "-a", "latin"][..],
            &["-a", "latin", "transmit", "SOS", "--wpm", "20"][..],
        ] {
            let parsed = request(order);
            assert_eq!(parsed.text, "SOS", "{order:?}");
            assert_eq!(parsed.command, after.command, "{order:?}");
            assert_eq!(resolve_timing(&parsed), resolve_timing(&after), "{order:?}");
            assert_eq!(resolve_alphabet(&parsed), Ok(Some(Alphabet::Latin)));
        }
    }

    #[test]
    fn flag_value_is_not_mistaken_for_the_text() {
        // The old parser took argv[2] as the text whatever it was.
        let parsed = request(&["decode", "-a", "cyrillic", ".-"]);
        assert_eq!(parsed.text, ".-");
        assert_eq!(resolve_alphabet(&parsed), Ok(Some(Alphabet::Cyrillic)));
    }

    #[test]
    fn long_flags_accept_an_equals_sign() {
        let parsed = request(&["transmit", "SOS", "--wpm=20", "--alphabet=ru"]);
        assert_eq!(resolve_timing(&parsed).unwrap().char_unit_ms, 60);
        assert_eq!(resolve_alphabet(&parsed), Ok(Some(Alphabet::Cyrillic)));
        let err = parse_args(&args(&["transmit", "SOS", "--wpm="])).unwrap_err();
        assert!(err.contains("--wpm expects a value"), "{err}");
    }

    #[test]
    fn morse_text_starting_with_a_dash_is_text() {
        assert_eq!(request(&["decode", "-... ---"]).text, "-... ---");
        assert_eq!(request(&["decode", "--"]).text, "--");
        assert_eq!(request(&["decode", "-"]).text, "-");
        assert_eq!(request(&["-a", "latin", "decode", "-.-"]).text, "-.-");
    }

    #[test]
    fn flag_missing_its_value_is_a_usage_error() {
        for line in [
            &["transmit", "SOS", "--wpm"][..],
            &["transmit", "SOS", "--farnsworth-wpm"][..],
            &["transmit", "SOS", "-u"][..],
            &["transmit", "SOS", "-g"][..],
            &["encode", "SOS", "-a"][..],
            &["encode", "SOS", "--alphabet"][..],
            &["transmit", "SOS", "--wpm", "--farnsworth-wpm", "5"][..],
            &["transmit", "SOS", "--wpm", "--help"][..],
        ] {
            let err = parse_args(&args(line)).unwrap_err();
            assert!(err.contains("expects a value"), "{line:?}: {err}");
        }
        // The text is not swallowed as the value of a flag written last.
        let err = parse_args(&args(&["encode", "-a", "SOS"])).unwrap_err();
        assert!(err.contains("missing text"), "{err}");
    }

    #[test]
    fn missing_or_extra_arguments_are_usage_errors() {
        assert!(parse_args(&args(&["encode"])).is_err());
        assert!(parse_args(&args(&["-a", "latin"])).is_err());
        assert!(parse_args(&args(&["frobnicate", "SOS"])).is_err());
        assert!(parse_args(&args(&["alphabets", "SOS"])).is_err());
        let err = parse_args(&args(&["encode", "HELLO", "WORLD"])).unwrap_err();
        assert!(err.contains("\"WORLD\""), "{err}");
        // A misspelt flag is not silently ignored.
        let err = parse_args(&args(&["transmit", "SOS", "--wmp", "20"])).unwrap_err();
        assert!(err.contains("--wmp"), "{err}");
    }

    #[test]
    fn help_and_version_flags_are_recognised_anywhere() {
        for line in [
            &["--help"][..],
            &["-h"][..],
            &["encode", "SOS", "--help"][..],
            &["-h", "transmit"][..],
            &["--version", "--help"][..],
        ] {
            assert_eq!(parse_args(&args(line)), Ok(Invocation::Help), "{line:?}");
        }
        for line in [
            &["--version"][..],
            &["-V"][..],
            &["encode", "SOS", "-V"][..],
        ] {
            assert_eq!(parse_args(&args(line)), Ok(Invocation::Version), "{line:?}");
        }
        assert_eq!(parse_args(&args(&["alphabets"])), Ok(Invocation::Alphabets));
        assert!(usage("morse").contains("--help") && usage("morse").contains("--version"));
    }

    #[test]
    fn repeated_flag_uses_the_last_value() {
        let timing = timing(&["transmit", "SOS", "--wpm", "10", "--wpm", "20"]).unwrap();
        assert_eq!(timing.char_unit_ms, 60);
    }

    #[test]
    fn farnsworth_wpm_above_wpm_is_a_usage_error() {
        let err =
            timing(&["transmit", "SOS", "--wpm", "10", "--farnsworth-wpm", "20"]).unwrap_err();
        assert!(err.contains("--farnsworth-wpm"), "unexpected error: {err}");
        // Equal speeds are plain standard timing, not an error.
        let equal = timing(&["transmit", "SOS", "--wpm", "20", "--farnsworth-wpm", "20"]).unwrap();
        assert_eq!(equal, Timing::uniform(60));
        // A speed whose unit length rounds (1200 / 13 = 92.3 ms) still
        // accepts itself as the effective speed.
        assert!(timing(&["transmit", "SOS", "--wpm", "13", "--farnsworth-wpm", "13"]).is_ok());
    }

    #[test]
    fn farnsworth_wpm_is_checked_against_the_default_and_raw_character_speed() {
        // No --wpm: the default 100 ms unit is 12 WPM.
        assert!(timing(&["transmit", "SOS", "--farnsworth-wpm", "12"]).is_ok());
        assert!(timing(&["transmit", "SOS", "--farnsworth-wpm", "13"]).is_err());
        // -u 60 is 20 WPM.
        assert!(timing(&["transmit", "SOS", "-u", "60", "--farnsworth-wpm", "5"]).is_ok());
        assert!(timing(&["transmit", "SOS", "-u", "60", "--farnsworth-wpm", "25"]).is_err());
    }

    #[test]
    fn zero_wpm_is_a_usage_error_not_a_silent_hang() {
        let err = timing(&["transmit", "SOS", "--wpm", "0"]).unwrap_err();
        assert!(err.contains("--wpm"), "unexpected error: {err}");
    }

    #[test]
    fn negative_wpm_is_a_usage_error() {
        let err = timing(&["transmit", "SOS", "--wpm", "-5"]).unwrap_err();
        assert!(err.contains("positive"), "unexpected error: {err}");
    }

    #[test]
    fn non_numeric_wpm_is_a_usage_error_not_a_silently_ignored_default() {
        assert!(timing(&["transmit", "SOS", "--wpm", "fast"]).is_err());
        assert!(timing(&["transmit", "SOS", "--farnsworth-wpm", "slow"]).is_err());
        assert!(timing(&["transmit", "SOS", "-g", "1.5"]).is_err());
    }

    #[test]
    fn alphabet_flag_parses_names_and_aliases() {
        assert_eq!(resolve_alphabet(&request(&["decode", ".-"])).unwrap(), None);
        assert_eq!(
            resolve_alphabet(&request(&["decode", ".-", "-a", "ru"])).unwrap(),
            Some(Alphabet::Cyrillic)
        );
        assert_eq!(
            resolve_alphabet(&request(&["decode", ".-", "--alphabet", "Wabun"])).unwrap(),
            Some(Alphabet::Japanese)
        );
    }

    #[test]
    fn unknown_alphabet_is_a_usage_error_listing_the_choices() {
        let err = resolve_alphabet(&request(&["decode", ".-", "-a", "klingon"])).unwrap_err();
        assert!(err.contains("cyrillic") && err.contains("korean"), "{err}");
    }

    #[test]
    fn absurdly_large_raw_unit_ms_is_rejected() {
        let err = timing(&["transmit", "SOS", "-u", "99999999999999"]).unwrap_err();
        assert!(err.contains("-u"), "unexpected error: {err}");
    }

    #[test]
    fn dropped_characters_warning_is_one_line_listing_each_once() {
        assert_eq!(dropped_warning(&[]), None);
        assert_eq!(
            dropped_warning(&['~']).unwrap(),
            "morse: warning: left out 1 character with no Morse code: '~' (U+007E)"
        );
        let warning = dropped_warning(&['~', '#', '~', '\u{0303}']).unwrap();
        assert_eq!(
            warning,
            "morse: warning: left out 4 characters with no Morse code: \
             '~' (U+007E), '#' (U+0023), '\\u{303}' (U+0303)"
        );
        assert!(!warning.contains('\n'));
    }
}
