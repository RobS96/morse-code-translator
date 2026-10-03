# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- A release is published only from a signed, annotated tag that GitHub
  verifies, on a commit that is on `main`, and only when the tag, the crate
  version, the version `morse --version` prints and the changelog heading
  agree. A tag that fails any of these publishes nothing.
- The SBOMs in each archive are generated in a job of their own, so the
  jobs that build the release binaries run nothing but the toolchain and
  the locked dependency tree. The release archives are built on named
  runner images (Ubuntu 24.04, macOS 26, Windows Server 2025).
- The documented `gh attestation verify` command names the release
  workflow (`--signer-workflow`), so an attestation from any other
  workflow in the repository is not accepted.

## [0.5.0] - 2026-10-02

### Added

- Ukrainian Morse alphabet: `Alphabet::Ukrainian` in `morse-core`,
  `-a ukrainian` (or `uk`, `українська`) in `morse-cli`, listed by
  `morse alphabets`, and **Українська** in the `morse-gui` alphabet picker.
  И is `-.--`, І `..`, Є `..-..` and Ї `.---.`, and each decodes to itself;
  Ґ is sent with Г's code (`--.`) and reads back as Г. Russian Ы, Э, Ъ and
  Ё have no code in it and are reported as left out.

### Changed

- `-a uk` (`Alphabet::from_name("uk")`) selects the Ukrainian alphabet. It
  used to select the Russian table, which sent И as `..` and decoded `..`
  as И, `-.--` as Ы and `..-..` as Э. `-a ru`, `russian`, `cyrillic` and
  `bg` select the Russian table as before, and it still sends І, Є and Ї.
- Cyrillic text containing І, Ї, Є or Ґ and none of Ы, Э, Ъ, Ё is detected
  as Ukrainian (`Alphabet::detect`, so `encode`, `transmit`, `wav` and the
  GUI when no alphabet is chosen). Its И is now sent as `-.--`, not `..`,
  and its Ґ as `--.` instead of being left out: `morse encode "ПРИВІТ"`
  gives `.--. .-. -.-- .-- .. -`, where it gave `.--. .-. .. .-- .. -`.
  All other Cyrillic text is detected and sent as before; `-a ru` sends
  Ukrainian letters with the Russian table.
- `morse_core::Alphabet` has a ninth variant and `Alphabet::ALL` is
  `[Alphabet; 9]`: code that matches on `Alphabet` exhaustively, or names
  the array's length, needs updating.

## [0.4.0] - 2026-10-02

### Added

- `morse wav [text] -o <file>` writes the transmission as a WAV file
  (16-bit mono PCM, 44100 Hz), with the timing options of `transmit` and
  the same checks on them, `--tone <Hz>` (20 to 20000, default 600) and
  `--volume <0-1>` (default 0.2). Text comes from the argument or standard
  input; dropped characters are warned about and `--strict` applies, as
  for `encode`. An existing file is not replaced without `--force`, and
  the file is written by way of a temporary file beside it, so a failed
  run leaves nothing partial behind. `-o -` is not standard output.
- `morse_core::build_schedule` lays a signal plan out in time as
  `ScheduleStep { kind, duration_ms }` (`StepKind::Tone`, `SymbolGap`,
  `LetterGap`, `WordGap`), with `schedule_duration_ms` for the total. It
  is the one place the unit of silence between two symbols of a letter is
  added; the CLI and the GUI each used to add it themselves.
- `morse_core::render_samples` / `render_schedule` render a transmission
  as mono `f32` PCM for a `Tone { frequency_hz, sample_rate, volume,
  ramp_ms }`: sample-exact step lengths taken from cumulative time, and a
  raised-cosine attack and release on every tone so the keying does not
  click. Out-of-range or non-finite parameters, and a transmission over
  `MAX_RENDER_SAMPLES`, are a `RenderError`. `morse_core::write_wav`
  writes samples as a 16-bit mono WAV stream to any `io::Write`.
- `morse-gui`: a **Stop** button, **Tone** (300 to 1200 Hz) and **Volume**
  sliders, and a status-line list of what the translation left out
  (characters with no Morse code, or codes that were not recognised), in
  all 12 interface languages.
