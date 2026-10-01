#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use morse_core::{
    Alphabet, MAX_UNIT_MS, MIN_UNIT_MS, Signal, Timing, UNIT_MS, build_signal_plan_in,
    decode_lossy_report_in, encode_lossy_report_in, wpm_to_unit_ms,
};

/// Exit code for a command line that could not be understood.
const EXIT_USAGE: u8 = 1;
/// Exit code under `--strict` when part of the input was left out.
const EXIT_LOSSY: u8 = 2;
/// Exit code when standard input could not be read or output written.
const EXIT_IO: u8 = 3;

/// Slowest speed `--wpm` and `--farnsworth-wpm` accept (1200 ms per unit).
const MIN_WPM: f64 = 1.0;
/// Fastest speed `--wpm` and `--farnsworth-wpm` accept (12 ms per unit).
const MAX_WPM: f64 = 100.0;

/// How many distinct unrecognised codes a decode warning lists in full.
const LISTED_CODES: usize = 10;

fn usage(prog: &str) -> String {
    format!(
        "Morse Code Translator\n\n\
         Usage:\n  \
         {prog} encode [text]            Text -> Morse (supports <SK>, <AR>, ... prosigns)\n  \
         {prog} decode [morse]           Morse -> text (use / between words)\n  \
         {prog} transmit [text] [opts]   Flash + beep the Morse in your terminal\n  \
         {prog} alphabets                List the supported Morse alphabets\n\n\
         Options may come before or after the text. With no text argument, the\n\
         text is read from standard input, unless that is a terminal.\n\n\
         Options (encode/decode/transmit):\n  \
         -a, --alphabet <NAME>       latin, cyrillic, greek, hebrew, arabic, persian,\n  \
         {pad:28}japanese (Wabun) or korean. Encode/transmit\n  \
         {pad:28}detect it from the text when omitted; decode\n  \
         {pad:28}defaults to latin (Morse can't be detected).\n\n\
         Options (encode/decode):\n  \
         --strict                    Exit with code {EXIT_LOSSY} if anything was left out\n\n\
         Transmit options:\n  \
         -u, --unit-ms <MS>          Character unit length in ms (default {UNIT_MS})\n  \
         -g, --gap-unit-ms <MS>      Letter/word gap unit length in ms (default: the\n  \
         {pad:28}character unit; must not be shorter than it)\n  \
         --wpm <N>                   Set character speed from words-per-minute\n  \
         {pad:28}({MIN_WPM} to {MAX_WPM}), instead of -u\n  \
         --farnsworth-wpm <N>        Set effective overall speed (Farnsworth timing),\n  \
         {pad:28}instead of -g; must not exceed the character\n  \
         {pad:28}speed\n\n\
         Other options:\n  \
         -h, --help                  Show this help\n  \
         -V, --version               Show the version\n\n\
         Characters with no Morse code, and codes that are not recognised, are\n\
         left out and named in a warning on stderr.\n\n\
         Exit codes:\n  \
         0  success\n  \
         {EXIT_USAGE}  the command line could not be understood\n  \
         {EXIT_LOSSY}  --strict was given and something was left out\n  \
         {EXIT_IO}  standard input could not be read, or the output written\n\n\
         Examples:\n  \
         {prog} encode \"SOS\"\n  \
         {prog} encode \"CQ CQ <AR>\"\n  \
         {prog} decode \"... --- ...\"\n  \
         {prog} encode \"привет\"\n  \
         {prog} decode \".--. .-. .. .-- . -\" --alphabet cyrillic\n  \
         echo \"SOS\" | {prog} encode --strict\n  \
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
    value_flag(arg).is_some() || matches!(arg, "-h" | "--help" | "-V" | "--version" | "--strict")
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
/// `text` is `None` when no text argument was given.
#[derive(Debug, PartialEq)]
struct Request {
    command: Command,
    text: Option<String>,
    strict: bool,
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
    let mut strict = false;

    let mut it = args.iter().map(String::as_str);
    while let Some(arg) = it.next() {
        if matches!(arg, "-h" | "--help") {
            return Ok(Invocation::Help);
        }
        if matches!(arg, "-V" | "--version") {
            version = true;
        } else if arg == "--strict" {
            strict = true;
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
    let text = positionals.next();
    if let Some(extra) = positionals.next() {
        return Err(format!(
            "unexpected argument {extra:?} (quote text that contains spaces)"
        ));
    }
    if strict && command == Command::Transmit {
        return Err("--strict applies to encode and decode only".to_string());
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
        text: text.map(str::to_string),
        strict,
        alphabet: value("--alphabet"),
        unit_ms: value("--unit-ms"),
        gap_unit_ms: value("--gap-unit-ms"),
        wpm: value("--wpm"),
        farnsworth_wpm: value("--farnsworth-wpm"),
    })))
}

/// Validate a `--*wpm` value: a number from [`MIN_WPM`] to [`MAX_WPM`].
fn parse_wpm((flag, v): &(String, String)) -> Result<f64, String> {
    let wpm: f64 = v
        .parse()
        .map_err(|_| format!("{flag} expects a number, got {v:?}"))?;
    if !wpm.is_finite() || wpm <= 0.0 {
        return Err(format!("{flag} must be a positive number, got {v:?}"));
    }
    if !(MIN_WPM..=MAX_WPM).contains(&wpm) {
        return Err(format!(
            "{flag} must be between {MIN_WPM} and {MAX_WPM} WPM, got {v:?}"
        ));
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

/// Resolve transmit timing: the character unit from `--wpm` or `-u`, the
/// gap unit from `--farnsworth-wpm` or `-g`, and standard
/// (non-Farnsworth) timing at [`UNIT_MS`] for whatever is not given.
///
/// Every value given is validated rather than silently ignored: a
/// fat-fingered `--wpm abc` or an out-of-range `-u 99999999999999` is a
/// usage error, not a value quietly swapped for a default the user didn't
/// ask for. So are two options that set the same unit, and a
/// `--farnsworth-wpm` above the character speed or a `-g` below the
/// character unit, which could only mean gaps shorter than standard.
fn resolve_timing(request: &Request) -> Result<Timing, String> {
    if let (Some((wpm, _)), Some((unit, _))) = (&request.wpm, &request.unit_ms) {
        return Err(format!(
            "{wpm} and {unit} both set the character speed; give only one"
        ));
    }
    if let (Some((farnsworth, _)), Some((gap, _))) = (&request.farnsworth_wpm, &request.gap_unit_ms)
    {
        return Err(format!(
            "{farnsworth} and {gap} both set the letter/word gaps; give only one"
        ));
    }
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

    let gap_unit_ms = gap_ms.unwrap_or(char_unit_ms);
    if gap_unit_ms < char_unit_ms {
        let flag = request.gap_unit_ms.as_ref().map_or("-g", |(flag, _)| flag);
        return Err(format!(
            "{flag} ({gap_unit_ms} ms) must not be shorter than the character unit \
             ({char_unit_ms} ms)"
        ));
    }
    Ok(Timing {
        char_unit_ms,
        gap_unit_ms,
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

/// One-line warning naming each distinct unrecognised code once (the
/// first [`LISTED_CODES`] of them), or `None` if every code was recognised.
fn unrecognised_warning(skipped: &[String], alphabet: Alphabet) -> Option<String> {
    let mut distinct: Vec<&String> = Vec::new();
    for code in skipped {
        if !distinct.contains(&code) {
            distinct.push(code);
        }
    }
    if distinct.is_empty() {
        return None;
    }
    let mut list: Vec<String> = distinct
        .iter()
        .take(LISTED_CODES)
        .map(|code| format!("{code:?}"))
        .collect();
    if distinct.len() > LISTED_CODES {
        list.push(format!("and {} more", distinct.len() - LISTED_CODES));
    }
    Some(format!(
        "morse: warning: left out {} not recognised in the {} alphabet: {}",
        if skipped.len() == 1 {
            "1 code".to_string()
        } else {
            format!("{} codes", skipped.len())
        },
        alphabet.id(),
        list.join(", ")
    ))
}

/// Write a line to stderr. A stderr that has gone away is no reason to
/// panic, which is what `eprintln!` does.
fn warn(message: &str) {
    let _ = writeln!(io::stderr().lock(), "{message}");
}

/// The name to show in the usage text: the file name of `argv[0]`,
/// without the directory it was run from.
fn program_name(arg0: Option<OsString>) -> String {
    arg0.as_deref()
        .map(Path::new)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "morse".to_string())
}

/// The arguments as UTF-8, or a usage error naming the first that is not.
fn utf8_args(args: impl Iterator<Item = OsString>) -> Result<Vec<String>, String> {
    args.map(|arg| {
        arg.into_string().map_err(|arg| {
            format!(
                "argument {:?} is not valid UTF-8",
                arg.to_string_lossy().as_ref()
            )
        })
    })
    .collect()
}

/// The text piped to standard input, without its final line break. `None`
/// if `stdin` is a terminal (it is then not read) or holds nothing.
fn piped_text(mut stdin: impl Read, is_terminal: bool) -> io::Result<Option<String>> {
    if is_terminal {
        return Ok(None);
    }
    let mut text = String::new();
    stdin.read_to_string(&mut text)?;
    if text.is_empty() {
        return Ok(None);
    }
    text.truncate(text.trim_end_matches(['\r', '\n']).len());
    Ok(Some(text))
}

/// Why a run ends without success.
#[derive(Debug)]
enum Failure {
    /// No arguments at all: the usage text is the whole answer.
    NoArguments,
    /// The command line could not be understood.
    Usage(String),
    /// `--strict` was given and part of the input was left out.
    Lossy,
    /// Standard input could not be read.
    Input(io::Error),
    /// Standard output could not be written.
    Output(io::Error),
}

impl From<io::Error> for Failure {
    fn from(err: io::Error) -> Self {
        Failure::Output(err)
    }
}

/// The outcome of printing a translation. Under `--strict` the verdict on
/// the input stands even if nobody was there to read the output.
fn translated(written: io::Result<()>, lossy: bool) -> Result<(), Failure> {
    match written {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(Failure::Output(err)),
        _ if lossy => Err(Failure::Lossy),
        written => Ok(written?),
    }
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let prog = program_name(args.next());
    let stdout = io::stdout();
    match run(args, &prog, &mut stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::NoArguments) => {
            warn(usage(&prog).trim_end());
            ExitCode::from(EXIT_USAGE)
        }
        Err(Failure::Usage(err)) => {
            warn(&format!("morse: {err}\n\n{}", usage(&prog).trim_end()));
            ExitCode::from(EXIT_USAGE)
        }
        Err(Failure::Lossy) => ExitCode::from(EXIT_LOSSY),
        // Nobody is reading any more (`morse ... | head -1`): stop quietly.
        Err(Failure::Output(err)) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(Failure::Output(err)) => {
            warn(&format!("morse: cannot write to standard output: {err}"));
            ExitCode::from(EXIT_IO)
        }
        Err(Failure::Input(err)) => {
            warn(&format!("morse: cannot read standard input: {err}"));
            ExitCode::from(EXIT_IO)
        }
    }
}

/// Carry out the command line `args` (without the program name), writing
/// results to `out` and warnings to stderr.
fn run(
    args: impl Iterator<Item = OsString>,
    prog: &str,
    out: &mut impl Write,
) -> Result<(), Failure> {
    let args = utf8_args(args).map_err(Failure::Usage)?;
    if args.is_empty() {
        return Err(Failure::NoArguments);
    }
    let request = match parse_args(&args).map_err(Failure::Usage)? {
        Invocation::Help => {
            write!(out, "{}", usage(prog))?;
            return Ok(());
        }
        Invocation::Version => {
            writeln!(out, "morse {}", env!("CARGO_PKG_VERSION"))?;
            return Ok(());
        }
        Invocation::Alphabets => {
            for a in Alphabet::ALL {
                writeln!(out, "{:<10} {}", a.id(), a.native_name())?;
            }
            return Ok(());
        }
        Invocation::Run(request) => request,
    };

    let alphabet = resolve_alphabet(&request).map_err(Failure::Usage)?;
    // Checked before any text is read from standard input.
    let timing = if request.command == Command::Transmit {
        resolve_timing(&request).map_err(Failure::Usage)?
    } else {
        Timing::uniform(UNIT_MS)
    };
    let Request {
        command,
        text,
        strict,
        ..
    } = *request;
    let text = match text {
        Some(text) => text,
        None => {
            let stdin = io::stdin();
            let is_terminal = stdin.is_terminal();
            piped_text(stdin.lock(), is_terminal)
                .map_err(Failure::Input)?
                .ok_or_else(|| Failure::Usage("missing text to translate".to_string()))?
        }
    };
    let text_alphabet = || alphabet.unwrap_or_else(|| Alphabet::detect(&text));

    match command {
        Command::Encode => {
            let report = encode_lossy_report_in(&text, text_alphabet());
            if let Some(warning) = dropped_warning(&report.skipped) {
                warn(&warning);
            }
            translated(
                writeln!(out, "{}", report.morse),
                strict && !report.skipped.is_empty(),
            )
        }
        Command::Decode => {
            let alphabet = alphabet.unwrap_or(Alphabet::Latin);
            let report = decode_lossy_report_in(&text, alphabet);
            if let Some(warning) = unrecognised_warning(&report.skipped, alphabet) {
                warn(&warning);
            }
            translated(
                writeln!(out, "{}", report.text),
                strict && !report.skipped.is_empty(),
            )
        }
        Command::Transmit => Ok(transmit(out, &text, timing, text_alphabet())?),
    }
}

/// Play a text message out as visual flashes + terminal-bell beeps, timed
/// according to standard Morse ratios (dot=1u, dash=3u, gaps 1u/3u/7u for
/// symbol/letter/word) — or, under Farnsworth `timing`, with letter/word
/// gaps stretched independently of character speed.
///
/// A failed write ends the transmission there: once stdout has been
/// closed (e.g. piping into `head`) nobody is left to see the rest.
fn transmit(
    out: &mut impl Write,
    text: &str,
    timing: Timing,
    alphabet: Alphabet,
) -> io::Result<()> {
    if timing.gap_unit_ms == timing.char_unit_ms {
        writeln!(
            out,
            "Transmitting \"{text}\" @ {}ms/unit\n",
            timing.char_unit_ms
        )?;
    } else {
        writeln!(
            out,
            "Transmitting \"{text}\" @ {}ms/unit (chars), {}ms/unit (gaps, Farnsworth)\n",
            timing.char_unit_ms, timing.gap_unit_ms
        )?;
    }
    let report = encode_lossy_report_in(text, alphabet);
    if let Some(warning) = dropped_warning(&report.skipped) {
        warn(&warning);
    }
    writeln!(out, "{}", report.morse)?;

    for signal in build_signal_plan_in(text, alphabet) {
        if signal.is_tone() {
            // \x07 = terminal bell (audible beep in most terminal apps).
            // \x1b[7m..\x1b[0m briefly inverts the colors for a visual flash.
            write!(out, "\x07\x1b[7m  \x1b[0m")?;
            out.flush()?;
            sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
            write!(out, "\r    \r")?;
            out.flush()?;
            // 1-unit gap after every symbol, at character speed.
            sleep(Duration::from_millis(timing.char_unit_ms));
        } else {
            if matches!(signal, Signal::WordGap) {
                writeln!(out)?;
            }
            sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
        }
    }
    writeln!(out)
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
            assert_eq!(parsed.text.as_deref(), Some("SOS"), "{order:?}");
            assert_eq!(parsed.command, after.command, "{order:?}");
            assert_eq!(resolve_timing(&parsed), resolve_timing(&after), "{order:?}");
            assert_eq!(resolve_alphabet(&parsed), Ok(Some(Alphabet::Latin)));
        }
    }

    #[test]
    fn flag_value_is_not_mistaken_for_the_text() {
        // The old parser took argv[2] as the text whatever it was.
        let parsed = request(&["decode", "-a", "cyrillic", ".-"]);
        assert_eq!(parsed.text.as_deref(), Some(".-"));
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
        let text = |rest: &[&str]| request(rest).text;
        assert_eq!(text(&["decode", "-... ---"]).as_deref(), Some("-... ---"));
        assert_eq!(text(&["decode", "--"]).as_deref(), Some("--"));
        assert_eq!(text(&["decode", "-"]).as_deref(), Some("-"));
        assert_eq!(
            text(&["-a", "latin", "decode", "-.-"]).as_deref(),
            Some("-.-")
        );
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
        // A flag written last takes the text as its value, leaving none.
        assert_eq!(request(&["encode", "-a", "SOS"]).text, None);
    }

    #[test]
    fn text_argument_is_optional_until_stdin_has_been_consulted() {
        for command in ["encode", "decode", "transmit"] {
            assert_eq!(request(&[command]).text, None);
        }
        assert_eq!(request(&["encode", ""]).text.as_deref(), Some(""));
    }

    #[test]
    fn piped_text_is_read_unless_stdin_is_a_terminal() {
        let piped = |input: &str| piped_text(input.as_bytes(), false).unwrap();
        assert_eq!(piped("SOS\n").as_deref(), Some("SOS"));
        assert_eq!(piped("SOS").as_deref(), Some("SOS"));
        assert_eq!(piped("... --- ...\r\n").as_deref(), Some("... --- ..."));
        // Only the final line break goes; inner ones separate words.
        assert_eq!(piped("HI\nTHERE\n\n").as_deref(), Some("HI\nTHERE"));
        assert_eq!(piped(" SOS \n").as_deref(), Some(" SOS "));
        // An empty line is empty text; nothing at all is no text.
        assert_eq!(piped("\n").as_deref(), Some(""));
        assert_eq!(piped(""), None);
        // A terminal is never read: the bytes here stay unread.
        let mut terminal = "SOS\n".as_bytes();
        assert_eq!(piped_text(&mut terminal, true).unwrap(), None);
        assert_eq!(terminal, b"SOS\n");
        // Bytes that are not UTF-8 are an error, not a panic.
        assert!(piped_text(&b"SOS \xff"[..], false).is_err());
    }

    #[test]
    fn strict_flag_is_recognised_anywhere_for_encode_and_decode() {
        for line in [
            &["encode", "SOS", "--strict"][..],
            &["--strict", "encode", "SOS"][..],
            &["decode", "--strict", "..."][..],
            &["decode", "--strict"][..],
        ] {
            assert!(request(line).strict, "{line:?}");
        }
        assert!(!request(&["encode", "SOS"]).strict);
        let err = parse_args(&args(&["transmit", "SOS", "--strict"])).unwrap_err();
        assert!(err.contains("--strict"), "{err}");
        // It is an option, so not the value of the one before it.
        let err = parse_args(&args(&["encode", "SOS", "-a", "--strict"])).unwrap_err();
        assert!(err.contains("expects a value"), "{err}");
    }

    #[test]
    fn program_name_is_the_file_name_of_argv0() {
        assert_eq!(program_name(Some("/usr/local/bin/morse".into())), "morse");
        assert_eq!(program_name(Some("./target/debug/morse".into())), "morse");
        assert_eq!(program_name(Some("morse".into())), "morse");
        assert_eq!(program_name(Some("cw".into())), "cw");
        assert_eq!(program_name(Some("".into())), "morse");
        assert_eq!(program_name(None), "morse");
        assert!(usage("cw").contains("\n  cw encode [text]"));
    }

    #[test]
    fn arguments_must_be_utf8() {
        let given = ["encode", "привет", "-a", "ru"].map(OsString::from);
        assert_eq!(
            utf8_args(given.into_iter()),
            Ok(args(&["encode", "привет", "-a", "ru"]))
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let given = [
                OsString::from("encode"),
                OsString::from_vec(b"SOS\xff".to_vec()),
            ];
            let err = utf8_args(given.into_iter()).unwrap_err();
            assert!(
                err.contains("not valid UTF-8") && err.contains("SOS"),
                "{err}"
            );
        }
    }

    #[test]
    fn printing_outcome_keeps_the_strict_verdict_over_a_closed_pipe() {
        let closed = || Err(io::Error::from(io::ErrorKind::BrokenPipe));
        let full = || Err(io::Error::from(io::ErrorKind::StorageFull));
        assert!(matches!(translated(Ok(()), false), Ok(())));
        assert!(matches!(translated(Ok(()), true), Err(Failure::Lossy)));
        assert!(matches!(translated(closed(), true), Err(Failure::Lossy)));
        assert!(matches!(
            translated(closed(), false),
            Err(Failure::Output(err)) if err.kind() == io::ErrorKind::BrokenPipe
        ));
        // A real write error is reported whatever the verdict.
        for lossy in [false, true] {
            assert!(matches!(
                translated(full(), lossy),
                Err(Failure::Output(err)) if err.kind() == io::ErrorKind::StorageFull
            ));
        }
    }

    #[test]
    fn transmit_stops_at_the_first_failed_write() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        // A unit this long would take minutes if the plan were played out.
        let err = transmit(
            &mut Closed,
            "PARIS PARIS",
            Timing::uniform(MAX_UNIT_MS),
            Alphabet::Latin,
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn transmit_writes_the_header_the_morse_and_one_flash_per_symbol() {
        let mut out = Vec::new();
        transmit(&mut out, "E T", Timing::uniform(1), Alphabet::Latin).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with("Transmitting \"E T\" @ 1ms/unit\n\n. / -\n"));
        assert_eq!(out.matches('\x07').count(), 2);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn unrecognised_codes_warning_is_one_line_listing_each_once() {
        let codes = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
        assert_eq!(unrecognised_warning(&[], Alphabet::Latin), None);
        assert_eq!(
            unrecognised_warning(&codes(&["..--..--"]), Alphabet::Latin).unwrap(),
            "morse: warning: left out 1 code not recognised in the latin alphabet: \"..--..--\""
        );
        let warning =
            unrecognised_warning(&codes(&["hello", ".-.-.-.-", "hello"]), Alphabet::Greek).unwrap();
        assert_eq!(
            warning,
            "morse: warning: left out 3 codes not recognised in the greek alphabet: \
             \"hello\", \".-.-.-.-\""
        );
        // Control characters are escaped, and a long list is cut short.
        let warning = unrecognised_warning(&codes(&["\x1b[2J"]), Alphabet::Latin).unwrap();
        assert!(warning.ends_with("\"\\u{1b}[2J\""), "{warning}");
        let many: Vec<String> = (0..25).map(|n| format!("x{n}")).collect();
        let warning = unrecognised_warning(&many, Alphabet::Latin).unwrap();
        assert!(warning.contains("left out 25 codes"), "{warning}");
        assert!(
            warning.ends_with("\"x8\", \"x9\", and 15 more"),
            "{warning}"
        );
        assert!(!warning.contains('\n'));
    }

    #[test]
    fn missing_or_extra_arguments_are_usage_errors() {
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
    fn gap_unit_shorter_than_the_character_unit_is_a_usage_error() {
        let err = timing(&["transmit", "SOS", "-u", "100", "-g", "50"]).unwrap_err();
        assert!(
            err.contains("-g") && err.contains("100"),
            "unexpected error: {err}"
        );
        // Against the default unit and a --wpm character speed too.
        assert!(timing(&["transmit", "SOS", "-g", "99"]).is_err());
        assert!(timing(&["transmit", "SOS", "--wpm", "20", "--gap-unit-ms", "59"]).is_err());
        // Equal units are plain standard timing, not an error.
        assert_eq!(
            timing(&["transmit", "SOS", "-u", "80", "-g", "80"]).unwrap(),
            Timing::uniform(80)
        );
        assert!(timing(&["transmit", "SOS", "--wpm", "20", "-g", "60"]).is_ok());
    }

    #[test]
    fn wpm_outside_the_supported_range_is_a_usage_error() {
        for wpm in ["99999", "100.5", "0.99", "0.0001"] {
            let err = timing(&["transmit", "SOS", "--wpm", wpm]).unwrap_err();
            assert!(
                err.contains("--wpm") && err.contains("between 1 and 100"),
                "{wpm}: {err}"
            );
        }
        let err = timing(&[
            "transmit",
            "SOS",
            "--wpm",
            "20",
            "--farnsworth-wpm",
            "0.0001",
        ])
        .unwrap_err();
        assert!(err.contains("--farnsworth-wpm"), "unexpected error: {err}");
        // Both ends of the range are accepted.
        assert_eq!(
            timing(&["transmit", "SOS", "--wpm", "1"]).unwrap(),
            Timing::uniform(1200)
        );
        assert_eq!(
            timing(&["transmit", "SOS", "--wpm", "100"]).unwrap(),
            Timing::uniform(12)
        );
        assert!(timing(&["transmit", "SOS", "--wpm", "20", "--farnsworth-wpm", "1"]).is_ok());
    }

    #[test]
    fn options_that_set_the_same_thing_twice_are_usage_errors() {
        let err = timing(&["transmit", "SOS", "--wpm", "20", "-u", "60"]).unwrap_err();
        assert!(
            err.contains("--wpm") && err.contains("-u"),
            "unexpected error: {err}"
        );
        let err = timing(&["transmit", "SOS", "--unit-ms=60", "--wpm=20"]).unwrap_err();
        assert!(err.contains("--unit-ms"), "unexpected error: {err}");
        let err = timing(&["transmit", "SOS", "--farnsworth-wpm", "5", "-g", "300"]).unwrap_err();
        assert!(
            err.contains("--farnsworth-wpm") && err.contains("-g"),
            "unexpected error: {err}"
        );
        // One from each pair is fine.
        assert!(timing(&["transmit", "SOS", "--wpm", "20", "-g", "200"]).is_ok());
        assert!(timing(&["transmit", "SOS", "-u", "60", "--farnsworth-wpm", "5"]).is_ok());
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
