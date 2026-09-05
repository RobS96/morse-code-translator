use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use morse_core::{Signal, Timing, UNIT_MS, build_signal_plan, decode, encode, wpm_to_unit_ms};

fn usage(prog: &str) -> String {
    format!(
        "Morse Code Translator\n\n\
         Usage:\n  \
         {prog} encode <text>            Text -> Morse (supports <SK>, <AR>, ... prosigns)\n  \
         {prog} decode <morse>           Morse -> text (use / between words)\n  \
         {prog} transmit <text> [opts]   Flash + beep the Morse in your terminal\n\n\
         Transmit options:\n  \
         -u, --unit-ms <MS>          Character unit length in ms (default {UNIT_MS})\n  \
         -g, --gap-unit-ms <MS>      Letter/word gap unit length in ms (default: same as -u)\n  \
         --wpm <N>                   Set character speed from words-per-minute\n  \
         --farnsworth-wpm <N>        Set gap speed from words-per-minute (Farnsworth timing)\n\n\
         Examples:\n  \
         {prog} encode \"SOS\"\n  \
         {prog} encode \"CQ CQ <AR>\"\n  \
         {prog} decode \"... --- ...\"\n  \
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

/// Resolve transmit timing from CLI flags: `--wpm`/`--farnsworth-wpm` take
/// precedence when given, falling back to raw `-u`/`-g` millisecond values,
/// and finally to standard (non-Farnsworth) timing at [`UNIT_MS`].
fn resolve_timing(args: &[String]) -> Timing {
    let char_unit_ms = flag_value(args, &["--wpm"])
        .and_then(|v| v.parse::<f64>().ok())
        .map(wpm_to_unit_ms)
        .or_else(|| flag_value(args, &["-u", "--unit-ms"]).and_then(|v| v.parse().ok()))
        .unwrap_or(UNIT_MS);

    let gap_unit_ms = flag_value(args, &["--farnsworth-wpm"])
        .and_then(|v| v.parse::<f64>().ok())
        .map(wpm_to_unit_ms)
        .or_else(|| flag_value(args, &["-g", "--gap-unit-ms"]).and_then(|v| v.parse().ok()))
        .unwrap_or(char_unit_ms);

    Timing {
        char_unit_ms,
        gap_unit_ms,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let prog = args.first().map(String::as_str).unwrap_or("morse");

    let (Some(cmd), Some(arg)) = (args.get(1), args.get(2)) else {
        eprint!("{}", usage(prog));
        return ExitCode::FAILURE;
    };

    match cmd.as_str() {
        "encode" => {
            println!("{}", encode(arg));
            ExitCode::SUCCESS
        }
        "decode" => {
            println!("{}", decode(arg));
            ExitCode::SUCCESS
        }
        "transmit" => {
            transmit(arg, resolve_timing(&args));
            ExitCode::SUCCESS
        }
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
fn transmit(text: &str, timing: Timing) {
    if timing.gap_unit_ms == timing.char_unit_ms {
        println!("Transmitting \"{text}\" @ {}ms/unit\n", timing.char_unit_ms);
    } else {
        println!(
            "Transmitting \"{text}\" @ {}ms/unit (chars), {}ms/unit (gaps, Farnsworth)\n",
            timing.char_unit_ms, timing.gap_unit_ms
        );
    }
    println!("{}", encode(text));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for signal in build_signal_plan(text) {
        if signal.is_tone() {
            // \x07 = terminal bell (audible beep in most terminal apps).
            // \x1b[7m..\x1b[0m briefly inverts the colors for a visual flash.
            print!("\x07\x1b[7m  \x1b[0m");
            out.flush().unwrap();
            sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
            print!("\r    \r");
            out.flush().unwrap();
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