- `morse_core::decode_lossy_report` / `decode_lossy_report_in` return the
  text together with the codes that were dropped for not being recognised
  (`DecodeReport { text, skipped }`). `morse decode` prints a one-line
  warning on stderr naming them; it used to drop them silently.
- `morse-cli`: `--strict` for `encode` and `decode` exits with code 2 when
  anything was left out. The translation and the warning are still
  printed.
- `morse-cli` reads the text from standard input when no text argument is
  given and standard input is not a terminal (`echo SOS | morse encode`).
- `decode` reads common look-alike characters: `·` `•` as a dot, `−` `–`
  `—` `_` as a dash, `…` as three dots and `|` as the word separator.
- Accented Latin decodes: every extension code that nothing else uses
  gives one letter (Ä, Å, Ç, CH, Ð, É, È, Ĝ, Ĵ, Ñ, Ö, Ś, Þ, Ü, Ź, Ż).
  `MÜNCHEN` now round-trips; it used to come back as `MNCHEN`.
- Decoding into a non-Latin alphabet gives the Latin letter for a code
  the alphabet has no letter of its own for (J, U, V in Greek; F, V, X, Y
  in Hebrew), instead of dropping it. Native letters are unaffected.
- Prosigns `<SOS>` and `<HH>` (error, eight dots), and `<VE>`, `<KA>`,
  `<VA>` as other spellings of `<SN>`, `<CT>`, `<SK>` on encode.
- Per ITU-R M.1677-1, `×` is sent as X and `%` as `0/0`, joined to a
  number before it by a hyphen (`2%` is `2-0/0`). Arabic ة is sent as ه.
  The zero-width non-joiner (U+200C) is ignored instead of being reported
  as a dropped character.

### Changed

- `morse-gui` renders the whole transmission once and plays it as one
  buffer, with shaped tone edges instead of hard on/off keying, and the
  lamp follows the same schedule against the clock instead of a chain of
  sleeps, so the rhythm no longer drifts. With no sound output available
  the lamp still runs and the status line says so (the failure used to be
  silent).
- `morse-gui`: the result box scrolls once it is a few lines tall, and
  the Copy button sits beside the "Result" label. The window opens 100
  pixels taller to make room for the new controls.
- `morse transmit` takes its timing from `build_schedule`: it no longer
  waits one more unit after the last symbol.
- `morse-cli`: `-o`/`--output` and `--force` with a command other than
  `wav` are usage errors.
- `build_signal_plan` / `build_signal_plan_in` no longer end with a
  `Signal::LetterGap`: the plan stops at the last dot or dash, so a
  transmission no longer waits out a letter gap (seconds, at Farnsworth
  speeds) after the final letter.
- `morse-cli` usage errors (exit code 1) that used to be accepted: a
  `-g` gap unit shorter than the character unit; `--wpm` or
  `--farnsworth-wpm` outside 1 to 100 WPM; `--wpm` together with `-u`,
  and `--farnsworth-wpm` together with `-g` (one of each pair used to be
  ignored).
- `morse-cli` exits with code 3 when standard input cannot be read or the
  output cannot be written. The usage text names the program by its file
  name rather than the path it was run by, and lists the exit codes.
- `Alphabet::detect` no longer picks Japanese for text whose only
  Japanese-looking characters are CJK punctuation (U+3000 to U+303F, such
  as an ideographic space), so Latin text containing one keeps its Latin
  bracket codes. Kana still select Japanese; `、。` alone need
  `-a japanese`.

### Fixed

- `morse-gui` no longer prints rodio's "Dropping DeviceSink" notice to
  stderr after every transmission.
- `morse-gui`: the Transmit button can no longer stay disabled if the
  transmit thread ends abnormally.
- `morse-gui`: a long result no longer pushes the controls below it out
  of the window.
- `morse-gui`: at the smallest window size (420×520) the content is about
  130 px taller than the window, so the lamp and the status line were cut
  off. The window's content now scrolls when it does not fit. It fits
  without scrolling at the size the window opens at, in all 12 languages
  (checked by laying the interface out without a window in the tests).
