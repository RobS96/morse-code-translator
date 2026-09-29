# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
