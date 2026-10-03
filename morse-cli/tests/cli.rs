//! End-to-end tests of the `morse` binary: exit codes, which stream each
//! kind of output goes to, piped input, and behaviour on a closed pipe.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_morse");

/// Run `morse` with nothing on stdin.
fn morse(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run morse")
}

/// Run `morse` with `input` piped to its stdin.
fn morse_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run morse");
    // The child may exit without reading (a usage error), so a failed
    // write here is not the test's concern.
    let _ = child.stdin.take().expect("piped stdin").write_all(input);
    child.wait_with_output().expect("wait for morse")
}

/// Run `morse` with its stdout connected to a pipe nobody reads from, as
/// in `morse ... | true`.
fn morse_with_closed_stdout(args: &[&str]) -> Output {
    let (reader, writer) = std::io::pipe().expect("create pipe");
    drop(reader);
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(Stdio::piped())
        .output()
        .expect("run morse")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

/// Assert the exit code and that nothing panicked on the way.
#[track_caller]
fn assert_exit(output: &Output, code: i32) {
    let err = stderr(output);
    assert!(!err.contains("panicked"), "panicked:\n{err}");
    assert_eq!(output.status.code(), Some(code), "stderr:\n{err}");
}

/// A directory of its own for one test's files, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("morse-cli-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch directory");
        Scratch(dir)
    }

    /// The path of `name` inside the directory, as an argument for `morse`.
    fn path(&self, name: &str) -> String {
        self.0
            .join(name)
            .to_str()
            .expect("scratch path is UTF-8")
            .to_string()
    }

    /// The names of the files in the directory, sorted.
    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("list scratch directory")
            .map(|entry| entry.expect("directory entry").file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What a WAV file says about itself, and its samples.
struct Wav {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    samples: Vec<i16>,
}

impl Wav {
    fn peak(&self) -> u16 {
        self.samples
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap_or(0)
    }
}

/// Read a 16-bit PCM WAV file, checking every header field that has only
/// one right value.
#[track_caller]
fn read_wav(path: &str) -> Wav {
    let bytes = fs::read(path).expect("read WAV file");
    assert!(bytes.len() >= 44, "{} bytes is no WAV file", bytes.len());
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32_at(4) as usize, bytes.len() - 8, "RIFF chunk size");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(16), 16, "fmt chunk size");
    assert_eq!(u16_at(20), 1, "format: integer PCM");
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(u32_at(40) as usize, bytes.len() - 44, "data chunk size");
    let wav = Wav {
        channels: u16_at(22),
        sample_rate: u32_at(24),
        bits_per_sample: u16_at(34),
        samples: bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    };
    let frame_bytes = u32::from(wav.channels) * u32::from(wav.bits_per_sample) / 8;
    assert_eq!(u32_at(28), wav.sample_rate * frame_bytes, "byte rate");
    assert_eq!(u32::from(u16_at(32)), frame_bytes, "block align");
    assert_eq!(2 * wav.samples.len(), bytes.len() - 44);
    wav
}

#[test]
fn wav_writes_a_valid_file_of_the_expected_duration() {
    let scratch = Scratch::new("wav-duration");
    let file = scratch.path("paris.wav");
    let output = morse(&["wav", "PARIS", "-o", &file, "--wpm", "20"]);
    assert_exit(&output, 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!("Wrote {file} (2.58 s, 44100 Hz, 16-bit mono)\n")
    );
    let wav = read_wav(&file);
    assert_eq!(
        (wav.channels, wav.sample_rate, wav.bits_per_sample),
        (1, 44_100, 16)
    );
    // PARIS is 43 units of 60 ms from its first tone to its last.
    assert_eq!(wav.samples.len(), 43 * 60 * 44_100 / 1000);
    // The default tone peaks at a fifth of full scale, and every tone
    // starts and ends in silence.
    assert!((6_400..=6_554).contains(&wav.peak()), "peak {}", wav.peak());
    assert_eq!(wav.samples.first(), Some(&0));
    assert_eq!(wav.samples.last(), Some(&0));
    // Nothing else is left behind.
    assert_eq!(scratch.files(), vec!["paris.wav"]);

    // Farnsworth: two dots 7 spacing units of 534 ms apart.
    let file = scratch.path("farnsworth.wav");
    let output = morse(&["wav", "E E", "--wpm=20", "--farnsworth-wpm=5", "-o", &file]);
    assert_exit(&output, 0);
    let wav = read_wav(&file);
    let ms = 60 + 7 * 534 + 60;
    assert_eq!(wav.samples.len(), (ms * 44_100 + 500) / 1000);
    // Raw unit lengths, as transmit takes them.
    let file = scratch.path("raw.wav");
    let output = morse(&["wav", "-u", "10", "-g", "20", "--output", &file, "E E"]);
    assert_exit(&output, 0);
    assert_eq!(read_wav(&file).samples.len(), (10 + 140 + 10) * 441 / 10);
}

