//! End-to-end tests of the `morse` binary: exit codes, which stream each
//! kind of output goes to, piped input, and behaviour on a closed pipe.

use std::io::Write;
use std::path::Path;
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
    assert_eq!(stdout(&output).lines().count(), 8);
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
