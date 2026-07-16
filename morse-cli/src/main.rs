use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use morse_core::{build_signal_plan, decode, encode, Signal, UNIT_MS};

fn usage(prog: &str) -> String {
    format!(
        "Morse Code Translator\n\n\
         Usage:\n  \
         {prog} encode <text>            Text -> Morse\n  \
         {prog} decode <morse>           Morse -> text (use / between words)\n  \
         {prog} transmit <text> [-u ms]  Flash + beep the Morse in your terminal\n\n\
         Examples:\n  \
         {prog} encode \"SOS\"\n  \
         {prog} decode \"... --- ...\"\n  \
         {prog} transmit \"HELLO WORLD\" -u 80\n"
    )
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
            let unit_ms = args
                .iter()
                .position(|a| a == "-u" || a == "--unit-ms")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(UNIT_MS);
            transmit(arg, unit_ms);
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{}", usage(prog));
            ExitCode::FAILURE
        }
    }
}

/// Play a text message out as visual flashes + terminal-bell beeps,
/// timed according to standard Morse ratios (dot=1u, dash=3u, gaps
/// 1u/3u/7u for symbol/letter/word).
fn transmit(text: &str, unit_ms: u64) {
    println!("Transmitting \"{text}\" @ {unit_ms}ms/unit\n");
    println!("{}", encode(text));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for signal in build_signal_plan(text) {
        if signal.is_tone() {
            // \x07 = terminal bell (audible beep in most terminal apps).
            // \x1b[7m..\x1b[0m briefly inverts the colors for a visual flash.
            print!("\x07\x1b[7m  \x1b[0m");
            out.flush().unwrap();
            sleep(Duration::from_millis(signal.duration_ms(unit_ms)));
            print!("\r    \r");
            out.flush().unwrap();
            // 1-unit gap after every symbol.
            sleep(Duration::from_millis(unit_ms));
        } else {
            if matches!(signal, Signal::WordGap) {
                println!();
            }
            sleep(Duration::from_millis(signal.duration_ms(unit_ms)));
        }
    }
    println!();
}