- `morse-gui` release builds for Windows no longer open a console window.
- `morse-cli` no longer panics (exit code 101) when its output is closed
  early, as in `morse encode "SOS" | true` or `morse transmit ... | head
  -1`: it stops quietly with exit code 0. A closed stderr no longer
  panics either.
- `morse-cli` no longer panics on an argument that is not valid UTF-8; it
  is a usage error.
- A prosign inside a word is sent fused (`SOS<SK>`), so what `decode`
  writes for a prosign that follows a letter encodes back to the same
  Morse. It used to be sent letter by letter with the brackets dropped.
- `decode` no longer leaves a double space where a word is empty or
  wholly unrecognised: `decode(".- // -...")` is `A B`.

### Removed

- `morse-gui` no longer depends on `winapi` on Windows. It was a
  workaround for `eframe` 0.24, which `eframe` 0.36 (on `windows-sys`)
  does not need; `winapi` and its two `*-pc-windows-gnu` import-library
  crates are gone from `Cargo.lock`.

## [0.3.0] - 2026-09-29

### Added

- Non-Latin Morse alphabets: Cyrillic, Greek, Hebrew, Arabic, Persian,
  Japanese (Wabun) and Korean, plus accented-Latin extensions on encode.
  New `morse_core::Alphabet` (with `detect`, `from_name`, `native_name`)
  and `encode_in` / `decode_in` / `build_signal_plan_in`. `encode` and
  `build_signal_plan` now auto-detect the alphabet; `decode` stays Latin.
  `morse-cli` gains `-a/--alphabet` and a `morse alphabets` command.
- `morse-gui` interface translated into 12 languages (en, es, fr, de, it,
  pt, ru, uk, el, ja, ko, zh-Hans) with a language picker, an alphabet
  picker, and system-font fallbacks for scripts egui doesn't bundle.
- Prosigns are now recognised in any case (`<sk>` as well as `<SK>`).
- Farnsworth timing now follows the ARRL formula, so `--wpm 20
  --farnsworth-wpm 5` really sends at 5 WPM overall. The spacing unit was
  `1200 / effective_wpm`, which gave about 10.9 WPM at 20/5. An effective
  speed at or above the character speed now gives standard timing
  instead of gaps shorter than standard.
- `morse_core::encode_lossy_report` / `encode_lossy_report_in` return the
  Morse together with the input characters that were dropped for having no
  code (`EncodeReport { morse, skipped }`). `morse encode` and `morse
  transmit` print a one-line warning on stderr naming them; the exit code
  is unchanged.
- `morse_core::normalize_input`, applied to all encoder input: decomposed
  Cyrillic Й/Ё/Ї, kana with combining dakuten/handakuten, half-width
  katakana and Hangul conjoining jamo now encode like their usual forms.
  Half-width katakana is detected as Japanese. Other combining marks are
  not covered; see the README.
- `morse-cli`: `--help`/`-h` and `--version`/`-V`; `--name=value` for long
  options.
- Release archives include a CycloneDX SBOM per crate, and releases carry
  per-platform `SHA256SUMS-<platform>.txt` files and GitHub build
  provenance attestations.
- `rust-version` is declared: 1.88 for `morse-core` and `morse-cli`, 1.95
  for `morse-gui`.

- Farnsworth timing: characters transmit at one WPM speed while
  letter/word gaps stretch to a slower "effective" WPM — the method
  recommended by the ARRL and CW Academy for learning Morse. New
  `morse_core::Timing` type, `Signal::duration_ms_timed`, and
  `wpm_to_unit_ms`; exposed as `--wpm`/`--farnsworth-wpm` in `morse-cli`
  and a speed slider + Farnsworth toggle in `morse-gui`.
- Procedural sign ("prosign") support — `<AR>`, `<SK>`, `<BT>`, `<KN>`,
  `<AS>`, `<CT>`, `<BK>`, `<SN>` — encoded as a single fused character with
  no inter-letter gap, matching real on-air Morse convention. `morse-gui`
  adds one-click prosign insert buttons in Encode mode.
- `morse-gui`: copy-to-clipboard button for the result field, WPM-based
  speed controls (replacing raw millisecond-per-unit), redesigned layout.

### Changed

