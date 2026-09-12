# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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

- `morse-core`'s letter tables build once via `std::sync::LazyLock`
  instead of being reconstructed into a new `HashMap` on every
  `encode`/`decode`/`build_signal_plan` call.

### Security

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