#[test]
fn wav_tone_and_volume_shape_the_audio() {
    let scratch = Scratch::new("wav-tone");
    let loud = scratch.path("loud.wav");
    let output = morse(&["wav", "T", "-o", &loud, "--volume", "1", "--tone", "1000"]);
    assert_exit(&output, 0);
    let wav = read_wav(&loud);
    assert!(wav.peak() >= 32_700, "peak {}", wav.peak());
    // 1 kHz crosses zero 2000 times a second; the dash lasts 0.3 s.
    let crossings = wav
        .samples
        .windows(2)
        .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
        .count();
    assert!((595..=605).contains(&crossings), "{crossings} crossings");

    let silent = scratch.path("silent.wav");
    let output = morse(&["wav", "T", "-o", &silent, "--volume=0"]);
    assert_exit(&output, 0);
    let wav = read_wav(&silent);
    assert_eq!(wav.samples.len(), 300 * 441 / 10);
    assert_eq!(wav.peak(), 0);
}

#[test]
fn wav_refuses_to_overwrite_an_existing_file_unless_forced() {
    let scratch = Scratch::new("wav-overwrite");
    let file = scratch.path("kept.wav");
    fs::write(&file, b"not to be lost").expect("write existing file");

    let output = morse(&["wav", "SOS", "-o", &file, "-u", "10"]);
    assert_exit(&output, 3);
    assert_eq!(stdout(&output), "");
    let err = stderr(&output);
    assert!(
        err.contains("already exists") && err.contains("--force"),
        "{err}"
    );
    assert!(!err.contains("Usage:"), "{err}");
    assert_eq!(fs::read(&file).expect("read file"), b"not to be lost");
    assert_eq!(scratch.files(), vec!["kept.wav"]);

    // The refusal comes before standard input is read.
    let output = morse_with_stdin(&["wav", "-o", &file], b"SOS\n");
    assert_exit(&output, 3);
    assert_eq!(fs::read(&file).expect("read file"), b"not to be lost");

    let output = morse(&["wav", "SOS", "-o", &file, "-u", "10", "--force"]);
    assert_exit(&output, 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(read_wav(&file).samples.len(), 27 * 441);
    assert_eq!(scratch.files(), vec!["kept.wav"]);
}

#[test]
fn wav_that_cannot_be_written_leaves_nothing_behind() {
    let scratch = Scratch::new("wav-unwritable");
    let file = scratch.path("no-such-directory/sos.wav");
    let output = morse(&["wav", "SOS", "-o", &file, "-u", "10"]);
    assert_exit(&output, 3);
    assert_eq!(stdout(&output), "");
    let err = stderr(&output);
    assert!(err.contains("sos.wav"), "{err}");
    assert!(!err.contains("Usage:"), "{err}");
    assert_eq!(scratch.files(), Vec::<String>::new());

    // A directory in the way is not replaced, forced or not.
    let directory = scratch.path("taken");
    fs::create_dir(&directory).expect("create directory");
    for force in [&[][..], &["--force"][..]] {
        let mut args = vec!["wav", "SOS", "-o", &directory, "-u", "10"];
        args.extend(force);
        let output = morse(&args);
        assert_exit(&output, 3);
        assert_eq!(scratch.files(), vec!["taken"], "{force:?}");
        assert!(Path::new(&directory).is_dir());
    }

    // More audio than one rendering may hold: nothing is written.
    let file = scratch.path("long.wav");
    let output = morse(&["wav", "PARIS PARIS", "-o", &file, "-u", "60000"]);
    assert_exit(&output, 3);
    assert!(stderr(&output).contains("too long"), "{}", stderr(&output));
    assert_eq!(scratch.files(), vec!["taken"]);
}

#[test]
fn wav_warns_about_dropped_characters_and_strict_exits_two() {
    let scratch = Scratch::new("wav-strict");
    let file = scratch.path("ab.wav");
    let output = morse(&["wav", "A~B", "-o", &file, "-u", "10"]);
    assert_exit(&output, 0);
    assert_eq!(
        stderr(&output),
        "morse: warning: left out 1 character with no Morse code: '~' (U+007E)\n"
    );
    let lossy = read_wav(&file).samples;

    // Like encode: the file is still written, and the exit code says
    // that something was left out.
    let strict = scratch.path("strict.wav");
    let output = morse(&["wav", "A~B", "-o", &strict, "-u", "10", "--strict"]);
    assert_exit(&output, 2);
    assert!(stderr(&output).contains("'~' (U+007E)"));
    assert!(!stderr(&output).contains("Usage:"));
    assert!(stdout(&output).starts_with("Wrote "));
    assert_eq!(read_wav(&strict).samples, lossy);

    let clean = scratch.path("clean.wav");
    let output = morse(&["wav", "AB", "-o", &clean, "-u", "10", "--strict"]);
    assert_exit(&output, 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(read_wav(&clean).samples, lossy);
}

#[test]
fn wav_reads_its_text_from_stdin_like_the_other_commands() {
    let scratch = Scratch::new("wav-stdin");
    let file = scratch.path("sos.wav");
    let output = morse_with_stdin(&["wav", "-o", &file, "-u", "10"], b"SOS\n");
    assert_exit(&output, 0);
    assert_eq!(read_wav(&file).samples.len(), 27 * 441);

    let missing = scratch.path("missing.wav");
    let output = morse(&["wav", "-o", &missing]);
    assert_exit(&output, 1);
    assert!(stderr(&output).contains("missing text"));
    assert_eq!(scratch.files(), vec!["sos.wav"]);
}

#[test]
fn wav_options_are_validated_before_anything_is_written() {
    let scratch = Scratch::new("wav-usage");
    let file = scratch.path("never.wav");
    for (args, message) in [
        (&["--tone", "high"][..], "--tone"),
        (&["--tone", "0"][..], "--tone"),
        (&["--tone", "-600"][..], "--tone"),
        (&["--tone", "nan"][..], "--tone"),
        (&["--tone", "inf"][..], "--tone"),
        (&["--tone", "19"][..], "between 20 and 20000"),
        (&["--tone=20001"][..], "between 20 and 20000"),
        (&["--volume", "loud"][..], "--volume"),
        (&["--volume", "1.5"][..], "between 0 and 1"),
        (&["--volume", "-0.5"][..], "--volume"),
        (&["--volume", "nan"][..], "--volume"),
        (&["--volume"][..], "--volume expects a value"),
        // The timing options are checked exactly as for transmit.
        (&["--wpm", "fast"][..], "--wpm"),
        (&["--wpm", "20", "-u", "60"][..], "-u"),
        (&["-u", "100", "-g", "50"][..], "-g"),
        (
            &["--wpm", "10", "--farnsworth-wpm", "20"][..],
            "--farnsworth-wpm",
        ),
        (&["-a", "klingon"][..], "unknown alphabet"),
    ] {
        let mut line = vec!["wav", "SOS", "-o", &file];
        line.extend(args);
        let output = morse(&line);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{args:?}");
        let err = stderr(&output);
        assert!(err.contains(message), "{args:?}: {err}");
        assert!(err.contains("Usage:"), "{args:?}: {err}");
    }
    // The output file is required, and "-" does not mean standard output.
    for (line, message) in [
        (&["wav", "SOS"][..], "-o"),
        (&["wav", "SOS", "-o"][..], "-o expects a value"),
        (&["wav", "SOS", "-o", "-"][..], "standard output"),
        (&["wav", "SOS", "--output=-"][..], "standard output"),
    ] {
        let output = morse(line);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{line:?}");
        assert!(stderr(&output).contains(message), "{line:?}");
    }
    // Options that only wav has are not silently ignored elsewhere.
    for line in [
        &["encode", "SOS", "-o", &file][..],
        &["transmit", "E", "-u", "1", "--output", &file][..],
        &["encode", "SOS", "--force"][..],
        &["decode", "...", "--force"][..],
    ] {
        let output = morse(line);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{line:?}");
        assert!(stderr(&output).contains("wav only"), "{line:?}");
    }
    assert_eq!(scratch.files(), Vec::<String>::new());
}

#[test]
fn encode_and_decode_print_the_result_on_stdout_and_exit_zero() {
    let output = morse(&["encode", "SOS"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "... --- ...\n");
    assert_eq!(stderr(&output), "");

    let output = morse(&["decode", "... --- ..."]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS\n");
    assert_eq!(stderr(&output), "");

    let output = morse(&["decode", ".--. .-. .. .-- . -", "-a", "cyrillic"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "ПРИВЕТ\n");
}

#[test]
fn ukrainian_text_is_detected_and_uk_selects_the_ukrainian_alphabet() {
    let output = morse(&["encode", "ПРИВІТ"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), ".--. .-. -.-- .-- .. -\n");
    assert_eq!(stderr(&output), "");

    for name in ["uk", "ukrainian", "українська"] {
        let output = morse(&["decode", "-a", name, ".--. .-. -.-- .-- .. -"]);
        assert_exit(&output, 0);
        assert_eq!(stdout(&output), "ПРИВІТ\n", "{name}");
        assert_eq!(stderr(&output), "");
    }
    let output = morse(&["decode", "-a", "uk", "..-.. / .. / .---. / --."]);
    assert_eq!(stdout(&output), "Є І Ї Г\n");

    // Text with no Ukrainian-only letter needs the option.
    let output = morse(&["encode", "-a", "uk", "МИР"]);
    assert_eq!(stdout(&output), "-- -.-- .-.\n");
    let output = morse_with_stdin(&["encode"], "привіт\n".as_bytes());
    assert_eq!(stdout(&output), ".--. .-. -.-- .-- .. -\n");
}

#[test]
fn russian_text_and_the_russian_alphabet_keep_their_codes() {
    let output = morse(&["encode", "-a", "ru", "ПРИВЕТ"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), ".--. .-. .. .-- . -\n");
    assert_eq!(stderr(&output), "");

    for args in [
        &["encode", "привет"][..],
        &["encode", "ПРИВЕТ", "-a", "cyrillic"][..],
        &["encode", "ПРИВЕТ", "-a", "russian"][..],
    ] {
        assert_eq!(stdout(&morse(args)), ".--. .-. .. .-- . -\n", "{args:?}");
    }
    // Asked for by name, the Russian table still sends Ukrainian letters.
    let output = morse(&["encode", "-a", "ru", "ПРИВІТ"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), ".--. .-. .. .-- .. -\n");
    assert_eq!(stderr(&output), "");
    let output = morse(&["decode", "-a", "ru", ".. / ..-.. / -.--"]);
    assert_eq!(stdout(&output), "И Э Ы\n");
}

#[test]
fn russian_only_letters_are_left_out_of_ukrainian_and_named() {
    let output = morse(&["encode", "-a", "uk", "МЫ", "--strict"]);
    assert_exit(&output, 2);
    assert_eq!(stdout(&output), "--\n");
    assert_eq!(
        stderr(&output),
        "morse: warning: left out 1 character with no Morse code: 'Ы' (U+042B)\n"
    );
    let output = morse(&["decode", "-a", "uk", "-- ..--.."]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "М?\n");
    let output = morse(&["decode", "-a", "uk", "-- ..--..--"]);
    assert!(
        stderr(&output).contains("ukrainian alphabet"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn transmit_and_wav_use_the_ukrainian_alphabet() {
    let output = morse(&["transmit", "привіт", "-u", "1"]);
    assert_exit(&output, 0);
    assert!(
        stdout(&output).contains("\n.--. .-. -.-- .-- .. -\n"),
        "{}",
        stdout(&output)
    );
    let output = morse(&["transmit", "-a", "uk", "И", "-u", "1"]);
    assert_exit(&output, 0);
    assert!(stdout(&output).contains("\n-.--\n"));
    // Four tones: dash dot dash dash.
    assert_eq!(stdout(&output).matches('\x07').count(), 4);

    // И is `-.--` (13 units) in Ukrainian and `..` (3 units) in Russian.
    let scratch = Scratch::new("wav-ukrainian");
    let (uk, ru, detected) = (
        scratch.path("uk.wav"),
        scratch.path("ru.wav"),
        scratch.path("detected.wav"),
    );
    assert_exit(&morse(&["wav", "И", "-a", "uk", "-o", &uk, "-u", "10"]), 0);
    assert_exit(&morse(&["wav", "И", "-a", "ru", "-o", &ru, "-u", "10"]), 0);
    assert_eq!(read_wav(&uk).samples.len(), 13 * 441);
    assert_eq!(read_wav(&ru).samples.len(), 3 * 441);
    // ІИ: `..` and `-.--` with a letter gap between them, 19 units.
    let output = morse(&["wav", "іи", "-o", &detected, "-u", "10"]);
    assert_exit(&output, 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(read_wav(&detected).samples.len(), 19 * 441);
}

#[test]
fn help_version_and_alphabets_go_to_stdout() {
    let output = morse(&["--help"]);
    assert_exit(&output, 0);
    assert!(stdout(&output).contains("Usage:"));
    assert_eq!(stderr(&output), "");

    let output = morse(&["--version"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        format!("morse {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(stderr(&output), "");

    let output = morse(&["alphabets"]);
    assert_exit(&output, 0);
    assert!(stdout(&output).contains("cyrillic"));
    assert!(stdout(&output).contains("ukrainian  Українська\n"));
    assert_eq!(stdout(&output).lines().count(), 9);
    assert_eq!(stderr(&output), "");
}

#[test]
fn no_arguments_prints_usage_on_stderr_and_exits_one() {
    let output = morse(&[]);
    assert_exit(&output, 1);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("Usage:"));
}

#[test]
fn usage_errors_explain_themselves_on_stderr_and_exit_one() {
    for (args, message) in [
        (&["frobnicate", "SOS"][..], "unknown command"),
        (&["encode", "HELLO", "WORLD"][..], "unexpected argument"),
        (&["encode", "SOS", "-a", "klingon"][..], "unknown alphabet"),
        (&["transmit", "SOS", "--wpm", "fast"][..], "--wpm"),
    ] {
        let output = morse(args);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{args:?}");
        let err = stderr(&output);
        assert!(err.contains(message), "{args:?}: {err}");
        assert!(err.contains("Usage:"), "{args:?}: {err}");
    }
}

#[test]
fn usage_names_the_program_by_its_file_name_not_its_path() {
    let name = Path::new(BIN)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("binary has a UTF-8 file name");
    let directory = Path::new(BIN)
        .parent()
        .and_then(|dir| dir.to_str())
        .expect("binary has a UTF-8 parent directory");
    for output in [morse(&[]), morse(&["--help"])] {
        let usage = stdout(&output) + &stderr(&output);
        assert!(usage.contains(&format!("  {name} encode ")), "{usage}");
        assert!(!usage.contains(directory), "{usage}");
    }
}

#[test]
fn encode_warns_on_stderr_and_still_exits_zero() {
    let output = morse(&["encode", "A~B"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), ".- -...\n");
    assert_eq!(
        stderr(&output),
        "morse: warning: left out 1 character with no Morse code: '~' (U+007E)\n"
    );
}

#[test]
fn decode_warns_about_codes_it_does_not_recognise() {
    let output = morse(&["decode", "... --- ... ..--..--"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS\n");
    assert_eq!(
        stderr(&output),
        "morse: warning: left out 1 code not recognised in the latin alphabet: \"..--..--\"\n"
    );

    // Text given to decode by mistake is all unrecognised.
    let output = morse(&["decode", "hello"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "\n");
    assert!(stderr(&output).contains("\"hello\""), "{}", stderr(&output));

    // The warning names the alphabet the code was looked up in.
    let output = morse(&["decode", ".-.-.-.-", "-a", "greek"]);
    assert_exit(&output, 0);
    assert!(
        stderr(&output).contains("greek alphabet"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn decode_reads_look_alike_symbols() {
    let output = morse(&["decode", "··· −−− ··· | … ——— …"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS SOS\n");
    assert_eq!(stderr(&output), "");
}

#[test]
fn strict_exits_two_when_something_was_left_out() {
    let output = morse(&["encode", "--strict", "A~B"]);
    assert_exit(&output, 2);
    // The translation and the warning are still printed.
    assert_eq!(stdout(&output), ".- -...\n");
    assert!(stderr(&output).contains("'~' (U+007E)"));
    assert!(!stderr(&output).contains("Usage:"));

    let output = morse(&["decode", "... --- ... ..--..--", "--strict"]);
    assert_exit(&output, 2);
    assert_eq!(stdout(&output), "SOS\n");
    assert!(stderr(&output).contains("\"..--..--\""));
    assert!(!stderr(&output).contains("Usage:"));
}

#[test]
fn strict_exits_zero_when_nothing_was_left_out() {
    let output = morse(&["encode", "SOS", "--strict"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "... --- ...\n");
    assert_eq!(stderr(&output), "");

    let output = morse(&["--strict", "decode", "... --- ..."]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS\n");
    assert_eq!(stderr(&output), "");
}

#[test]
fn strict_is_a_usage_error_for_transmit() {
    let output = morse(&["transmit", "E", "-u", "1", "--strict"]);
    assert_exit(&output, 1);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("--strict"));
}

#[test]
fn text_is_read_from_stdin_when_no_text_argument_is_given() {
    let output = morse_with_stdin(&["encode"], b"SOS\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "... --- ...\n");
    assert_eq!(stderr(&output), "");

    let output = morse_with_stdin(&["decode"], b"... --- ...\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS\n");

    // Options still apply, and lines are words.
    let output = morse_with_stdin(&["-a", "cyrillic", "decode"], b".-\r\n-...\r\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "АБ\n");
    let output = morse_with_stdin(&["encode", "--strict"], "HI\nTHERE ~\n".as_bytes());
    assert_exit(&output, 2);
    assert_eq!(stdout(&output), ".... .. / - .... . .-. .\n");
}

#[test]
fn a_byte_order_mark_on_stdin_is_not_part_of_the_text() {
    let output = morse_with_stdin(&["decode", "--strict"], b"\xef\xbb\xbf... --- ...\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "SOS\n");
    assert_eq!(stderr(&output), "");

    let output = morse_with_stdin(&["encode", "--strict"], b"\xef\xbb\xbfSOS\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "... --- ...\n");
    assert_eq!(stderr(&output), "");
}

#[test]
fn an_accent_with_no_code_is_left_out_and_its_letter_still_sent() {
    let output = morse(&["encode", "ÊTRE"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), ". - .-. .\n");
    assert_eq!(
        stderr(&output),
        "morse: warning: left out 1 character with no Morse code: '\\u{302}' (U+0302)\n"
    );
    // The accent was lost, which --strict counts.
    assert_exit(&morse(&["encode", "--strict", "ÊTRE"]), 2);

    // Typographic punctuation and Greek ΐ lose nothing.
    let output = morse(&["encode", "--strict", "DON\u{2019}T"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "-.. --- -. .----. -\n");
    assert_eq!(stderr(&output), "");
    let output = morse(&["encode", "--strict", "ταΐζω"]);
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "- .- .. --.. .--\n");
    assert_eq!(stderr(&output), "");
}

#[test]
fn a_text_argument_wins_over_stdin() {
    let output = morse_with_stdin(&["encode", "SOS"], b"IGNORED\n");
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "... --- ...\n");
}

#[test]
fn transmit_reads_stdin_and_shows_the_text_without_its_newline() {
    let output = morse_with_stdin(&["transmit", "-u", "1"], b"E\n");
    assert_exit(&output, 0);
    let out = stdout(&output);
    assert!(
        out.starts_with("Transmitting \"E\" @ 1ms/unit\n"),
        "{out:?}"
    );
}

#[test]
fn empty_stdin_is_the_missing_text_usage_error() {
    for command in ["encode", "decode", "transmit"] {
        let output = morse(&[command]);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{command}");
        let err = stderr(&output);
        assert!(err.contains("missing text"), "{command}: {err}");
        assert!(err.contains("Usage:"), "{command}: {err}");
    }
}

#[test]
fn stdin_that_is_not_utf8_is_an_error_not_a_panic() {
    let output = morse_with_stdin(&["encode"], b"SOS \xff\xfe\n");
    assert_exit(&output, 3);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("standard input"));
}

#[test]
fn closed_stdout_ends_quietly_with_exit_zero() {
    for args in [
        &["encode", "SOS"][..],
        &["decode", "... --- ..."][..],
        &["--help"][..],
        &["--version"][..],
        &["alphabets"][..],
        &["transmit", "E E E E E E", "-u", "1"][..],
    ] {
        let output = morse_with_closed_stdout(args);
        let err = stderr(&output);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {err}");
        assert_eq!(err, "", "{args:?}");
    }
}

#[test]
fn closed_stdout_still_delivers_the_warning_and_the_strict_exit_code() {
    let output = morse_with_closed_stdout(&["encode", "A~B", "--strict"]);
    assert_exit(&output, 2);
    assert!(stderr(&output).contains("'~' (U+007E)"));
}

#[test]
fn closed_stderr_does_not_turn_a_usage_error_into_a_panic() {
    let (reader, writer) = std::io::pipe().expect("create pipe");
    drop(reader);
    let output = Command::new(BIN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(writer)
        .output()
        .expect("run morse");
    assert_eq!(output.status.code(), Some(1));

    let (reader, writer) = std::io::pipe().expect("create pipe");
    drop(reader);
    let output = Command::new(BIN)
        .args(["encode", "A~B"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(writer)
        .output()
        .expect("run morse");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), ".- -...\n");
}

#[cfg(unix)]
#[test]
fn argument_that_is_not_utf8_is_a_usage_error_not_a_panic() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    for args in [
        &[OsStr::new("encode"), OsStr::from_bytes(b"SOS\xff")][..],
        &[OsStr::from_bytes(b"\xfe"), OsStr::new("SOS")][..],
        &[
            OsStr::new("encode"),
            OsStr::new("SOS"),
            OsStr::new("-a"),
            OsStr::from_bytes(b"lat\xffin"),
        ][..],
    ] {
        let output = Command::new(BIN)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("run morse");
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{args:?}");
        let err = stderr(&output);
        assert!(err.contains("not valid UTF-8"), "{args:?}: {err}");
        assert!(err.contains("Usage:"), "{args:?}: {err}");
    }
}

#[test]
fn transmit_timing_options_are_validated_before_anything_is_sent() {
    for (args, message) in [
        // A gap unit shorter than the character unit.
        (&["transmit", "E", "-u", "100", "-g", "50"][..], "-g"),
        // Words per minute outside the supported range.
        (&["transmit", "E", "--wpm", "99999"][..], "--wpm"),
        (&["transmit", "E", "--wpm", "0.0001"][..], "--wpm"),
        // Two options that set the same thing.
        (&["transmit", "E", "--wpm", "20", "-u", "60"][..], "-u"),
        (
            &["transmit", "E", "--farnsworth-wpm", "5", "-g", "300"][..],
            "-g",
        ),
    ] {
        let output = morse(args);
        assert_exit(&output, 1);
        assert_eq!(stdout(&output), "", "{args:?}");
        let err = stderr(&output);
        assert!(err.contains(message), "{args:?}: {err}");
        assert!(err.contains("Usage:"), "{args:?}: {err}");
    }
}

#[test]
fn transmit_prints_the_morse_and_finishes() {
    let output = morse(&["transmit", "E E", "-u", "1", "-g", "2"]);
    assert_exit(&output, 0);
    let out = stdout(&output);
    assert!(
        out.starts_with("Transmitting \"E E\" @ 1ms/unit (chars), 2ms/unit (gaps, Farnsworth)\n"),
        "{out:?}"
    );
    assert!(out.contains("\n. / .\n"), "{out:?}");
    assert_eq!(stderr(&output), "");
}