- `morse-cli` options may come before or after the text. Previously the
  second argument was always taken as the text, so `morse decode -a
  cyrillic ".-"` decoded `-a`.
- `morse-cli` usage errors (exit code 1) that used to be accepted: an
  option with its value missing, a misspelt option, more than one text
  argument, an argument after `alphabets`, and a `--farnsworth-wpm` above
  the character speed (previously sent with standard timing).
- Release builds use `codegen-units = 1`, strip symbols and keep integer
  overflow checks on.
- The committed `sbom.json` files are removed; SBOMs are generated at
  release time instead.
- `morse-core`'s letter tables build once via `std::sync::LazyLock`
  instead of being reconstructed into a new `HashMap` on every
  `encode`/`decode`/`build_signal_plan` call.

### Fixed

- Decoding Hebrew restores the final letter form when the word ends in
  punctuation, so `שלום.` round-trips instead of coming back as `שלומ.`.
- A word with no encodable characters no longer produces an empty Morse
  word or a doubled word gap: `encode("A ~ B")` is `.- / -...` (was
  `.- /  / -...`), and the signal plan holds one word gap there, with none
  leading or trailing.
- `morse --help` continuation lines are aligned under their option text.

### Security

- CI pins the versions of `cargo-deny` and `cargo-vet` it installs, builds
  and tests with `--locked`, checks out without persisting credentials and
  sets job timeouts. Release permissions for provenance (`id-token`,
  `attestations`) are scoped to the release job.
- `wpm_to_unit_ms` now clamps its result to a `MIN_UNIT_MS..=MAX_UNIT_MS`
  range: a zero, negative, NaN, or infinite WPM previously divided out to
  `+inf`, which a saturating float-to-int cast turned into a
  `u64::MAX`-millisecond ("effectively forever") sleep on `transmit`.
- `Signal::duration_ms_timed` uses saturating multiplication, so an
  extreme raw `-u`/`-g` unit length can no longer overflow into a panic
  (debug) or a silently wrong, tiny duration (release).
- `morse-cli`'s `--wpm`/`--farnsworth-wpm`/`-u`/`-g` flags are now
  validated: a non-numeric or out-of-range value is a usage error (clear
  message, non-zero exit) instead of being silently swapped for a default.
- Added `#![forbid(unsafe_code)]` to all three crates.
- CI now runs `cargo deny check` (advisories/bans/licenses/sources) on
  every push and PR, and gates the release job — previously `deny.toml`
  existed but was only ever invoked by an external, unversioned local
  script.

## [0.2.0] - 2026-07-17

### Added

- Restructured into a Cargo workspace: `morse-core` (shared logic),
  `morse-cli`, and `morse-gui`.
- `morse-gui`: cross-platform (Windows/macOS/Linux) desktop app built with
  `eframe`/`egui` — encode/decode fields, adjustable transmit speed, and a
  live flashing lamp synced to an audible tone (`rodio`) when transmitting.
- Multi-OS CI matrix (Ubuntu, macOS, Windows): format check, clippy
  (`-D warnings`), build, and test on every push/PR.
- Tagged-release workflow that builds and uploads per-OS binaries.
- Project documentation: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `SECURITY.md`, issue/PR templates.
- Punctuation support in the Morse table (`. , ? ' ! / ( ) & : ; = + - _ " $ @`).

### Changed

- `morse-core`'s encode/decode logic is now fully decoupled from I/O, so it
  is shared verbatim between the CLI and GUI.
- Rust edition 2021 → 2024.
- macOS release binaries are now universal (x86_64 + arm64 via `lipo`).
- Repository renamed `Morse-Code-Translator-` → `morse-code-translator`
  (GitHub redirects the old URL).

### Fixed

- `morse-gui` failed to compile on Windows (`eframe` 0.24 uses `winapi`
  features it doesn't declare) — fixed via an explicit `winapi` feature
  declaration; caught by the first real CI run.

## [0.1.0] - 2026-07-16

### Added

- Initial release: single-crate CLI with `encode`, `decode`, and
  `transmit` (terminal flash + bell) subcommands.
- Unit tests for encode/decode/timing.
- Basic CI (build + test), MIT license.
